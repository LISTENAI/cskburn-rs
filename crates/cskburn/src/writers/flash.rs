use crate::{
    CSKBurn, Image, Result,
    commands::Request,
    utils,
    writers::{WriteIterator, WriteIteratorState, WriteProtocol},
};

const FLASH_BLOCK_SIZE: usize = 4096;

pub struct FlashWriteOperation {}

impl WriteProtocol for FlashWriteOperation {
    fn begin(&mut self, offset: u32, size: usize) -> Request {
        Request::FlashBegin {
            size: size as u32,
            blocks: utils::blocks(size, FLASH_BLOCK_SIZE) as u32,
            block_size: FLASH_BLOCK_SIZE as u32,
            offset,
        }
    }

    fn data(&mut self, seq: u32, data: &[u8]) -> Request {
        Request::FlashData {
            seq,
            data: data.to_vec(),
        }
    }

    fn end(&self) -> Request {
        Request::FlashEnd {}
    }
}

pub type FlashWriteIterator<'a> = WriteIterator<'a, FlashWriteOperation>;

impl<'a> FlashWriteIterator<'a> {
    pub fn new(cskburn: &'a mut CSKBurn, source: &'a mut Image) -> Result<Self> {
        let total_bytes = source.size()?;
        Ok(FlashWriteIterator {
            cskburn,
            source,
            ops: FlashWriteOperation {},
            state: WriteIteratorState::Begin,
            buffer: vec![0; FLASH_BLOCK_SIZE],
            bytes_written: 0,
            total_bytes,
        })
    }
}
