use crate::cskburn::Source;

use super::addr_spec::addr_from_str;
use std::{
    fs::File,
    io::{self, Read},
    str::FromStr,
};

#[derive(Clone, Debug)]
pub struct FileSpec {
    pub addr: u32,
    pub path: String,
}

impl FromStr for FileSpec {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err("Invalid number of parts");
        }

        let addr = addr_from_str(parts[0]).map_err(|_| "Invalid addr format")?;
        let path = parts[1].to_string();

        Ok(Self { addr, path })
    }
}

impl FileSpec {
    pub fn as_source(&self) -> Result<Box<dyn Source>, io::Error> {
        Ok(Box::new(File::open(&self.path)?))
    }

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
