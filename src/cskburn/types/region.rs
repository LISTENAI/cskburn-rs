use super::image::Image;
use core::fmt;
use std::io;

#[derive(Clone, Debug)]
pub struct Region {
    pub addr: u32,
    pub size: u32,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:08x}:0x{:08x}", self.addr, self.size)
    }
}

impl TryFrom<Image> for Region {
    type Error = io::Error;

    fn try_from(spec: Image) -> Result<Self, Self::Error> {
        Ok(Region {
            addr: spec.addr,
            size: spec.size()? as u32,
        })
    }
}
