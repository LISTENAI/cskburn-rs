use std::io;

#[derive(Debug)]
pub enum ErrorKind {
    Burner(u8),
    SerialPort(serialport::Error),
    BinRW(binrw::Error),
    Io(io::Error),
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
}

impl Error {
    pub fn burner(code: u8) -> Self {
        Self {
            kind: ErrorKind::Burner(code),
        }
    }

    pub fn io(kind: io::ErrorKind) -> Self {
        Self {
            kind: ErrorKind::Io(io::Error::from(kind)),
        }
    }
}

impl From<serialport::Error> for Error {
    fn from(err: serialport::Error) -> Self {
        Self {
            kind: ErrorKind::SerialPort(err),
        }
    }
}

impl From<binrw::Error> for Error {
    fn from(err: binrw::Error) -> Self {
        match err {
            binrw::Error::Io(err) => Self {
                kind: ErrorKind::Io(err),
            },
            _ => Self {
                kind: ErrorKind::BinRW(err),
            },
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io(err),
        }
    }
}

impl From<io::ErrorKind> for Error {
    fn from(err: io::ErrorKind) -> Self {
        Self {
            kind: ErrorKind::Io(io::Error::from(err)),
        }
    }
}
