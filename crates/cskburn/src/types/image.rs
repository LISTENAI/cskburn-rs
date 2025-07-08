use std::{fs::File, io};

use super::source::Source;

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

    pub fn try_from_file(addr: u32, path: &str) -> io::Result<Self> {
        Ok(Image {
            addr,
            source: Box::new(File::open(path)?) as Box<dyn Source>,
        })
    }
}
