use std::fmt;

/// Parse a user-supplied number into a `u32`.
///
/// Accepted forms:
/// - hex literal: `0x100000`, `0X100000`, `0x3000_0000`
/// - decimal: `1048576`, `1_048_576`
/// - decimal with binary unit suffix: `4K`, `1M`, `1G` (1024-based; lowercase OK)
///
/// Underscores are allowed anywhere as digit separators and are simply
/// stripped before parsing — like Rust literal syntax. Hex literals must be
/// raw values; `0x100K` is rejected to avoid the ambiguity of "hex times
/// 1024". Fractional inputs like `1.5M` are not supported either.
pub fn parse_u32(input: &str) -> Result<u32, ParseError> {
    let s: String = input.chars().filter(|c| *c != '_').collect();
    if s.is_empty() {
        return Err(ParseError::Invalid);
    }

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if hex.is_empty() || hex.chars().any(|c| !c.is_ascii_hexdigit()) {
            return Err(ParseError::Invalid);
        }
        return u32::from_str_radix(hex, 16).map_err(|_| ParseError::Overflow);
    }

    let (digits, multiplier): (&str, u64) = match s.as_bytes().last() {
        Some(b'K' | b'k') => (&s[..s.len() - 1], 1024),
        Some(b'M' | b'm') => (&s[..s.len() - 1], 1024 * 1024),
        Some(b'G' | b'g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s.as_str(), 1),
    };

    if digits.is_empty() {
        return Err(ParseError::Invalid);
    }
    let n: u64 = digits.parse().map_err(|_| ParseError::Invalid)?;
    let total = n.checked_mul(multiplier).ok_or(ParseError::Overflow)?;
    if total > u32::MAX as u64 {
        return Err(ParseError::Overflow);
    }
    Ok(total as u32)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Invalid,
    Overflow,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Invalid => f.write_str("invalid number format"),
            ParseError::Overflow => f.write_str("value out of range for u32"),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_basic() {
        assert_eq!(parse_u32("0x1000").unwrap(), 0x1000);
        assert_eq!(parse_u32("0X1000").unwrap(), 0x1000);
        assert_eq!(parse_u32("0xdeadBEEF").unwrap(), 0xdeadbeef);
    }

    #[test]
    fn decimal_basic() {
        assert_eq!(parse_u32("0").unwrap(), 0);
        assert_eq!(parse_u32("1024").unwrap(), 1024);
        assert_eq!(parse_u32("4294967295").unwrap(), u32::MAX);
    }

    #[test]
    fn underscores_anywhere() {
        assert_eq!(parse_u32("0x3000_0000").unwrap(), 0x3000_0000);
        assert_eq!(parse_u32("1_048_576").unwrap(), 1_048_576);
        assert_eq!(parse_u32("1_M").unwrap(), 1024 * 1024);
    }

    #[test]
    fn binary_suffixes() {
        assert_eq!(parse_u32("4K").unwrap(), 4 * 1024);
        assert_eq!(parse_u32("4k").unwrap(), 4 * 1024);
        assert_eq!(parse_u32("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_u32("1m").unwrap(), 1024 * 1024);
        assert_eq!(parse_u32("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn hex_with_suffix_rejected() {
        assert_eq!(parse_u32("0x100K"), Err(ParseError::Invalid));
        assert_eq!(parse_u32("0xK"), Err(ParseError::Invalid));
    }

    #[test]
    fn fractional_rejected() {
        assert_eq!(parse_u32("1.5M"), Err(ParseError::Invalid));
        assert_eq!(parse_u32("1.5"), Err(ParseError::Invalid));
    }

    #[test]
    fn overflow_rejected() {
        assert_eq!(parse_u32("4G"), Err(ParseError::Overflow));
        assert_eq!(parse_u32("4294967296"), Err(ParseError::Overflow));
        assert_eq!(parse_u32("0x100000000"), Err(ParseError::Overflow));
    }

    #[test]
    fn empty_or_garbage() {
        assert_eq!(parse_u32(""), Err(ParseError::Invalid));
        assert_eq!(parse_u32("_"), Err(ParseError::Invalid));
        assert_eq!(parse_u32("K"), Err(ParseError::Invalid));
        assert_eq!(parse_u32("0x"), Err(ParseError::Invalid));
        assert_eq!(parse_u32("abc"), Err(ParseError::Invalid));
    }
}
