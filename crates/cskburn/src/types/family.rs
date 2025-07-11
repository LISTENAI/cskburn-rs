use std::{io::Cursor, str::FromStr};

use crate::Image;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Family {
    VENUS,
    MARS,
}

impl Family {
    pub fn burner(&self) -> Image {
        match self {
            Family::VENUS => Image::new(
                0x0000_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_venus.bin"))),
            ),
            Family::MARS => Image::new(
                0x0001_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_mars.bin"))),
            ),
        }
    }

    pub fn rom_supports_change_baudrate(&self) -> bool {
        match self {
            Family::VENUS => true,
            Family::MARS => true,
        }
    }
}

impl FromStr for Family {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "venus" | "6" => Ok(Family::VENUS),
            x if x.starts_with("csk6") => Ok(Family::VENUS),
            "mars" | "5" => Ok(Family::MARS),
            x if x.starts_with("csk5") => Ok(Family::MARS),
            _ => Err("Unsupported chip family"),
        }
    }
}
