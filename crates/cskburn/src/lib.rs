mod commands;
mod error;
mod family;
mod requests;
mod slip;
mod types;
mod utils;
mod writers;

use commands::{ChipId, FlashInfo, Request};
use log::debug;
use serialport::{ClearBuffer, SerialPort};
use slip_codec::{SlipDecoder, SlipEncoder};
use std::{
    io::{self, Cursor},
    thread::sleep,
    time::Duration,
};

pub use error::Error;
pub use family::Family;
pub use types::{image::Image, region::Region, source::Source};

use crate::{family::ChipProtocol, requests::TransferMode, writers::WriteIterator};

pub use crate::writers::WriteTarget;

pub type Result<T> = std::result::Result<T, Error>;

const BAUD_RATE_DEFAULT: u32 = 115200;

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

        let protocol = self.chip.protocol();

        Ok(CSKBurn {
            port,
            baud: self.baud,
            chip: self.chip,
            protocol,
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
    protocol: Box<dyn ChipProtocol>,
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
        self.protocol
            .reset(self.port.as_mut(), boot_mode, reset_interval)?;
        Ok(())
    }

    pub fn probe(&mut self, target: ProbeTarget, attempts: Option<usize>) -> Result<()> {
        if target == ProbeTarget::Burner && self.port.baud_rate()? != BAUD_RATE_DEFAULT {
            self.port.clear(ClearBuffer::All)?;
            self.port.set_baud_rate(BAUD_RATE_DEFAULT)?;
        }

        if self.protocol.rom_requires_adaptive_duplex() {
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

        if self.baud != BAUD_RATE_DEFAULT && self.protocol.rom_supports_change_baudrate() {
            let _: () = self.command(Request::ChangeBaudrate {
                to: self.baud,
                from: BAUD_RATE_DEFAULT,
            })?;
            self.port.clear(ClearBuffer::All)?;
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

    pub fn write(&mut self, source: &mut Image, target: WriteTarget) -> Result<usize> {
        let mut total_written = 0;
        for step in self.write_iter(source, target)? {
            let step = step?;
            total_written = step.bytes_written;
        }
        Ok(total_written)
    }

    pub fn write_iter<'a>(
        &'a mut self,
        source: &'a mut Image,
        target: WriteTarget,
    ) -> Result<WriteIterator<'a>> {
        WriteIterator::new(self, source, target)
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
