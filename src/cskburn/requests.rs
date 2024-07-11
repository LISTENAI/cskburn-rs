use super::{
    commands::{Command, Request},
    Error, Result,
};
use binrw::{binread, binwrite, BinRead, BinWrite};
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

        let mut writer = Cursor::new(Vec::new());
        req.write(&mut writer)?;

        self.port.set_timeout(TIMEOUT_WRITE_DEFAULT)?;
        self.write(writer.get_ref())?;

        self.flush()?;

        let mut reader = vec![0; 1024];

        self.port.set_timeout(read_timeout)?;
        self.read(&mut reader)?;

        let res = ResponseEnvelope::read(&mut Cursor::new(reader))?;

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
                return Err(Error::burner((res.code >> 8) as u8));
            } else {
                return Err(Error::burner((res.code & 0xff) as u8));
            }
        }

        T::try_from(res).map_err(|_| Error::io(io::ErrorKind::InvalidData))
    }
}
