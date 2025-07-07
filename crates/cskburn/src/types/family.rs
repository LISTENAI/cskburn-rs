#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Family {
    CSK4,
    CSK6,
}

impl Family {
    pub fn burner(&self) -> Vec<u8> {
        match self {
            Family::CSK4 => include_bytes!("../burners/burner_4.bin").to_vec(),
            Family::CSK6 => include_bytes!("../burners/burner_6.bin").to_vec(),
        }
    }

    pub fn rom_supports_change_baudrate(&self) -> bool {
        match self {
            Family::CSK4 => false,
            Family::CSK6 => true,
        }
    }
}

impl TryFrom<u8> for Family {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            3 | 4 => Ok(Family::CSK4),
            6 => Ok(Family::CSK6),
            _ => Err("Invalid chip family"),
        }
    }
}
