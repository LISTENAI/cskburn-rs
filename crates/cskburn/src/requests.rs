use super::{
    Error, Result,
    commands::{Command, Request},
};
use binrw::{BinRead, BinWrite, binread, binwrite, io::NoSeek};
use log::{debug, trace};
use std::{
    cmp,
    fmt::Debug,
    io::{self, Cursor, Read, Write},
    time::Duration,
};

const TIMEOUT_WRITE_DEFAULT: Duration = Duration::from_millis(1000);
const TIMEOUT_READ_DEFAULT: Duration = Duration::from_millis(200);

#[binwrite]
#[bw(little, magic = 0x00u8)]
#[derive(Debug)]
struct RequestEnvelope {
    command: Command,
    #[bw(calc = data.len() as u16)]
    size: u16,
    checksum: u32,
    data: Vec<u8>,
}

#[binread]
#[br(little, magic = 0x01u8)]
#[derive(Debug)]
pub struct ResponseEnvelope {
    pub command: Command,
    #[br(temp)]
    size: u16,
    pub value: u32,
    pub code: u16,
    #[br(count = size - 2)]
    pub data: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferMode {
    FullDuplex,
    HalfDuplex,
}

impl TryFrom<ResponseEnvelope> for () {
    type Error = io::Error;

    fn try_from(_: ResponseEnvelope) -> io::Result<Self> {
        Ok(())
    }
}

impl TryFrom<ResponseEnvelope> for Vec<u8> {
    type Error = io::Error;

    fn try_from(res: ResponseEnvelope) -> io::Result<Self> {
        Ok(res.data)
    }
}

impl ResponseEnvelope {
    pub fn to_reader(&self) -> Cursor<Vec<u8>> {
        Cursor::new(self.data.to_owned())
    }
}

impl<const N: usize> TryFrom<ResponseEnvelope> for [u8; N] {
    type Error = io::Error;

    fn try_from(res: ResponseEnvelope) -> io::Result<Self> {
        if res.data.len() != N {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut data = [0; N];
        data.copy_from_slice(&res.data);

        Ok(data)
    }
}

impl super::CSKBurn {
    pub fn command<T>(&mut self, request: Request) -> Result<T>
    where
        T: TryFrom<ResponseEnvelope>,
    {
        trace!("{:02x?}", request);

        let read_timeout = cmp::max(
            request.timeout().unwrap_or(TIMEOUT_READ_DEFAULT),
            TIMEOUT_READ_DEFAULT,
        );

        let req: RequestEnvelope = RequestEnvelope {
            command: request.command(),
            checksum: request.checksum(),
            data: request.try_into()?,
        };

        debug!(
            "req: {:?}, checksum={:08x}, size={}",
            req.command,
            req.checksum,
            req.data.len()
        );

        self.with_timeout(TIMEOUT_WRITE_DEFAULT, |s| {
            let mut writer = NoSeek::new(s);
            req.write(&mut writer)?;
            writer.flush()?;
            Ok(())
        })?;

        let res = self.with_timeout(read_timeout, |s| {
            let mut reader = NoSeek::new(s);
            let res = ResponseEnvelope::read(&mut reader)?;
            Ok(res)
        })?;

        debug!(
            "res: {:?}, value={}, code={:04x}, size={}",
            res.command,
            res.value,
            res.code,
            res.data.len()
        );
        trace!("{:02x?}", res);

        if res.code != 0 {
            if res.code & 0xff == 0x01 {
                return Err(Error::Burner((res.code >> 8) as u8));
            } else {
                return Err(Error::Burner((res.code & 0xff) as u8));
            }
        }

        T::try_from(res).map_err(|_| io::ErrorKind::InvalidData.into())
    }

    pub fn adaptive_duplex(&mut self) -> Result<TransferMode> {
        self.port.clear(serialport::ClearBuffer::All)?;

        debug!("detecting transfer mode...");

        self.with_timeout(TIMEOUT_WRITE_DEFAULT, |s| {
            s.port.write(&[0xFF, 0xF0, 0x00])?;
            s.port.flush()?;
            Ok(())
        })?;

        let mode = self.with_timeout(TIMEOUT_READ_DEFAULT, |s| {
            let mut res = vec![0; 3];
            if s.port.read(&mut res).is_ok() && res == [0xFF, 0xF0, 0x00] {
                Ok(TransferMode::HalfDuplex)
            } else {
                Ok(TransferMode::FullDuplex)
            }
        })?;

        debug!("detected transfer mode, mode={:?}", mode);

        Ok(mode)
    }

    fn with_timeout<F, T>(&mut self, timeout: Duration, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self) -> Result<T>,
    {
        self.port.set_timeout(timeout)?;
        f(self)
    }
}
