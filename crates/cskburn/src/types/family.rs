use std::{io::Cursor, str::FromStr};

use crate::Image;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Family {
    CSK4,
    CSK6,
}

impl Family {
    pub fn burner(&self) -> Image {
        match self {
            Family::CSK4 => Image::new(
                0x0000_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_4.bin"))),
            ),
            Family::CSK6 => Image::new(
                0x0000_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_6.bin"))),
            ),
        }
    }

    pub fn rom_supports_change_baudrate(&self) -> bool {
        match self {
            Family::CSK4 => false,
            Family::CSK6 => true,
        }
    }
}

impl FromStr for Family {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "3" | "4" => Ok(Family::CSK4),
            "6" => Ok(Family::CSK6),
            _ => Err("Invalid chip family"),
        }
    }
}
