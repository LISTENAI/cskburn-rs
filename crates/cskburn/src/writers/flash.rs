use crate::{commands::Request, utils, writers::WriteProtocol};

const FLASH_BLOCK_SIZE: usize = 4096;

pub struct FlashWriteOperation {}

impl FlashWriteOperation {
    pub fn new() -> Self {
        FlashWriteOperation {}
    }
}

impl WriteProtocol for FlashWriteOperation {
    fn block_size(&self) -> usize {
        FLASH_BLOCK_SIZE
    }

    fn begin(&self, offset: u32, size: usize) -> Request {
        Request::FlashBegin {
            size: size as u32,
            blocks: utils::blocks(size, self.block_size()) as u32,
            block_size: self.block_size() as u32,
            offset,
        }
    }

    fn data(&self, seq: u32, data: &[u8]) -> Request {
        Request::FlashData {
            seq,
            data: data.to_vec(),
        }
    }

    fn end(&self) -> Request {
        Request::FlashEnd {}
    }
}
