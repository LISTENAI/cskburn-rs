mod commands;
mod error;
mod family;
#[cfg(feature = "hex")]
pub mod hex;
mod requests;
mod slip;
mod types;
mod utils;
mod writers;

use commands::Request;
use log::debug;
use serialport::{ClearBuffer, SerialPort};
use slip_codec::{SlipDecoder, SlipEncoder};
use std::{
    io::{self, Cursor},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::sleep,
    time::Duration,
};

pub use error::Error;
pub use family::{Family, ResetStrategy};
pub use types::{image::Image, region::Region, source::Source};

use crate::{family::ChipProtocol, requests::TransferMode};

pub use crate::commands::{ChipId, FlashInfo, MemoryAction};
pub use crate::writers::{WriteIterator, WriteStep, WriteTarget};

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

pub struct CSKBurn {
    port: Box<dyn SerialPort>,
    baud: u32,
    chip: Family,
    protocol: Box<dyn ChipProtocol>,
    slip_enc: SlipEncoder,
    slip_enc_buf: Cursor<Vec<u8>>,
    slip_dec: SlipDecoder,
    slip_dec_buf: Cursor<Vec<u8>>,
    cancel: Option<Arc<AtomicBool>>,
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
    pub fn set_cancel_token(&mut self, token: Arc<AtomicBool>) {
        self.cancel = Some(token);
    }

    pub(crate) fn check_cancelled(&self) -> Result<()> {
        if let Some(ref token) = self.cancel {
            if token.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
        }
        Ok(())
    }

    pub fn connect(path: &str, baud: u32, chip: Family) -> Result<Self> {
        let port = serialport::new(path, BAUD_RATE_DEFAULT)
            .flow_control(serialport::FlowControl::None)
            .dtr_on_open(false)
            .open()?;

        let protocol = chip.protocol();

        Ok(CSKBurn {
            port,
            baud,
            chip,
            protocol,
            slip_enc: SlipEncoder::new(true),
            slip_enc_buf: Cursor::new(Vec::new()),
            slip_dec: SlipDecoder::new(),
            slip_dec_buf: Cursor::new(Vec::new()),
            cancel: None,
        })
    }

    /// Reset the device. Pass `None` for the strategy to use the chip's
    /// default (first candidate); pass `Some(...)` to force a specific reset
    /// circuit topology.
    pub fn reset(
        &mut self,
        strategy: Option<ResetStrategy>,
        boot_mode: bool,
        reset_interval: Option<Duration>,
    ) -> Result<()> {
        let strategy = strategy.unwrap_or_else(|| self.protocol.reset_candidates()[0]);
        self.protocol
            .reset(self.port.as_mut(), strategy, boot_mode, reset_interval)?;
        Ok(())
    }

    /// Reset strategies known to work on production boards for this chip,
    /// ordered with the most common one first. Useful for callers that want
    /// to cycle through candidates across retries.
    pub fn reset_candidates(&self) -> &'static [ResetStrategy] {
        self.protocol.reset_candidates()
    }

    pub fn probe(&mut self, _target: ProbeTarget, attempts: Option<usize>) -> Result<()> {
        if self.port.baud_rate()? != BAUD_RATE_DEFAULT {
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
            // ROM accepted the new baud, but the host driver may still refuse
            // it (e.g. macOS WCH driver rejects non-standard rates). That is
            // deterministic, not worth retrying — surface it distinctly so the
            // caller can bail instead of looping through reset attempts.
            self.port
                .set_baud_rate(self.baud)
                .map_err(|_| Error::BaudRateUnsupported(self.baud))?;

            self.sync(attempts)?;
        }

        Ok(())
    }

    fn sync(&mut self, attempts: Option<usize>) -> Result<()> {
        for _ in 0..attempts.unwrap_or(1) {
            self.check_cancelled()?;
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
        self.check_cancelled()?;
        self.command(Request::FlashMd5 {
            offset: region.addr,
            size: region.size,
        })
    }

    pub fn flash_erase(&mut self, target: EraseTarget) -> Result<()> {
        self.check_cancelled()?;
        match target {
            EraseTarget::Entire => self.command(Request::EraseFlash),
            EraseTarget::Region(region) => self.command(Request::EraseRegion {
                offset: region.addr,
                size: region.size,
            }),
        }
    }
}
