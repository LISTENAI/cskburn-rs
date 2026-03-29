use std::fs::read_to_string;
use std::io::Cursor;

use anyhow::{Result, anyhow};
use cskburn::{Image, Source};
use ihex::{Reader, Record};

/// A contiguous data segment extracted from an Intel HEX file.
pub struct HexSegment {
    pub addr: u32,
    data: Vec<u8>,
}

impl HexSegment {
    pub fn md5(&self) -> [u8; 16] {
        md5::compute(&self.data).0
    }
}

impl Into<Image> for HexSegment {
    fn into(self) -> Image {
        let source: Box<dyn Source> = Box::new(Cursor::new(self.data));
        Image::new(self.addr, source)
    }
}

/// Parse an Intel HEX file into a list of HexSegments.
///
/// The HEX file contains absolute addresses. The chip's `base_addr` is
/// subtracted to produce flash-relative offsets.
pub fn parse_hex(path: &str, base_addr: u32) -> Result<Vec<HexSegment>> {
    let content =
        read_to_string(path).map_err(|e| anyhow!("Failed to read HEX file {}: {}", path, e))?;

    let segments = assemble_segments(&content)?;

    segments
        .into_iter()
        .map(|seg| {
            let HexSegment { addr, data } = seg;
            let addr = addr.checked_sub(base_addr).ok_or_else(|| {
                anyhow!(
                    "HEX address 0x{:08x} is below chip base address 0x{:08x}",
                    addr,
                    base_addr
                )
            })?;
            Ok(HexSegment { addr, data })
        })
        .collect()
}

const FLASH_ALIGN: u32 = 4 * 1024;

fn align_up(addr: u32, align: u32) -> u32 {
    (addr + align - 1) & !(align - 1)
}

/// Assemble raw HEX records into contiguous memory segments.
///
/// When two segments have a small gap (within flash alignment boundary),
/// the gap is filled with 0xFF and the segments are merged.
fn assemble_segments(content: &str) -> Result<Vec<HexSegment>> {
    let reader = Reader::new(content);

    let mut segments: Vec<HexSegment> = Vec::new();
    let mut upper_addr: u32 = 0;

    for result in reader {
        let record = result.map_err(|e| anyhow!("Failed to parse HEX record: {}", e))?;

        match record {
            Record::ExtendedLinearAddress(ela) => {
                upper_addr = (ela as u32) << 16;
            }
            Record::ExtendedSegmentAddress(esa) => {
                upper_addr = (esa as u32) << 4;
            }
            Record::Data { offset, value } => {
                let addr = upper_addr + offset as u32;

                if let Some(last) = segments.last_mut() {
                    let last_end = last.addr + last.data.len() as u32;

                    // Contiguous: append directly
                    if addr == last_end {
                        last.data.extend_from_slice(&value);
                        continue;
                    }

                    // Small gap within alignment boundary: fill with 0xFF and merge
                    if addr > last_end && align_up(last_end, FLASH_ALIGN) >= addr {
                        let gap = (addr - last_end) as usize;
                        last.data.resize(last.data.len() + gap, 0xFF);
                        last.data.extend_from_slice(&value);
                        continue;
                    }
                }

                // Start a new segment
                segments.push(HexSegment { addr, data: value });
            }
            Record::EndOfFile => break,
            _ => {}
        }
    }

    if segments.is_empty() {
        return Err(anyhow!("HEX file contains no data records"));
    }

    Ok(segments)
}
