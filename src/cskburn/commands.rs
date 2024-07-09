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
#[bw(little)]
#[derive(Debug)]
pub enum Request {
    Sync(#[bw(calc = SYNC_STUB)] [u8; 36]),
}

impl Request {
    pub fn command(&self) -> Command {
        match self {
            Request::Sync() => Command::Sync,
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
