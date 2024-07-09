use super::utils;
use binrw::{binrw, binwrite, BinWrite};
use std::{
    fmt::Debug,
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

#[binwrite]
#[bw(little)]
#[derive(Debug)]
pub enum Request {
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
}

impl Request {
    pub fn command(&self) -> Command {
        match self {
            Request::MemoryBegin { .. } => Command::MemoryBegin,
            Request::MemoryEnd { .. } => Command::MemoryEnd,
            Request::MemoryData { .. } => Command::MemoryData,
            Request::Sync() => Command::Sync,
        }
    }

    pub fn checksum(&self) -> u32 {
        match self {
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
