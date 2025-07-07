use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("error response from burner: {0:02x?}")]
    Burner(u8),
    #[error(transparent)]
    SerialPort(#[from] serialport::Error),
    #[error(transparent)]
    BinRW(#[from] binrw::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<io::ErrorKind> for Error {
    fn from(err: io::ErrorKind) -> Self {
        Self::Io(err.into())
    }
}
