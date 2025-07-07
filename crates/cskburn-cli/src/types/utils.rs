use std::num::ParseIntError;

pub fn parse_addr(s: &str) -> Result<u32, ParseIntError> {
    if s.starts_with("0x") {
        u32::from_str_radix(&s[2..], 16)
    } else {
        s.parse()
    }
}
