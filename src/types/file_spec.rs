use crate::cskburn::Image;

use core::fmt;
use std::{
    fs::File,
    io::{self, Read},
};

#[derive(Clone, Debug)]
pub struct FileSpec {
    pub addr: u32,
    pub path: String,
}

impl fmt::Display for FileSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:08x}:{}", self.addr, self.path)
    }
}

impl FileSpec {
    pub fn md5(&self) -> Result<[u8; 16], io::Error> {
        let mut file = File::open(&self.path)?;
        let mut context = md5::Context::new();
        let mut buf = [0; 1024];
        loop {
            let count = file.read(&mut buf)?;
            if count == 0 {
                break;
            }
            context.consume(&buf[..count]);
        }
        Ok(context.compute().0)
    }
}

impl TryFrom<FileSpec> for Image {
    type Error = io::Error;

    fn try_from(spec: FileSpec) -> Result<Self, Self::Error> {
        Ok(Image {
            addr: spec.addr,
            source: Box::new(File::open(&spec.path)?),
        })
    }
}
