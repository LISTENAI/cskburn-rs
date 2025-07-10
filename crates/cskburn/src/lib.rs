mod commands;
mod error;
mod requests;
mod slip;
mod types;
mod utils;

use commands::{ChipId, FlashInfo, MemoryAction, Request};
use log::{debug, trace};
use serialport::SerialPort;
use slip_codec::{SlipDecoder, SlipEncoder};
use std::{
    io::{self, Cursor},
    thread::sleep,
    time::Duration,
};

pub use error::Error;
pub use types::{family::Family, image::Image, region::Region, source::Source};

use crate::requests::TransferMode;

type Result<T> = std::result::Result<T, Error>;

const BAUD_RATE_DEFAULT: u32 = 115200;

const MEMORY_BLOCK_SIZE: usize = 2048;
const FLASH_BLOCK_SIZE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSKBurnBuilder {
    path: String,
    baud: u32,
    chip: Family,
}

pub fn list_ports() -> Result<Vec<String>> {
    serialport::available_ports()
        .map_err(Error::from)
        .map(|ports| {
            ports
                .into_iter()
                .filter_map(|port| {
                    if cfg!(target_os = "macos") && !port.port_name.starts_with("/dev/cu.") {
                        return None;
                    }

                    if let serialport::SerialPortType::UsbPort(_) = port.port_type {
                        Some(port.port_name)
                    } else {
                        None
                    }
                })
                .collect()
        })
}

pub fn new(path: String, baud: u32, chip: Family) -> CSKBurnBuilder {
    CSKBurnBuilder { path, baud, chip }
}

impl CSKBurnBuilder {
    pub fn open(self) -> Result<CSKBurn> {
        let port = serialport::new(self.path, BAUD_RATE_DEFAULT)
            .flow_control(serialport::FlowControl::None)
            .dtr_on_open(false)
            .open()?;

        Ok(CSKBurn {
            port,
            baud: self.baud,
            chip: self.chip,
            slip_enc: SlipEncoder::new(true),
            slip_enc_buf: Cursor::new(Vec::new()),
            slip_dec: SlipDecoder::new(),
            slip_dec_buf: Cursor::new(Vec::new()),
        })
    }
}

pub struct CSKBurn {
    port: Box<dyn SerialPort>,
    baud: u32,
    chip: Family,
    slip_enc: SlipEncoder,
    slip_enc_buf: Cursor<Vec<u8>>,
    slip_dec: SlipDecoder,
    slip_dec_buf: Cursor<Vec<u8>>,
}

#[derive(PartialEq)]
pub enum ProbeTarget {
    ROM,
    Burner,
}

pub enum EraseTarget {
    Entire,
    Region(Region),
}

impl CSKBurn {
    pub fn reset(&mut self, boot_mode: bool, reset_interval: Option<Duration>) -> Result<()> {
        let reset_interval = reset_interval.unwrap_or(Duration::from_millis(100));

        if self.chip == Family::MARS {
            trace!("set DTR: {}", true);
            self.port.write_data_terminal_ready(true)?;

            sleep(if boot_mode {
                Duration::from_secs(2)
            } else {
                reset_interval
            });

            trace!("set DTR: {}", false);
            self.port.write_data_terminal_ready(false)?;

            sleep(reset_interval);
        } else {
            trace!("set RTS: {}", boot_mode);
            self.port.write_request_to_send(boot_mode)?;

            trace!("set DTR: {}", true);
            self.port.write_data_terminal_ready(true)?;

            sleep(reset_interval);

            trace!("set DTR: {}", false);
            self.port.write_data_terminal_ready(false)?;

            sleep(reset_interval);
        }

        Ok(())
    }

    pub fn probe(&mut self, target: ProbeTarget, attempts: Option<usize>) -> Result<()> {
        if target == ProbeTarget::Burner && self.port.baud_rate()? != BAUD_RATE_DEFAULT {
            self.port.clear(serialport::ClearBuffer::All)?;
            self.port.set_baud_rate(BAUD_RATE_DEFAULT)?;
        }

        if self.chip == Family::MARS {
            let mode = self.adaptive_duplex()?;
            debug!("adaptive duplex mode: {:?}", mode);

            if mode == TransferMode::HalfDuplex {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Half-duplex mode is not currently support in this version",
                )));
            }

            sleep(Duration::from_millis(100));
        }

        self.sync(attempts)?;

        if self.baud != BAUD_RATE_DEFAULT && self.chip.rom_supports_change_baudrate() {
            let _: () = self.command(Request::ChangeBaudrate {
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
        if self.chip == Family::MARS {
            return Ok(ChipId::empty());
        }

        self.command(Request::ReadChipId)
    }

    pub fn memory_write<F>(
        &mut self,
        source: &mut Image,
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

        let _: () = self.command(Request::MemoryBegin {
            size: size as u32,
            blocks: blocks as u32,
            block_size: MEMORY_BLOCK_SIZE as u32,
            offset: source.addr,
        })?;

        let mut buf = vec![0; MEMORY_BLOCK_SIZE];
        let mut written = 0;
        for seq in 0..blocks as u32 {
            let read = source.source.read(&mut buf)?;
            if read == 0 {
                break;
            }

            let _: () = self.command(Request::MemoryData {
                seq,
                data: buf[..read].to_vec(),
            })?;

            written += read;
            if let Some(ref mut on_progress) = on_progress {
                on_progress(written, size);
            }
        }

        let _: () = self.command(Request::MemoryEnd {
            action: action.unwrap_or(MemoryAction::Boot()),
        })?;

        Ok(size)
    }

    pub fn flash_write<F>(
        &mut self,
        source: &mut Image,
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

        let _: () = self.command(Request::FlashBegin {
            size: size as u32,
            blocks: blocks as u32,
            block_size: FLASH_BLOCK_SIZE as u32,
            offset: source.addr,
        })?;

        let mut buf = vec![0; FLASH_BLOCK_SIZE];
        let mut written = 0;
        for seq in 0..blocks as u32 {
            let read = source.source.read(&mut buf)?;
            if read == 0 {
                break;
            }

            let _: () = self.command(Request::FlashData {
                seq,
                data: buf[..read].to_vec(),
            })?;

            written += read;
            if let Some(ref mut on_progress) = on_progress {
                on_progress(written, size);
            }
        }

        let _: () = self.command(Request::FlashEnd {})?;

        Ok(size)
    }

    pub fn flash_verify(&mut self, region: Region) -> Result<[u8; 16]> {
        self.command(Request::FlashMd5 {
            offset: region.addr,
            size: region.size,
        })
    }

    pub fn flash_erase(&mut self, target: EraseTarget) -> Result<()> {
        match target {
            EraseTarget::Entire => self.command(Request::EraseFlash),
            EraseTarget::Region(region) => self.command(Request::EraseRegion {
                offset: region.addr,
                size: region.size,
            }),
        }
    }
}
