mod commands;
mod errno;
mod error;
mod family;
#[cfg(feature = "hex")]
pub mod hex;
mod requests;
mod slip;
mod types;
mod utils;
mod writers;

use commands::{Request, TIMEOUT_FLASH_ERASE_PER_MB};
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

pub use errno::ErrorCode;
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

fn classify_serial_open_error(path: &str, e: serialport::Error) -> Error {
    use serialport::ErrorKind;
    match e.kind() {
        ErrorKind::NoDevice => Error::SerialNotFound(path.to_string()),
        ErrorKind::Io(io::ErrorKind::PermissionDenied) => {
            Error::SerialAccessDenied(path.to_string())
        }
        // serialport doesn't have a dedicated kind for "port busy" — both Linux
        // (EBUSY) and Windows (ERROR_ACCESS_DENIED with file in use) surface
        // platform-specific kinds. Probe the description as a fallback for the
        // common "already open" cases.
        _ if {
            let msg = e.to_string().to_lowercase();
            msg.contains("busy") || msg.contains("in use") || msg.contains("resource")
        } =>
        {
            Error::SerialBusy(path.to_string())
        }
        _ => Error::SerialOpenFailed {
            path: path.to_string(),
            source: e,
        },
    }
}

pub fn list_ports() -> Result<Vec<String>> {
    let ports = serialport::available_ports().map_err(|e| Error::SerialOpenFailed {
        path: "<enumeration>".to_string(),
        source: e,
    })?;
    Ok(ports
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
        .collect())
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
    flash_info: Option<FlashInfo>,
    chip_id: Option<ChipId>,
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
            .open()
            .map_err(|e| classify_serial_open_error(path, e))?;

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
            flash_info: None,
            chip_id: None,
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
            .reset(self.port.as_mut(), strategy, boot_mode, reset_interval)
            .map_err(Error::ResetPinFailed)?;
        Ok(())
    }

    /// Reset strategies known to work on production boards for this chip,
    /// ordered with the most common one first. Useful for callers that want
    /// to cycle through candidates across retries.
    pub fn reset_candidates(&self) -> &'static [ResetStrategy] {
        self.protocol.reset_candidates()
    }

    pub fn probe(&mut self, target: ProbeTarget, attempts: Option<usize>) -> Result<()> {
        if self.port.baud_rate()? != BAUD_RATE_DEFAULT {
            self.port.clear(ClearBuffer::All)?;
            self.port.set_baud_rate(BAUD_RATE_DEFAULT)?;
        }

        if self.protocol.rom_requires_adaptive_duplex() {
            let mode = self.adaptive_duplex()?;
            debug!("adaptive duplex mode: {:?}", mode);

            if mode == TransferMode::HalfDuplex {
                unimplemented!(
                    "Half-duplex transfer mode (CSK506x ROM) is not implemented in this build"
                );
            }

            sleep(Duration::from_millis(100));
        }

        // Initial sync. ROM stage uses 4001 (PROBE_NO_SYNC); burner stage
        // uses 5004 (BURNER_NO_RESPONSE).
        self.sync(attempts).map_err(|_| match target {
            ProbeTarget::ROM => Error::ProbeNoSync,
            ProbeTarget::Burner => Error::BurnerNoResponse,
        })?;

        if self.baud != BAUD_RATE_DEFAULT && self.protocol.rom_supports_change_baudrate() {
            // ChangeBaudrate command: ROM stage uses 5001, burner stage 5005.
            self.command::<()>(Request::ChangeBaudrate {
                to: self.baud,
                from: BAUD_RATE_DEFAULT,
            })
            .map_err(|_| match target {
                ProbeTarget::ROM => Error::RomBaudRejected,
                ProbeTarget::Burner => Error::BaudRejected,
            })?;
            self.port.clear(ClearBuffer::All)?;
            // ROM accepted the new baud, but the host driver may still refuse
            // it (e.g. macOS WCH driver rejects non-standard rates). That is
            // deterministic, not worth retrying — surface it distinctly so the
            // caller can bail instead of looping through reset attempts.
            self.port
                .set_baud_rate(self.baud)
                .map_err(|_| Error::SerialBaudUnsupported(self.baud))?;

            // Sync after baud change: ROM stage uses 5002, burner stage 5006.
            self.sync(attempts).map_err(|_| match target {
                ProbeTarget::ROM => Error::RomSyncLost,
                ProbeTarget::Burner => Error::BaudSyncLost,
            })?;
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

    /// Read the JEDEC flash info from the device. The result is cached on
    /// the first successful call; subsequent calls return the cached value.
    pub fn flash_info(&mut self) -> Result<FlashInfo> {
        if let Some(info) = self.flash_info {
            return Ok(info);
        }
        let info: FlashInfo = match self.command(Request::ReadFlashId) {
            Ok(info) => info,
            // FlashInfo's binread assert flags all-zero/all-FF/out-of-range
            // capacity bytes by translating to InvalidData. That's
            // semantically "no flash present" rather than a comms failure.
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::InvalidData => {
                return Err(Error::FlashNotDetected);
            }
            Err(e) => return Err(e.wrap_as(Error::FlashIdReadFailed)),
        };
        self.flash_info = Some(info);
        Ok(info)
    }

    /// Read the device's chip ID. The result is cached on the first
    /// successful call; subsequent calls return the cached value. Returns an
    /// empty ID on families that don't report one (e.g. MARS).
    pub fn chip_id(&mut self) -> Result<ChipId> {
        if let Some(id) = self.chip_id {
            return Ok(id);
        }
        let id = if self.chip == Family::MARS {
            ChipId::empty()
        } else {
            self.command(Request::ReadChipId)
                .map_err(|e| e.wrap_as(Error::ChipIdReadFailed))?
        };
        self.chip_id = Some(id);
        Ok(id)
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
        .map_err(|e| e.wrap_as(Error::VerifyReadFailed))
    }

    pub fn flash_erase(&mut self, target: EraseTarget) -> Result<()> {
        self.check_cancelled()?;
        match target {
            EraseTarget::Entire => {
                // Full-chip erase has no fixed duration — scale the read
                // timeout by the actual flash size. flash_info() caches its
                // result, so the round-trip is paid at most once.
                let flash_size = self.flash_info()?.size() as u32;
                let mb = (flash_size / 1024 / 1024).max(1);
                let timeout = TIMEOUT_FLASH_ERASE_PER_MB * mb;
                self.command_with_timeout(Request::EraseFlash, timeout)
                    .map_err(|e| e.wrap_as(Error::FlashEraseFailed))
            }
            EraseTarget::Region(region) => self
                .command(Request::EraseRegion {
                    offset: region.addr,
                    size: region.size,
                })
                .map_err(|e| e.wrap_as(Error::FlashEraseFailed)),
        }
    }
}
