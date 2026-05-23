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

impl WriteTarget {
    /// Map a write-time failure to the appropriate categorized error.
    fn wrap_err(&self, e: Error) -> Error {
        match self {
            WriteTarget::Flash => e.wrap_as(Error::FlashWriteFailed),
            WriteTarget::Memory { .. } => e.wrap_as(Error::RamWriteFailed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WriteStep {
    pub bytes_written: usize,
    pub total_bytes: usize,
}

const DEFAULT_DATA_RETRIES: usize = 3;

pub trait WriteProtocol {
    fn block_size(&self) -> usize;
    fn begin(&self, offset: u32, size: usize) -> Request;
    fn data<'a>(&self, seq: u32, data: &'a [u8]) -> Request<'a>;
    fn end(&self) -> Request;

    fn data_retries(&self) -> usize {
        0
    }
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
    target: WriteTarget,
    state: WriteIteratorState,
    buffer: Vec<u8>,
    bytes_written: usize,
    total_bytes: usize,
}

impl<'a> Iterator for WriteIterator<'a> {
    type Item = Result<WriteStep>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Err(e) = self.cskburn.check_cancelled() {
            return Some(Err(e));
        }
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
                    Err(e) => Some(Err(self.target.wrap_err(e))),
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
                        Err(e) => Some(Err(self.target.wrap_err(e))),
                    }
                }
                Ok(bytes_read) => {
                    debug!("write data, seq: {}, size: {}", seq, bytes_read);

                    let retries = self.protocol.data_retries();
                    let mut last_err = None;

                    for attempt in 0..=retries {
                        if attempt > 0 {
                            debug!("retry writing block {}, attempt {}", seq, attempt);
                        }

                        let req = self.protocol.data(seq, &self.buffer[..bytes_read]);
                        match self.cskburn.command(req) {
                            Ok(()) => {
                                self.state = WriteIteratorState::Data { seq: seq + 1 };
                                self.bytes_written += bytes_read;
                                return Some(Ok(WriteStep {
                                    bytes_written: self.bytes_written,
                                    total_bytes: self.total_bytes,
                                }));
                            }
                            Err(e) if e.is_timeout() && attempt < retries => {
                                last_err = Some(e);
                                continue;
                            }
                            Err(e) => return Some(Err(self.target.wrap_err(e))),
                        }
                    }

                    Some(Err(self.target.wrap_err(last_err.unwrap())))
                }
                // Reading from the local source (file backing the image) failed.
                // We don't have the file path here, so map to a bare I/O error
                // — the caller has the path context and should report it.
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
        let protocol: Box<dyn WriteProtocol> = match &target {
            WriteTarget::Memory { action } => Box::new(MemoryWriteOperation::new(action.clone())),
            WriteTarget::Flash => Box::new(FlashWriteOperation::new()),
        };

        let buffer = vec![0; protocol.block_size()];

        let total_bytes = source.size()?;

        Ok(WriteIterator {
            cskburn,
            source,
            protocol,
            target,
            state: WriteIteratorState::Begin,
            buffer,
            bytes_written: 0,
            total_bytes,
        })
    }
}
