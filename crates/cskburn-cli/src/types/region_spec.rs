use core::fmt;
use std::str::FromStr;

use cskburn::Region;

use crate::types::utils;

#[derive(Clone, Debug)]
pub struct RegionSpec(pub Region);

impl fmt::Display for RegionSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RegionSpec {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err("Invalid number of parts");
        }

        let addr = utils::parse_u32(parts[0]).map_err(|_| "Invalid addr format")?;
        let size = utils::parse_u32(parts[1]).map_err(|_| "Invalid size format")?;

        Ok(Self(Region { addr, size }))
    }
}

impl From<RegionSpec> for Region {
    fn from(spec: RegionSpec) -> Self {
        spec.0
    }
}
