use std::{thread::sleep, time::Duration};

use log::trace;
use serialport::SerialPort;

use crate::family::ChipProtocol;

pub struct MarsProtocol;

impl ChipProtocol for MarsProtocol {
    fn reset(
        &self,
        port: &mut dyn SerialPort,
        boot_mode: bool,
        reset_interval: Option<Duration>,
    ) -> Result<(), std::io::Error> {
        let reset_interval = reset_interval.unwrap_or(Duration::from_millis(100));

        trace!("set DTR: {}", true);
        port.write_data_terminal_ready(true)?;

        sleep(if boot_mode {
            Duration::from_secs(2)
        } else {
            reset_interval
        });

        trace!("set DTR: {}", false);
        port.write_data_terminal_ready(false)?;

        sleep(reset_interval);

        Ok(())
    }

    fn rom_requires_adaptive_duplex(&self) -> bool {
        true
    }
}
