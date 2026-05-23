//! Stable numeric error codes for surfacing failures to GUI tools and shell
//! scripts. Each variant maps to exactly one root cause at the software
//! level. Codes are assigned in ranges so front-ends can route them to
//! detailed guidance without parsing text.
//!
//! Human-readable messages live on the corresponding [`Error`](crate::Error)
//! variant's `Display` impl, not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ErrorCode {
    // 1xxx — CLI / argument validation
    ArgInvalid = 1001,
    ArgAddrUnaligned = 1005,
    ArgAddrOutOfBounds = 1006,

    // 2xxx — local file I/O
    FileReadFailed = 2001,
    FileWriteFailed = 2002,
    HexParseFailed = 2003,

    // 3xxx — serial port open/config
    SerialNotFound = 3001,
    SerialAccessDenied = 3002,
    SerialBusy = 3003,
    SerialOpenFailed = 3004,
    SerialBaudUnsupported = 3006,

    // 4xxx — probe / reset
    ProbeNoSync = 4001,
    ResetPinFailed = 4002,

    // 5xxx — entering update mode
    RomBaudRejected = 5001,
    RomSyncLost = 5002,
    BurnerLoadFailed = 5003,
    BurnerNoResponse = 5004,
    BaudRejected = 5005,
    BaudSyncLost = 5006,

    // 6xxx — device / flash info
    ChipIdReadFailed = 6001,
    FlashIdReadFailed = 6002,
    FlashNotDetected = 6003,

    // 7xxx — flash operations
    FlashReadFailed = 7001,
    FlashEraseFailed = 7002,
    FlashWriteFailed = 7003,
    RamWriteFailed = 7005,

    // 8xxx — verification
    VerifyReadFailed = 8001,
    VerifyLocalMd5Failed = 8002,
    VerifyMismatch = 8003,
}

impl ErrorCode {
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}
