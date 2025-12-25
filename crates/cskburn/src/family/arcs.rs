use std::{thread::sleep, time::Duration};

use log::trace;

use crate::family::ChipProtocol;

pub struct ArcsProtocol;

impl ChipProtocol for ArcsProtocol {
    fn reset(
        &self,
        port: &mut dyn serialport::SerialPort,
        boot_mode: bool,
        reset_interval: Option<std::time::Duration>,
    ) -> Result<(), std::io::Error> {
        let reset_interval = reset_interval
            .unwrap_or_default()
            .max(Duration::from_millis(500));

        // Hold RESET first, so holding BOOT won't harm the chip
        trace!("set RTS: {}", true);
        port.write_request_to_send(true)?;
        trace!("set DTR: {}", false);
        port.write_data_terminal_ready(false)?;

        sleep(Duration::from_millis(50));

        trace!("set DTR: {}", boot_mode);
        port.write_data_terminal_ready(boot_mode)?;

        sleep(reset_interval);

        trace!("set RTS: {}", false);
        port.write_request_to_send(false)?;

        sleep(Duration::from_millis(50));

        // Make sure to release BOOT so TX frees up
        trace!("set DTR: {}", false);
        port.write_data_terminal_ready(false)?;

        sleep(Duration::from_millis(50));

        Ok(())
    }

    fn burner_supports_progressive_erase(&self) -> bool {
        false
    }
}
