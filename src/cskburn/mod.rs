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

const MEMORY_BLOCK_SIZE: usize = 2048;

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
        let port = serialport::new(self.path, 115200)
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

    pub fn probe(&mut self, attempts: Option<usize>) -> Result<()> {
        for _ in 0..attempts.unwrap_or(1) {
            if self.command::<()>(Request::Sync(), None).is_ok() {
                return Ok(());
            }
        }

        Err(Error::io(io::ErrorKind::TimedOut))
    }

    pub fn flash_info(&mut self) -> Result<FlashInfo> {
        self.command(Request::ReadFlashId, None)
    }

    pub fn chip_id(&mut self) -> Result<ChipId> {
        self.command(Request::ReadChipId, None)
    }

    pub fn memory_write(
        &mut self,
        offset: u32,
        source: &mut dyn Source,
        action: Option<MemoryAction>,
    ) -> Result<usize> {
        let size: usize = source.size()?;
        let blocks = utils::blocks(size, MEMORY_BLOCK_SIZE);
        debug!(
            "begin memory write, total size: {}, blocks: {}",
            size, blocks
        );

        self.command(
            Request::MemoryBegin {
                size: size as u32,
                blocks: blocks as u32,
                block_size: MEMORY_BLOCK_SIZE as u32,
                offset,
            },
            None,
        )?;

        let mut buf = vec![0; MEMORY_BLOCK_SIZE];
        for seq in 0..blocks as u32 {
            let read = source.read(&mut buf)?;
            if read == 0 {
                break;
            }

            self.command(
                Request::MemoryData {
                    seq,
                    data: buf[..read].to_vec(),
                },
                None,
            )?;
        }

        self.command(
            Request::MemoryEnd {
                action: action.unwrap_or(MemoryAction::Boot()),
            },
            None,
        )?;

        Ok(size)
    }
}
