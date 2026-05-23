use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

use core::fmt;
use cskburn::Image;

use crate::types::num;

/// A single write entry. Either a raw binary destined for an explicit
/// flash address, or a HEX file (which carries its own addresses).
///
/// A `write` invocation's positional arguments resolve to a single
/// `Vec<WriteEntry>` — either all `Raw` (parsed as `<addr> <file>` pairs)
/// or all `Hex` (each token is a `.hex` path). Mixing the two in one
/// command is rejected.
#[derive(Clone, Debug)]
pub enum WriteEntry {
    Raw { addr: u32, path: PathBuf },
    Hex { path: PathBuf },
}

impl WriteEntry {
    pub fn path(&self) -> &PathBuf {
        match self {
            WriteEntry::Raw { path, .. } => path,
            WriteEntry::Hex { path } => path,
        }
    }

    pub fn md5(&self) -> Result<[u8; 16], io::Error> {
        let mut file = File::open(self.path())?;
        let mut context = md5::Context::new();
        let mut buf = [0; 1024];
        loop {
            let count = file.read(&mut buf)?;
            if count == 0 {
                break;
            }
            context.consume(&buf[..count]);
        }
        Ok(context.finalize().0)
    }
}

impl fmt::Display for WriteEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WriteEntry::Raw { addr, path } => {
                write!(f, "0x{:08x} {}", addr, path.display())
            }
            WriteEntry::Hex { path } => write!(f, "{}", path.display()),
        }
    }
}

impl TryFrom<&WriteEntry> for Image {
    type Error = io::Error;

    fn try_from(spec: &WriteEntry) -> Result<Self, Self::Error> {
        match spec {
            WriteEntry::Raw { addr, path } => Image::try_from_file(
                *addr,
                path.to_str().ok_or(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "non-UTF-8 path",
                ))?,
            ),
            WriteEntry::Hex { .. } => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "HEX files must be resolved via hex::parse_hex",
            )),
        }
    }
}

/// Parse the positional argument list of `write` into a homogeneous entry list.
///
/// Either every token ends in `.hex` (HEX mode — one entry per token) or
/// the list is interpreted as alternating `<addr> <file>` pairs. Mixing
/// the two forms is rejected.
pub fn parse_write_entries(tokens: &[String]) -> Result<Vec<WriteEntry>, String> {
    if tokens.is_empty() {
        return Err("expected one or more files".to_string());
    }

    let all_hex = tokens
        .iter()
        .all(|t| t.to_ascii_lowercase().ends_with(".hex"));
    if all_hex {
        return Ok(tokens
            .iter()
            .map(|t| WriteEntry::Hex { path: t.into() })
            .collect());
    }

    if tokens
        .iter()
        .any(|t| t.to_ascii_lowercase().ends_with(".hex"))
    {
        return Err(
            "cannot mix .hex files with raw `<addr> <file>` pairs in one command".to_string(),
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
            Ok(WriteEntry::Raw {
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
    fn parse_pairs() {
        let entries =
            parse_write_entries(&toks(&["0x0", "app.bin", "0x100000", "res.bin"])).unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            WriteEntry::Raw { addr, path } => {
                assert_eq!(*addr, 0);
                assert_eq!(path, &PathBuf::from("app.bin"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_pairs_with_units() {
        let entries = parse_write_entries(&toks(&["1M", "app.bin"])).unwrap();
        match &entries[0] {
            WriteEntry::Raw { addr, .. } => assert_eq!(*addr, 0x100000),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_hex_only() {
        let entries = parse_write_entries(&toks(&["a.hex", "b.HEX"])).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], WriteEntry::Hex { .. }));
    }

    #[test]
    fn rejects_odd_count() {
        assert!(parse_write_entries(&toks(&["0x0", "app.bin", "0x100000"])).is_err());
    }

    #[test]
    fn rejects_mixed() {
        assert!(parse_write_entries(&toks(&["0x0", "app.bin", "b.hex"])).is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_write_entries(&toks(&[])).is_err());
    }
}
