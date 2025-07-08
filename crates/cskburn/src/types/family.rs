use std::str::FromStr;

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
