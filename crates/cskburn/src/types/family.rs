use std::{io::Cursor, str::FromStr};

use crate::Image;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Family {
    CSK6,
    MARS,
}

impl Family {
    pub fn burner(&self) -> Image {
        match self {
            Family::CSK6 => Image::new(
                0x0000_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_6.bin"))),
            ),
            Family::MARS => Image::new(
                0x0001_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_mars.bin"))),
            ),
        }
    }

    pub fn rom_supports_change_baudrate(&self) -> bool {
        match self {
            Family::CSK6 => true,
            Family::MARS => true,
        }
    }
}

impl FromStr for Family {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "6" => Ok(Family::CSK6),
            "mars" | "MARS" => Ok(Family::MARS),
            _ => Err("Invalid chip family"),
        }
    }
}
