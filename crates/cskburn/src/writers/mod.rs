mod flash;
mod memory;

use log::debug;

use crate::{
    CSKBurn, Error, Image, Result,
    commands::{MemoryAction, Request},
    writers::{flash::FlashWriteOperation, memory::MemoryWriteOperation},
};

#[derive(Debug, Clone)]
pub enum WriteTarget {
    Memory { action: Option<MemoryAction> },
    Flash,
}

#[derive(Debug, Clone)]
pub struct WriteStep {
    pub bytes_written: usize,
    pub total_bytes: usize,
}

pub trait WriteProtocol {
    fn block_size(&self) -> usize;
    fn begin(&mut self, offset: u32, size: usize) -> Request;
    fn data(&mut self, seq: u32, data: &[u8]) -> Request;
    fn end(&self) -> Request;
}

pub enum WriteIteratorState {
    Begin,
    Data { seq: u32 },
    End,
}

pub struct WriteIterator<'a> {
    cskburn: &'a mut CSKBurn,
    source: &'a mut Image,
    protocol: Box<dyn WriteProtocol>,
    state: WriteIteratorState,
    buffer: Vec<u8>,
    bytes_written: usize,
    total_bytes: usize,
}

impl<'a> Iterator for WriteIterator<'a> {
    type Item = Result<WriteStep>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            WriteIteratorState::Begin => {
                let req = self.protocol.begin(self.source.addr, self.total_bytes);
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
                    let req = self.protocol.end();
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
                    let req = self.protocol.data(seq, &self.buffer[..bytes_read]);
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

impl<'a> WriteIterator<'a> {
    pub fn new(
        cskburn: &'a mut CSKBurn,
        source: &'a mut Image,
        target: WriteTarget,
    ) -> Result<Self> {
        let protocol: Box<dyn WriteProtocol> = match target {
            WriteTarget::Memory { action } => Box::new(MemoryWriteOperation::new(action)),
            WriteTarget::Flash => Box::new(FlashWriteOperation::new()),
        };

        let buffer = vec![0; protocol.block_size()];

        let total_bytes = source.size()?;

        Ok(WriteIterator {
            cskburn,
            source,
            protocol,
            state: WriteIteratorState::Begin,
            buffer,
            bytes_written: 0,
            total_bytes,
        })
    }
}
