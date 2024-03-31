use super::addr_spec::addr_from_str;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct RegionSpec {
    pub addr: u32,
    pub size: u32,
}

impl FromStr for RegionSpec {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err("Invalid number of parts");
        }

        let addr = addr_from_str(parts[0]).map_err(|_| "Invalid addr format")?;
        let size = addr_from_str(parts[1]).map_err(|_| "Invalid size format")?;

        Ok(Self { addr, size })
    }
}
