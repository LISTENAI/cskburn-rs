use log::{error, trace};
use slip_codec::SlipError;
use std::io::{Error, ErrorKind, Read, Result, Seek, Write};

impl Write for super::CSKBurn {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        trace!("tx raw: {} {:02x?}", buf.len(), buf);
        self.slip_enc_buf.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.slip_enc_buf.rewind()?;

        let ret = self
            .slip_enc
            .encode(self.slip_enc_buf.get_ref(), &mut self.port)?;
        trace!("tx len: {}", ret);

        self.slip_enc_buf.get_mut().clear();
        self.slip_enc_buf.rewind()?;

        self.port.flush()
    }
}

impl Read for super::CSKBurn {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if (self.slip_dec_buf.position() as usize) < self.slip_dec_buf.get_ref().len() {
            let size = self.slip_dec_buf.read(buf)?;
            trace!(
                "rx len: {} ({}/{})",
                size,
                self.slip_dec_buf.position(),
                self.slip_dec_buf.get_ref().len()
            );
            return Ok(size);
        }

        self.slip_dec_buf.get_mut().clear();
        self.slip_dec_buf.rewind()?;

        match self.slip_dec.decode(&mut self.port, &mut self.slip_dec_buf) {
            Ok(size) => {
                trace!("rx raw: {} {:02x?}", size, self.slip_dec_buf.get_ref());
                self.slip_dec_buf.rewind()?;
                let size = self.slip_dec_buf.read(buf)?;
                trace!("rx len: {}", size);
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
