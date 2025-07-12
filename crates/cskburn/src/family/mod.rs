use std::{io::Cursor, str::FromStr, thread::sleep, time::Duration};

use log::trace;
use serialport::SerialPort;

mod mars;
mod venus;

use crate::{
    Image,
    family::{mars::MarsProtocol, venus::VenusProtocol},
};

pub trait ChipProtocol {
    fn reset(
        &self,
        port: &mut dyn SerialPort,
        boot_mode: bool,
        reset_interval: Option<Duration>,
    ) -> Result<(), std::io::Error> {
        let reset_interval = reset_interval.unwrap_or(Duration::from_millis(100));

        trace!("set RTS: {}", boot_mode);
        port.write_request_to_send(boot_mode)?;

        trace!("set DTR: {}", true);
        port.write_data_terminal_ready(true)?;

        sleep(reset_interval);

        trace!("set DTR: {}", false);
        port.write_data_terminal_ready(false)?;

        sleep(reset_interval);

        Ok(())
    }

    fn rom_supports_change_baudrate(&self) -> bool {
        true
    }

    fn rom_requires_adaptive_duplex(&self) -> bool {
        false
    }
}

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

    pub fn protocol(&self) -> Box<dyn ChipProtocol> {
        match self {
            Family::VENUS => Box::new(VenusProtocol),
            Family::MARS => Box::new(MarsProtocol),
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
