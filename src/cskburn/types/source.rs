use std::{
    fs::File,
    io::{self, Cursor, Read},
};

pub trait Source: Read {
    fn size(&self) -> io::Result<usize>;
}

impl Source for File {
    fn size(&self) -> io::Result<usize> {
        self.metadata().map(|m| m.len() as usize)
    }
}

impl<T: AsRef<[u8]>> Source for Cursor<T> {
    fn size(&self) -> io::Result<usize> {
        Ok(self.get_ref().as_ref().len())
    }
}
