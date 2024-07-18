use super::source::Source;
use std::io;

pub struct Image {
    pub addr: u32,
    pub source: Box<dyn Source>,
}

impl Image {
    pub fn new(addr: u32, source: Box<dyn Source>) -> Self {
        Image { addr, source }
    }

    pub fn size(&self) -> io::Result<usize> {
        self.source.size()
    }
}
