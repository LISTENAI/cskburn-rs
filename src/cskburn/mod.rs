mod commands;
mod error;
mod requests;
mod slip;

use commands::Request;
use log::trace;
use serialport::SerialPort;
use slip_codec::{SlipDecoder, SlipEncoder};
use std::{io, thread::sleep, time::Duration};

pub use error::Error;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSKBurnBuilder {
    path: String,
    baud: u32,
}

pub fn new(path: String, baud: u32) -> CSKBurnBuilder {
    CSKBurnBuilder { path, baud }
}

impl CSKBurnBuilder {
    pub fn open(self) -> Result<CSKBurn> {
        let port = serialport::new(self.path, 115200)
            .flow_control(serialport::FlowControl::None)
            .open()?;

        Ok(CSKBurn {
            port,
            baud: self.baud,
            slip_enc: SlipEncoder::new(true),
            slip_dec: SlipDecoder::new(),
        })
    }
}

pub struct CSKBurn {
    port: Box<dyn SerialPort>,
    baud: u32,
    slip_enc: SlipEncoder,
    slip_dec: SlipDecoder,
}

impl CSKBurn {
    pub fn reset(&mut self, boot_mode: bool, reset_interval: Option<Duration>) -> Result<()> {
        trace!("set RTS: {}", boot_mode);
        self.port.write_request_to_send(boot_mode)?;

        trace!("set DTR: {}", true);
        self.port.write_data_terminal_ready(true)?;

        sleep(reset_interval.unwrap_or(Duration::from_millis(100)));

        trace!("set DTR: {}", false);
        self.port.write_data_terminal_ready(false)?;

        Ok(())
    }

    pub fn probe(&mut self, attempts: Option<usize>) -> Result<()> {
        for _ in 0..attempts.unwrap_or(1) {
            if self.command::<()>(Request::Sync(), None).is_ok() {
                return Ok(());
            }
        }

        Err(Error::io(io::ErrorKind::TimedOut))
    }
}
