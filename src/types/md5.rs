use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct Md5(pub [u8; 16]);

impl fmt::Display for Md5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}
