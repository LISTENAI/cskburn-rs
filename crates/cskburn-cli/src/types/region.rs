use core::fmt;
use std::str::FromStr;

use crate::types::num;

/// A single region argument for the erase / verify subcommands.
///
/// Accepted forms:
///   - `chip`        — the entire flash chip
///   - `<addr>+<size>` — a specific range; both halves use the number syntax
///                       documented in [`num::parse_u32`]
#[derive(Clone, Debug)]
pub enum RegionToken {
    Chip,
    Range { addr: u32, size: u32 },
}

impl fmt::Display for RegionToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RegionToken::Chip => f.write_str("chip"),
            RegionToken::Range { addr, size } => write!(f, "0x{:08x}+0x{:x}", addr, size),
        }
    }
}

impl FromStr for RegionToken {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("chip") {
            return Ok(RegionToken::Chip);
        }

        let (addr_str, size_str) = s
            .split_once('+')
            .ok_or("Expected `chip` or `<addr>+<size>`")?;
        let addr = num::parse_u32(addr_str).map_err(|_| "Invalid addr")?;
        let size = num::parse_u32(size_str).map_err(|_| "Invalid size")?;

        Ok(RegionToken::Range { addr, size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chip() {
        assert!(matches!(
            RegionToken::from_str("chip").unwrap(),
            RegionToken::Chip
        ));
        assert!(matches!(
            RegionToken::from_str("CHIP").unwrap(),
            RegionToken::Chip
        ));
    }

    #[test]
    fn parse_range() {
        match RegionToken::from_str("0x100000+4K").unwrap() {
            RegionToken::Range { addr, size } => {
                assert_eq!(addr, 0x100000);
                assert_eq!(size, 4096);
            }
            _ => panic!("expected Range"),
        }
    }

    #[test]
    fn parse_range_decimal_units() {
        match RegionToken::from_str("1M+4K").unwrap() {
            RegionToken::Range { addr, size } => {
                assert_eq!(addr, 0x100000);
                assert_eq!(size, 0x1000);
            }
            _ => panic!("expected Range"),
        }
    }

    #[test]
    fn rejects_no_plus() {
        assert!(RegionToken::from_str("0x100000").is_err());
        assert!(RegionToken::from_str("foo").is_err());
    }
}
