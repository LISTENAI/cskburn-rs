use crate::{
    CSKBurn, Image, Result,
    commands::{MemoryAction, Request},
    utils,
    writers::{WriteIterator, WriteIteratorState, WriteProtocol},
};

const MEMORY_BLOCK_SIZE: usize = 2048;

pub struct MemoryWriteOperation {
    action: Option<MemoryAction>,
}

impl WriteProtocol for MemoryWriteOperation {
    fn begin(&mut self, offset: u32, size: usize) -> Request {
        Request::MemoryBegin {
            size: size as u32,
            blocks: utils::blocks(size, MEMORY_BLOCK_SIZE) as u32,
            block_size: MEMORY_BLOCK_SIZE as u32,
            offset,
        }
    }

    fn data(&mut self, seq: u32, data: &[u8]) -> Request {
        Request::MemoryData {
            seq,
            data: data.to_vec(),
        }
    }

    fn end(&self) -> Request {
        Request::MemoryEnd {
            action: self.action.clone().unwrap_or(MemoryAction::Boot()),
        }
    }
}

pub type MemoryWriteIterator<'a> = WriteIterator<'a, MemoryWriteOperation>;

impl<'a> MemoryWriteIterator<'a> {
    pub fn new(
        cskburn: &'a mut CSKBurn,
        source: &'a mut Image,
        action: Option<MemoryAction>,
    ) -> Result<Self> {
        let total_bytes = source.size()?;
        Ok(MemoryWriteIterator {
            cskburn,
            source,
            ops: MemoryWriteOperation { action },
            state: WriteIteratorState::Begin,
            buffer: vec![0; MEMORY_BLOCK_SIZE],
            bytes_written: 0,
            total_bytes,
        })
    }
}
