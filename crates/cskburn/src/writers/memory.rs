use crate::{
    commands::{MemoryAction, Request},
    utils,
    writers::WriteProtocol,
};

const MEMORY_BLOCK_SIZE: usize = 2048;

pub struct MemoryWriteOperation {
    action: Option<MemoryAction>,
}

impl MemoryWriteOperation {
    pub fn new(action: Option<MemoryAction>) -> Self {
        MemoryWriteOperation { action }
    }
}

impl WriteProtocol for MemoryWriteOperation {
    fn block_size(&self) -> usize {
        MEMORY_BLOCK_SIZE
    }

    fn begin(&self, offset: u32, size: usize) -> Request {
        Request::MemoryBegin {
            size: size as u32,
            blocks: utils::blocks(size, self.block_size()) as u32,
            block_size: self.block_size() as u32,
            offset,
        }
    }

    fn data<'a>(&self, seq: u32, data: &'a [u8]) -> Request<'a> {
        Request::MemoryData { seq, data }
    }

    fn end(&self) -> Request {
        Request::MemoryEnd {
            action: self.action.clone().unwrap_or(MemoryAction::Boot()),
        }
    }
}
