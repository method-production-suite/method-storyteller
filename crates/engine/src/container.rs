//! The `.fadein` container: a ZIP archive holding one Open Screenplay Format document,
//! `document.xml`. Only what that needs: reading a stored or deflated `document.xml`, and
//! writing a one-file archive. Sizes are checked against the input limit before inflating.

use std::io::{Read, Write};

use storyteller_core::MAX_INPUT_BYTES;

/// The member that holds the document.
const DOCUMENT: &str = "document.xml";

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Whether bytes start like a ZIP archive.
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// The `document.xml` inside a `.fadein` archive, as bytes.
pub fn read_document(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let damaged = || "the .fadein archive is damaged".to_string();
    // The end-of-central-directory record sits in the last 22 bytes plus any comment.
    let search_from = bytes.len().saturating_sub(22 + 65_535);
    let end = (search_from..bytes.len().saturating_sub(21)).rev().find(|&at| u32_at(bytes, at) == Some(0x0605_4b50)).ok_or_else(damaged)?;
    let entries = usize::from(u16_at(bytes, end + 10).ok_or_else(damaged)?);
    let mut at = usize::try_from(u32_at(bytes, end + 16).ok_or_else(damaged)?).map_err(|_| damaged())?;
    for _ in 0..entries {
        if u32_at(bytes, at) != Some(0x0201_4b50) {
            return Err(damaged());
        }
        let method = u16_at(bytes, at + 10).ok_or_else(damaged)?;
        let compressed = usize::try_from(u32_at(bytes, at + 20).ok_or_else(damaged)?).map_err(|_| damaged())?;
        let size = usize::try_from(u32_at(bytes, at + 24).ok_or_else(damaged)?).map_err(|_| damaged())?;
        let name_len = usize::from(u16_at(bytes, at + 28).ok_or_else(damaged)?);
        let extra_len = usize::from(u16_at(bytes, at + 30).ok_or_else(damaged)?);
        let comment_len = usize::from(u16_at(bytes, at + 32).ok_or_else(damaged)?);
        let local = usize::try_from(u32_at(bytes, at + 42).ok_or_else(damaged)?).map_err(|_| damaged())?;
        let name = bytes.get(at + 46..at + 46 + name_len).ok_or_else(damaged)?;
        let name = String::from_utf8_lossy(name);
        at += 46 + name_len + extra_len + comment_len;
        if name != DOCUMENT && !name.ends_with("/document.xml") {
            continue;
        }
        if size > MAX_INPUT_BYTES {
            return Err("the script is larger than the size limit".into());
        }
        if u32_at(bytes, local) != Some(0x0403_4b50) {
            return Err(damaged());
        }
        let data_start =
            local + 30 + usize::from(u16_at(bytes, local + 26).ok_or_else(damaged)?) + usize::from(u16_at(bytes, local + 28).ok_or_else(damaged)?);
        let data = bytes.get(data_start..data_start.saturating_add(compressed)).ok_or_else(damaged)?;
        return match method {
            0 => Ok(data.to_vec()),
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(data).take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut out).map_err(|_| damaged())?;
                if out.len() > MAX_INPUT_BYTES {
                    return Err("the script is larger than the size limit".into());
                }
                Ok(out)
            }
            _ => Err("the .fadein archive uses a compression method this app cannot read".into()),
        };
    }
    Err("the .fadein archive has no document.xml".into())
}

/// A one-file `.fadein` archive holding `document`. Timestamps are fixed, so the same document
/// always gives the same bytes.
pub fn write_document(document: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(document)?;
    let compressed = encoder.finish()?;
    let crc = crc32fast::hash(document);
    let too_big = || std::io::Error::other("the script is too large for a .fadein archive");
    let size = u32::try_from(document.len()).map_err(|_| too_big())?;
    let compressed_size = u32::try_from(compressed.len()).map_err(|_| too_big())?;
    let name = DOCUMENT.as_bytes();
    let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
    // 1980-01-01 00:00 in DOS format.
    let (time, date) = (0u16, 0x0021u16);

    let mut out = Vec::with_capacity(compressed.len() + 128);
    let local = |out: &mut Vec<u8>| {
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&time.to_le_bytes());
        out.extend_from_slice(&date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&compressed_size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
    };
    local(&mut out);
    out.extend_from_slice(&compressed);
    let central = u32::try_from(out.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&time.to_le_bytes());
    out.extend_from_slice(&date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&compressed_size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(name);
    let central_size = u32::try_from(out.len()).map_err(|_| too_big())? - central;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_round_trip() {
        let document = b"<?xml version=\"1.0\"?><document version=\"30\"></document>".repeat(20);
        let archive = write_document(&document).unwrap_or_default();
        assert!(is_zip(&archive));
        assert_eq!(read_document(&archive), Ok(document.clone()));
        assert_eq!(write_document(&document).unwrap_or_default(), archive, "the same document gives the same bytes");
    }

    #[test]
    fn damaged_archives_are_refused() {
        assert!(read_document(b"PK\x03\x04 not really a zip").is_err());
        let archive = write_document(b"<document/>").unwrap_or_default();
        assert!(read_document(archive.get(..archive.len() - 10).unwrap_or_default()).is_err());
    }
}
