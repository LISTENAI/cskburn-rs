use std::{io::Cursor, str::FromStr, time::Duration};

use serialport::SerialPort;

mod arcs;
mod mars;
mod reset;
mod venus;
mod venusa;

pub use reset::ResetStrategy;

use crate::{
    Image,
    family::{
        arcs::ArcsProtocol, mars::MarsProtocol, venus::VenusProtocol, venusa::VenusaProtocol,
    },
};

pub trait ChipProtocol {
    /// Run the reset sequence using the given strategy. Chips with a custom
    /// reset sequence (e.g. those without a BOOT pin) may override this and
    /// ignore the strategy.
    fn reset(
        &self,
        port: &mut dyn SerialPort,
        strategy: ResetStrategy,
        boot_mode: bool,
        reset_interval: Option<Duration>,
    ) -> Result<(), std::io::Error> {
        let reset_delay = reset_interval.unwrap_or(Duration::from_millis(100));
        strategy.apply(port, boot_mode, reset_delay)
    }

    /// Reset strategies known to work on production boards for this chip. The
    /// first entry is the default; the rest are alternates that auto-mode
    /// cycles through.
    fn reset_candidates(&self) -> &'static [ResetStrategy] {
        &[ResetStrategy::RtsBoot]
    }

    fn rom_supports_change_baudrate(&self) -> bool {
        true
    }

    fn rom_requires_adaptive_duplex(&self) -> bool {
        false
    }

    fn burner_supports_progressive_erase(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    VENUS,
    MARS,
    ARCS,
    VENUSA,
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
            Family::ARCS => Image::new(
                0x2004_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_arcs.bin"))),
            ),
            Family::VENUSA => Image::new(
                0x2005_0000,
                Box::new(Cursor::new(include_bytes!("../burners/burner_venusa.bin"))),
            ),
        }
    }

    pub fn base_addr(&self) -> u32 {
        match self {
            Family::VENUS => 0x1800_0000,
            Family::MARS => 0x1800_0000,
            Family::ARCS => 0x3000_0000,
            Family::VENUSA => 0x0300_0000,
        }
    }

    pub fn protocol(&self) -> Box<dyn ChipProtocol> {
        match self {
            Family::VENUS => Box::new(VenusProtocol),
            Family::MARS => Box::new(MarsProtocol),
            Family::ARCS => Box::new(ArcsProtocol),
            Family::VENUSA => Box::new(VenusaProtocol),
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
            "arcs" => Ok(Family::ARCS),
            x if x.starts_with("ls26") => Ok(Family::ARCS),
            "venusa" | "7" => Ok(Family::VENUSA),
            x if x.starts_with("csk7") => Ok(Family::VENUSA),
            _ => Err("Unsupported chip family"),
        }
    }
}
