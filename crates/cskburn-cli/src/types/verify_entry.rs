use std::path::PathBuf;
use std::str::FromStr;

use crate::types::{RegionToken, num};

/// A single argument to the `verify` subcommand. Three input shapes:
///
///   - `<addr>+<size>` / `chip` — dump the device-side MD5 for this region,
///     no local comparison.
///   - `<addr> <file>` (pairs) — read the device's MD5 over the file-sized
///     range starting at `<addr>` and compare it against MD5 of `<file>`.
///   - `<file>.hex` — same as above but addresses come from the HEX file.
///
/// The three shapes can't be mixed within one invocation.
#[derive(Clone, Debug)]
pub enum VerifyEntry {
    Region(RegionToken),
    Raw { addr: u32, path: PathBuf },
    Hex { path: PathBuf },
}

pub fn parse_verify_entries(tokens: &[String]) -> Result<Vec<VerifyEntry>, String> {
    if tokens.is_empty() {
        return Err("expected one or more regions or files".to_string());
    }

    // Region tokens are syntactically distinctive (`chip` or contains `+`),
    // so detection is by shape rather than by trial parsing — keeps the
    // error path for genuinely malformed regions (`0x0+`) intact.
    let looks_region = |t: &str| t.eq_ignore_ascii_case("chip") || t.contains('+');
    let looks_hex = |t: &str| t.to_ascii_lowercase().ends_with(".hex");

    if tokens.iter().all(|t| looks_region(t)) {
        return tokens
            .iter()
            .map(|t| {
                RegionToken::from_str(t)
                    .map(VerifyEntry::Region)
                    .map_err(|e| format!("invalid region `{}`: {}", t, e))
            })
            .collect();
    }

    if tokens.iter().all(|t| looks_hex(t)) {
        return Ok(tokens
            .iter()
            .map(|t| VerifyEntry::Hex { path: t.into() })
            .collect());
    }

    if tokens.iter().any(|t| looks_region(t) || looks_hex(t)) {
        return Err(
            "cannot mix region tokens, .hex files, and `<addr> <file>` pairs in one command"
                .to_string(),
        );
    }

    if tokens.len() % 2 != 0 {
        return Err(format!(
            "expected `<addr> <file>` pairs, got {} tokens",
            tokens.len()
        ));
    }

    tokens
        .chunks(2)
        .map(|chunk| {
            let addr = num::parse_u32(&chunk[0])
                .map_err(|e| format!("invalid address `{}`: {}", chunk[0], e))?;
            Ok(VerifyEntry::Raw {
                addr,
                path: PathBuf::from(&chunk[1]),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn region_only() {
        let entries = parse_verify_entries(&toks(&["0x0+4K", "chip"])).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(
            entries[0],
            VerifyEntry::Region(RegionToken::Range { .. })
        ));
        assert!(matches!(entries[1], VerifyEntry::Region(RegionToken::Chip)));
    }

    #[test]
    fn pair_compare() {
        let entries = parse_verify_entries(&toks(&["0x0", "app.bin", "1M", "res.bin"])).unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            VerifyEntry::Raw { addr, path } => {
                assert_eq!(*addr, 0);
                assert_eq!(path, &PathBuf::from("app.bin"));
            }
            _ => panic!(),
        }
        match &entries[1] {
            VerifyEntry::Raw { addr, .. } => assert_eq!(*addr, 0x100000),
            _ => panic!(),
        }
    }

    #[test]
    fn hex_only() {
        let entries = parse_verify_entries(&toks(&["a.hex", "b.HEX"])).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| matches!(e, VerifyEntry::Hex { .. })));
    }

    #[test]
    fn rejects_mixed() {
        assert!(parse_verify_entries(&toks(&["chip", "app.bin"])).is_err());
        assert!(parse_verify_entries(&toks(&["0x0+1M", "0x100000", "app.bin"])).is_err());
        assert!(parse_verify_entries(&toks(&["a.hex", "0x0", "b.bin"])).is_err());
    }

    #[test]
    fn rejects_odd_pair_count() {
        assert!(parse_verify_entries(&toks(&["0x0", "app.bin", "1M"])).is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_verify_entries(&toks(&[])).is_err());
    }
}
