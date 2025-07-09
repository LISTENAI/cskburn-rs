use log::{error, trace};
use slip_codec::SlipError;
use std::io::{Error, ErrorKind, Read, Result, Write};

impl Write for super::CSKBurn {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        trace!("tx raw: {} {:02x?}", buf.len(), buf);

        let ret = self.slip_enc.encode(buf, &mut self.port)?;
        trace!("tx len: {}", ret);

        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        self.port.flush()
    }
}

impl Read for super::CSKBurn {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut out: Vec<u8> = vec![];
        match self.slip_dec.decode(&mut self.port, &mut out) {
            Ok(size) => {
                trace!("rx raw: {} {:02x?}", size, &out);
                let size = size.min(buf.len()).min(out.len());
                trace!("rx len: {}", size);
                buf[..size].copy_from_slice(&out[..size]);
                return Ok(size);
            }
            Err(SlipError::ReadError(err)) if err.kind() == ErrorKind::TimedOut => {
                trace!("rx timeout");
                return Err(Error::new(ErrorKind::TimedOut, "Timeout"));
            }
            Err(e) => {
                error!("rx error: {:?}", e);
                return Err(Error::new(
                    ErrorKind::BrokenPipe,
                    "Failed to read from port",
                ));
            }
        }
    }
}
