use std::io;
use thiserror::Error;

use crate::errno::ErrorCode;

/// Library error type. Every variant has a stable [`ErrorCode`] via
/// [`Error::code`] and a self-contained `Display` message — front-ends can
/// surface failures as `[E####]: {error}` without extra plumbing.
///
/// Raw transport errors (`io::Error`, `serialport::Error`, `binrw::Error`)
/// are wrapped at the public-API boundary into a semantic variant; the
/// hidden `Io` / `SerialPort` / `BinRW` / `Burner` variants exist only so
/// internal code can use `?` before that wrapping happens.
#[derive(Error, Debug)]
pub enum Error {
    // 1xxx — argument validation
    #[error("Invalid argument: {0}")]
    ArgInvalid(String),

    #[error("Address 0x{addr:08x} of {label} should be aligned to {align:#x}")]
    ArgAddrUnaligned {
        addr: u32,
        align: u32,
        label: String,
    },

    #[error("Address 0x{addr:08x}+{size:#x} of {label} exceeds capacity ({capacity_mib} MiB)")]
    ArgAddrOutOfBounds {
        addr: u32,
        size: u32,
        capacity_mib: u64,
        label: String,
    },

    // 2xxx — local file I/O
    #[error("Failed to read input file {path}: {source}")]
    FileReadFailed {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Failed to write output file {path}: {source}")]
    FileWriteFailed {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Failed to parse HEX file: {0}")]
    HexParseFailed(String),

    // 3xxx — serial port
    #[error("Serial port not found: {0}")]
    SerialNotFound(String),

    #[error("Permission denied to access serial port: {0}")]
    SerialAccessDenied(String),

    #[error("Serial port is already in use: {0}")]
    SerialBusy(String),

    #[error("Failed to open serial port {path}: {source}")]
    SerialOpenFailed {
        path: String,
        #[source]
        source: serialport::Error,
    },

    #[error("Serial driver does not support baud rate {0} bps")]
    SerialBaudUnsupported(u32),

    // 4xxx — probe / reset
    #[error("Device did not respond after reset (check wiring and BOOT pin)")]
    ProbeNoSync,

    #[error("Failed to toggle RTS/DTR lines for reset: {0}")]
    ResetPinFailed(#[source] io::Error),

    // 5xxx — entering update mode
    #[error("ROM rejected baud rate change")]
    RomBaudRejected,

    #[error("Lost sync with ROM after baud rate change")]
    RomSyncLost,

    #[error("Failed to load RAM burner: {0}")]
    BurnerLoadFailed(#[source] Box<Error>),

    #[error("Burner did not start after being loaded")]
    BurnerNoResponse,

    #[error("Burner rejected baud rate change")]
    BaudRejected,

    #[error("Lost sync with burner after baud rate change")]
    BaudSyncLost,

    // 6xxx — device / flash info
    #[error("Failed to read chip ID: {0}")]
    ChipIdReadFailed(#[source] Box<Error>),

    #[error("Failed to read flash ID: {0}")]
    FlashIdReadFailed(#[source] Box<Error>),

    #[error("No flash detected (ID reads as all zeros or all ones)")]
    FlashNotDetected,

    // 7xxx — flash operations
    #[error("Failed to read flash: {0}")]
    FlashReadFailed(#[source] Box<Error>),

    #[error("Failed to erase flash: {0}")]
    FlashEraseFailed(#[source] Box<Error>),

    #[error("Failed to write flash: {0}")]
    FlashWriteFailed(#[source] Box<Error>),

    #[error("Failed to write RAM: {0}")]
    RamWriteFailed(#[source] Box<Error>),

    // 8xxx — verification
    #[error("Failed to compute MD5 on device: {0}")]
    VerifyReadFailed(#[source] Box<Error>),

    #[error("Failed to compute MD5 on local file {path}: {source}")]
    VerifyLocalMd5Failed {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Verification failed: expected {expected:02x?}, got {actual:02x?}")]
    VerifyMismatch {
        expected: [u8; 16],
        actual: [u8; 16],
    },

    /// Cooperative cancellation. Not an error per se — propagated through
    /// `Result` so internal loops can short-circuit. Callers should treat
    /// this as a normal early exit, not a failure.
    #[error("Operation cancelled")]
    Cancelled,

    // ---- Internal error sources ----
    // These exist so internal code can use `?` to bubble up raw errors before
    // the public-API boundary wraps them into a semantic variant. They should
    // not appear in errors returned from public APIs in practice; if one does
    // slip through, .code() returns `None` so callers know it's unmapped.
    #[doc(hidden)]
    #[error(transparent)]
    Io(#[from] io::Error),

    #[doc(hidden)]
    #[error(transparent)]
    SerialPort(#[from] serialport::Error),

    #[doc(hidden)]
    #[error(transparent)]
    BinRW(#[from] binrw::Error),

    #[doc(hidden)]
    #[error("Error response from burner: 0x{0:02x}")]
    Burner(u8),
}

impl Error {
    /// The numeric error code for this error, if it's been categorized.
    /// Returns `None` for [`Cancelled`] and unmapped internal errors.
    pub fn code(&self) -> Option<ErrorCode> {
        Some(match self {
            Error::ArgInvalid(_) => ErrorCode::ArgInvalid,
            Error::ArgAddrUnaligned { .. } => ErrorCode::ArgAddrUnaligned,
            Error::ArgAddrOutOfBounds { .. } => ErrorCode::ArgAddrOutOfBounds,
            Error::FileReadFailed { .. } => ErrorCode::FileReadFailed,
            Error::FileWriteFailed { .. } => ErrorCode::FileWriteFailed,
            Error::HexParseFailed(_) => ErrorCode::HexParseFailed,
            Error::SerialNotFound(_) => ErrorCode::SerialNotFound,
            Error::SerialAccessDenied(_) => ErrorCode::SerialAccessDenied,
            Error::SerialBusy(_) => ErrorCode::SerialBusy,
            Error::SerialOpenFailed { .. } => ErrorCode::SerialOpenFailed,
            Error::SerialBaudUnsupported(_) => ErrorCode::SerialBaudUnsupported,
            Error::ProbeNoSync => ErrorCode::ProbeNoSync,
            Error::ResetPinFailed(_) => ErrorCode::ResetPinFailed,
            Error::RomBaudRejected => ErrorCode::RomBaudRejected,
            Error::RomSyncLost => ErrorCode::RomSyncLost,
            Error::BurnerLoadFailed(_) => ErrorCode::BurnerLoadFailed,
            Error::BurnerNoResponse => ErrorCode::BurnerNoResponse,
            Error::BaudRejected => ErrorCode::BaudRejected,
            Error::BaudSyncLost => ErrorCode::BaudSyncLost,
            Error::ChipIdReadFailed(_) => ErrorCode::ChipIdReadFailed,
            Error::FlashIdReadFailed(_) => ErrorCode::FlashIdReadFailed,
            Error::FlashNotDetected => ErrorCode::FlashNotDetected,
            Error::FlashReadFailed(_) => ErrorCode::FlashReadFailed,
            Error::FlashEraseFailed(_) => ErrorCode::FlashEraseFailed,
            Error::FlashWriteFailed(_) => ErrorCode::FlashWriteFailed,
            Error::RamWriteFailed(_) => ErrorCode::RamWriteFailed,
            Error::VerifyReadFailed(_) => ErrorCode::VerifyReadFailed,
            Error::VerifyLocalMd5Failed { .. } => ErrorCode::VerifyLocalMd5Failed,
            Error::VerifyMismatch { .. } => ErrorCode::VerifyMismatch,
            Error::Cancelled
            | Error::Io(_)
            | Error::SerialPort(_)
            | Error::BinRW(_)
            | Error::Burner(_) => return None,
        })
    }

    /// Wrap this error in the given semantic variant, preserving the original
    /// as a source. [`Cancelled`] passes through unchanged so cancellation
    /// stays distinguishable at the call site.
    pub(crate) fn wrap_as<F>(self, wrap: F) -> Self
    where
        F: FnOnce(Box<Error>) -> Error,
    {
        match self {
            Error::Cancelled => Error::Cancelled,
            other => wrap(Box::new(other)),
        }
    }

    pub fn is_timeout(&self) -> bool {
        match self {
            Error::Io(e) => e.kind() == io::ErrorKind::TimedOut,
            Error::SerialPort(e) => e.kind() == serialport::ErrorKind::Io(io::ErrorKind::TimedOut),
            _ => false,
        }
    }

    /// Whether retrying the operation could plausibly succeed. Deterministic
    /// failures (e.g. host driver rejecting a baud rate) should not be retried.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Error::SerialBaudUnsupported(_) | Error::Cancelled)
    }
}

impl From<io::ErrorKind> for Error {
    fn from(err: io::ErrorKind) -> Self {
        Self::Io(err.into())
    }
}
