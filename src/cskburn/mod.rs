mod commands;
mod error;
mod family;
mod requests;
mod slip;
mod utils;

use commands::{ChipId, FlashInfo, MemoryAction, Request};
use log::{debug, trace};
use serialport::SerialPort;
use slip_codec::{SlipDecoder, SlipEncoder};
use std::{
    fs::File,
    io::{self, Cursor, Read},
    thread::sleep,
    time::Duration,
};

pub use error::Error;
pub use family::Family;

type Result<T> = std::result::Result<T, Error>;

const BAUD_RATE_DEFAULT: u32 = 115200;

const MEMORY_BLOCK_SIZE: usize = 2048;
const FLASH_BLOCK_SIZE: usize = 4096;

pub trait Source: Read {
    fn size(&self) -> io::Result<usize>;
}

impl Source for File {
    fn size(&self) -> io::Result<usize> {
        self.metadata().map(|m| m.len() as usize)
    }
}

impl<T: AsRef<[u8]>> Source for Cursor<T> {
    fn size(&self) -> io::Result<usize> {
        Ok(self.get_ref().as_ref().len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSKBurnBuilder {
    path: String,
    baud: u32,
    chip: Family,
}

pub fn new(path: String, baud: u32, chip: Family) -> CSKBurnBuilder {
    CSKBurnBuilder { path, baud, chip }
}

impl CSKBurnBuilder {
    pub fn open(self) -> Result<CSKBurn> {
        let port = serialport::new(self.path, BAUD_RATE_DEFAULT)
            .flow_control(serialport::FlowControl::None)
            .open()?;

        Ok(CSKBurn {
            port,
            baud: self.baud,
            chip: self.chip,
            slip_enc: SlipEncoder::new(true),
            slip_dec: SlipDecoder::new(),
        })
    }
}

pub struct CSKBurn {
    port: Box<dyn SerialPort>,
    baud: u32,
    chip: Family,
    slip_enc: SlipEncoder,
    slip_dec: SlipDecoder,
}

#[derive(PartialEq)]
pub enum ProbeTarget {
    ROM,
    Burner,
}

pub enum EraseTarget {
    Entire,
    Region { offset: u32, size: u32 },
}

impl CSKBurn {
    pub fn reset(&mut self, boot_mode: bool, reset_interval: Option<Duration>) -> Result<()> {
        trace!("set RTS: {}", boot_mode);
        self.port.write_request_to_send(boot_mode)?;

        trace!("set DTR: {}", true);
        self.port.write_data_terminal_ready(true)?;

        sleep(reset_interval.unwrap_or(Duration::from_millis(100)));

        trace!("set DTR: {}", false);
        self.port.write_data_terminal_ready(false)?;

        Ok(())
    }

    pub fn probe(&mut self, target: ProbeTarget, attempts: Option<usize>) -> Result<()> {
        if target == ProbeTarget::Burner && self.port.baud_rate()? != BAUD_RATE_DEFAULT {
            self.port.clear(serialport::ClearBuffer::All)?;
            self.port.set_baud_rate(BAUD_RATE_DEFAULT)?;
        }

        self.sync(attempts)?;

        if self.baud != BAUD_RATE_DEFAULT && self.chip.rom_supports_change_baudrate() {
            self.command(Request::ChangeBaudrate {
                to: self.baud,
                from: BAUD_RATE_DEFAULT,
            })?;
            self.port.clear(serialport::ClearBuffer::All)?;
            self.port.set_baud_rate(self.baud)?;

            self.sync(attempts)?;
        }

        Ok(())
    }

    fn sync(&mut self, attempts: Option<usize>) -> Result<()> {
        for _ in 0..attempts.unwrap_or(1) {
            if self.command::<()>(Request::Sync()).is_ok() {
                return Ok(());
            }
        }

        Err(io::ErrorKind::TimedOut.into())
    }

    pub fn flash_info(&mut self) -> Result<FlashInfo> {
        self.command(Request::ReadFlashId)
    }

    pub fn chip_id(&mut self) -> Result<ChipId> {
        self.command(Request::ReadChipId)
    }

    pub fn memory_write<F>(
        &mut self,
        offset: u32,
        source: &mut dyn Source,
        action: Option<MemoryAction>,
        mut on_progress: Option<F>,
    ) -> Result<usize>
    where
        F: FnMut(usize, usize),
    {
        let size: usize = source.size()?;
        let blocks = utils::blocks(size, MEMORY_BLOCK_SIZE);
        debug!(
            "begin memory write, total size: {}, blocks: {}",
            size, blocks
        );

        self.command(Request::MemoryBegin {
            size: size as u32,
            blocks: blocks as u32,
            block_size: MEMORY_BLOCK_SIZE as u32,
            offset,
        })?;

        let mut buf = vec![0; MEMORY_BLOCK_SIZE];
        let mut written = 0;
        for seq in 0..blocks as u32 {
            let read = source.read(&mut buf)?;
            if read == 0 {
                break;
            }

            self.command(Request::MemoryData {
                seq,
                data: buf[..read].to_vec(),
            })?;

            written += read;
            if let Some(ref mut on_progress) = on_progress {
                on_progress(written, size);
            }
        }

        self.command(Request::MemoryEnd {
            action: action.unwrap_or(MemoryAction::Boot()),
        })?;

        Ok(size)
    }

    pub fn flash_write<F>(
        &mut self,
        offset: u32,
        source: &mut dyn Source,
        mut on_progress: Option<F>,
    ) -> Result<usize>
    where
        F: FnMut(usize, usize),
    {
        let size: usize = source.size()?;
        let blocks = utils::blocks(size, FLASH_BLOCK_SIZE);
        debug!(
            "begin flash write, total size: {}, blocks: {}",
            size, blocks
        );

        self.command(Request::FlashBegin {
            size: size as u32,
            blocks: blocks as u32,
            block_size: FLASH_BLOCK_SIZE as u32,
            offset,
        })?;

        let mut buf = vec![0; FLASH_BLOCK_SIZE];
        let mut written = 0;
        for seq in 0..blocks as u32 {
            let read = source.read(&mut buf)?;
            if read == 0 {
                break;
            }

            self.command(Request::FlashData {
                seq,
                data: buf[..read].to_vec(),
            })?;

            written += read;
            if let Some(ref mut on_progress) = on_progress {
                on_progress(written, size);
            }
        }

        self.command(Request::FlashEnd {})?;

        Ok(size)
    }

    pub fn flash_verify(&mut self, offset: u32, size: u32) -> Result<[u8; 16]> {
        self.command(Request::FlashMd5 { offset, size })
    }

    pub fn flash_erase(&mut self, target: EraseTarget) -> Result<()> {
        match target {
            EraseTarget::Entire => self.command(Request::EraseFlash),
            EraseTarget::Region { offset, size } => {
                self.command(Request::EraseRegion { offset, size })
            }
        }
    }
}
