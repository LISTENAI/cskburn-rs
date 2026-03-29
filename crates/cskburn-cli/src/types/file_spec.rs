use std::{
    fs::File,
    io::{self, Read},
    str::FromStr,
};

use core::fmt;
use cskburn::Image;

use crate::types::utils;

#[derive(Clone, Debug)]
pub enum FileSpec {
    Raw { addr: u32, path: String },
    Hex { path: String },
}

impl FileSpec {
    pub fn path(&self) -> &str {
        match self {
            FileSpec::Raw { path, .. } => path,
            FileSpec::Hex { path } => path,
        }
    }

    pub fn md5(&self) -> Result<[u8; 16], io::Error> {
        let mut file = File::open(self.path())?;
        let mut context = md5::Context::new();
        let mut buf = [0; 1024];
        loop {
            let count = file.read(&mut buf)?;
            if count == 0 {
                break;
            }
            context.consume(&buf[..count]);
        }
        Ok(context.finalize().0)
    }
}

impl fmt::Display for FileSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileSpec::Raw { addr, path } => write!(f, "0x{:08x}:{}", addr, path),
            FileSpec::Hex { path } => write!(f, "{}", path),
        }
    }
}

impl FromStr for FileSpec {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Try parsing as ADDR:FILE first
        if let Some((addr_str, path)) = s.split_once(':') {
            if let Ok(addr) = utils::parse_addr(addr_str) {
                return Ok(FileSpec::Raw {
                    addr,
                    path: path.to_string(),
                });
            }
        }

        // For HEX files, allow just a file path without address
        if s.to_lowercase().ends_with(".hex") {
            return Ok(FileSpec::Hex {
                path: s.to_string(),
            });
        }

        Err("Expected ADDR:FILE or a .hex file path")
    }
}

impl TryFrom<FileSpec> for Image {
    type Error = io::Error;

    fn try_from(spec: FileSpec) -> Result<Self, Self::Error> {
        match spec {
            FileSpec::Raw { addr, path } => Image::try_from_file(addr, &path),
            FileSpec::Hex { .. } => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "HEX files must be resolved via hex::parse_hex",
            )),
        }
    }
}
