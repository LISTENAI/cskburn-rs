use super::{requests::ResponseEnvelope, utils};
use binrw::{binread, binrw, binwrite, BinRead, BinWrite};
use std::{
    fmt::{self, Debug},
    io::{self, Cursor},
};

const SYNC_STUB: [u8; 36] = [
    0x07, 0x07, 0x12, 0x20, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55,
];

#[binrw]
#[brw(repr = u8)]
#[derive(Debug)]
pub enum Command {
    FlashBegin = 0x02,
    FlashData = 0x03,
    FlashEnd = 0x04,
    MemoryBegin = 0x05,
    MemoryEnd = 0x06,
    MemoryData = 0x07,
    Sync = 0x08,
    ReadFlash = 0x0e,
    ChangeBaudrate = 0x0f,
    FlashMd5 = 0x13,
    EraseFlash = 0xd0,
    EraseRegion = 0xd1,
    ReadFlashId = 0xf3,
    ReadChipId = 0xf4,
}

#[binwrite]
#[derive(Debug)]
#[allow(dead_code)]
pub enum MemoryAction {
    #[bw(magic = 0x00u32)]
    Boot(#[bw(calc = 0)] u32),
    #[bw(magic = 0x01u32)]
    JumpTo(u32),
    #[bw(magic = 0x02u32)]
    Run(#[bw(calc = 0)] u32),
}

#[binread]
#[br(little)]
#[derive(Debug)]
pub struct FlashInfo {
    #[br(assert(id != [0x00, 0x00, 0x00] && id != [0xFF, 0xFF, 0xFF]))]
    pub id: [u8; 3],
}

impl FlashInfo {
    pub fn size(&self) -> usize {
        2 << (self.id[2] - 1)
    }
}

impl fmt::Display for FlashInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.id {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl TryFrom<ResponseEnvelope> for FlashInfo {
    type Error = io::Error;

    fn try_from(res: ResponseEnvelope) -> io::Result<Self> {
        Self::read(&mut Cursor::new(res.value.to_le_bytes()))
            .map_err(|_| io::ErrorKind::InvalidData.into())
    }
}

#[binread]
#[br(little)]
#[derive(Debug)]
pub struct ChipId([u8; 8]);

impl fmt::Display for ChipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl TryFrom<ResponseEnvelope> for ChipId {
    type Error = io::Error;

    fn try_from(res: ResponseEnvelope) -> io::Result<Self> {
        Self::read(&mut res.to_reader()).map_err(|_| io::ErrorKind::InvalidData.into())
    }
}

#[binwrite]
#[bw(little)]
#[derive(Debug)]
pub enum Request {
    FlashBegin {
        size: u32,
        blocks: u32,
        block_size: u32,
        offset: u32,
    },
    FlashData {
        #[bw(calc = data.len() as u32)]
        size: u32,
        seq: u32,
        #[bw(calc = 0)]
        rev1: u32,
        #[bw(calc = 0)]
        rev2: u32,
        data: Vec<u8>,
    },
    FlashEnd {
        #[bw(calc = 0xFF)]
        rev: u32,
    },
    MemoryBegin {
        size: u32,
        blocks: u32,
        block_size: u32,
        offset: u32,
    },
    MemoryEnd {
        action: MemoryAction,
    },
    MemoryData {
        #[bw(calc = data.len() as u32)]
        size: u32,
        seq: u32,
        #[bw(calc = 0)]
        rev1: u32,
        #[bw(calc = 0)]
        rev2: u32,
        data: Vec<u8>,
    },
    Sync(#[bw(calc = SYNC_STUB)] [u8; 36]),
    ChangeBaudrate {
        to: u32,
        from: u32,
    },
    FlashMd5 {
        offset: u32,
        size: u32,
        #[bw(calc = 0)]
        rev1: u32,
        #[bw(calc = 0)]
        rev2: u32,
    },
    ReadFlashId,
    ReadChipId,
}

impl Request {
    pub fn command(&self) -> Command {
        match self {
            Request::FlashBegin { .. } => Command::FlashBegin,
            Request::FlashData { .. } => Command::FlashData,
            Request::FlashEnd { .. } => Command::FlashEnd,
            Request::MemoryBegin { .. } => Command::MemoryBegin,
            Request::MemoryEnd { .. } => Command::MemoryEnd,
            Request::MemoryData { .. } => Command::MemoryData,
            Request::Sync() => Command::Sync,
            Request::ChangeBaudrate { .. } => Command::ChangeBaudrate,
            Request::FlashMd5 { .. } => Command::FlashMd5,
            Request::ReadFlashId => Command::ReadFlashId,
            Request::ReadChipId => Command::ReadChipId,
        }
    }

    pub fn checksum(&self) -> u32 {
        match self {
            Request::FlashData { data, .. } => utils::checksum(data),
            Request::MemoryData { data, .. } => utils::checksum(data),
            _ => 0,
        }
    }
}

impl TryInto<Vec<u8>> for Request {
    type Error = io::Error;

    fn try_into(self) -> io::Result<Vec<u8>> {
        let mut writer = Cursor::new(Vec::new());
        self.write(&mut writer)
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        Ok(writer.into_inner())
    }
}
