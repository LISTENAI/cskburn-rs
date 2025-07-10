use log::debug;

use crate::{
    CSKBurn, Error, Image, Result,
    commands::{MemoryAction, Request},
    utils,
};

const MEMORY_BLOCK_SIZE: usize = 2048;
const FLASH_BLOCK_SIZE: usize = 4096;

#[derive(Debug, Clone)]
pub struct WriteStep {
    pub bytes_written: usize,
    pub total_bytes: usize,
}

pub trait WriteProtocol {
    fn begin(&mut self, offset: u32, size: usize) -> Request;
    fn data(&mut self, seq: u32, data: &[u8]) -> Request;
    fn end(&self) -> Request;
}

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

pub enum WriteIteratorState {
    Begin,
    Data { seq: u32 },
    End,
}

pub struct WriteIterator<'a, T: WriteProtocol> {
    cskburn: &'a mut CSKBurn,
    source: &'a mut Image,
    ops: T,
    state: WriteIteratorState,
    buffer: Vec<u8>,
    bytes_written: usize,
    total_bytes: usize,
}

impl<'a, T: WriteProtocol> Iterator for WriteIterator<'a, T> {
    type Item = Result<WriteStep>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            WriteIteratorState::Begin => {
                let req = self.ops.begin(self.source.addr, self.total_bytes);
                debug!("begin write, total size: {}", self.total_bytes);

                match self.cskburn.command(req) {
                    Ok(()) => {
                        self.state = WriteIteratorState::Data { seq: 0 };
                        Some(Ok(WriteStep {
                            bytes_written: self.bytes_written,
                            total_bytes: self.total_bytes,
                        }))
                    }
                    Err(e) => Some(Err(e)),
                }
            }
            WriteIteratorState::Data { seq } => match self.source.source.read(&mut self.buffer) {
                Ok(0) => {
                    let req = self.ops.end();
                    debug!("end write");

                    match self.cskburn.command(req) {
                        Ok(()) => {
                            self.state = WriteIteratorState::End;
                            Some(Ok(WriteStep {
                                bytes_written: self.bytes_written,
                                total_bytes: self.total_bytes,
                            }))
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
                Ok(bytes_read) => {
                    let req = self.ops.data(seq, &self.buffer[..bytes_read]);
                    debug!("write data, seq: {}, size: {}", seq, bytes_read);

                    match self.cskburn.command(req) {
                        Ok(()) => {
                            self.state = WriteIteratorState::Data { seq: seq + 1 };
                            self.bytes_written += bytes_read;
                            Some(Ok(WriteStep {
                                bytes_written: self.bytes_written,
                                total_bytes: self.total_bytes,
                            }))
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
                Err(e) => Some(Err(Error::from(e))),
            },
            WriteIteratorState::End => None,
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
