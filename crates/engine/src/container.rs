//! The `.fadein` container: a ZIP archive holding one Open Screenplay Format document,
//! `document.xml`. Only what that needs: reading a stored or deflated `document.xml`, and
//! writing a one-file archive. Sizes are checked against the input limit before inflating.

use std::io::{Read, Write};

use storyteller_core::{ArchiveEntry, MAX_INPUT_BYTES};

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

/// The `document.xml` inside a `.fadein` archive, as bytes, and the archive's other members
/// (kept compressed, to be written back unchanged).
pub fn read_document(bytes: &[u8]) -> Result<(Vec<u8>, Vec<ArchiveEntry>), String> {
    let damaged = || "the .fadein archive is damaged".to_string();
    // The end-of-central-directory record sits in the last 22 bytes plus any comment.
    let search_from = bytes.len().saturating_sub(22 + 65_535);
    let end = (search_from..bytes.len().saturating_sub(21)).rev().find(|&at| u32_at(bytes, at) == Some(0x0605_4b50)).ok_or_else(damaged)?;
    let entries = usize::from(u16_at(bytes, end + 10).ok_or_else(damaged)?);
    let mut at = usize::try_from(u32_at(bytes, end + 16).ok_or_else(damaged)?).map_err(|_| damaged())?;
    let mut document = None;
    let mut others = Vec::new();
    let mut total = 0usize;
    for _ in 0..entries {
        if u32_at(bytes, at) != Some(0x0201_4b50) {
            return Err(damaged());
        }
        let method = u16_at(bytes, at + 10).ok_or_else(damaged)?;
        let crc32 = u32_at(bytes, at + 16).ok_or_else(damaged)?;
        let compressed = usize::try_from(u32_at(bytes, at + 20).ok_or_else(damaged)?).map_err(|_| damaged())?;
        let size_field = u32_at(bytes, at + 24).ok_or_else(damaged)?;
        let size = usize::try_from(size_field).map_err(|_| damaged())?;
        let name_len = usize::from(u16_at(bytes, at + 28).ok_or_else(damaged)?);
        let extra_len = usize::from(u16_at(bytes, at + 30).ok_or_else(damaged)?);
        let comment_len = usize::from(u16_at(bytes, at + 32).ok_or_else(damaged)?);
        let local = usize::try_from(u32_at(bytes, at + 42).ok_or_else(damaged)?).map_err(|_| damaged())?;
        let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + name_len).ok_or_else(damaged)?).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        total = total.saturating_add(size).saturating_add(compressed);
        if total > MAX_INPUT_BYTES {
            return Err("the script is larger than the size limit".into());
        }
        if u32_at(bytes, local) != Some(0x0403_4b50) {
            return Err(damaged());
        }
        let data_start =
            local + 30 + usize::from(u16_at(bytes, local + 26).ok_or_else(damaged)?) + usize::from(u16_at(bytes, local + 28).ok_or_else(damaged)?);
        let data = bytes.get(data_start..data_start.saturating_add(compressed)).ok_or_else(damaged)?;
        let is_document = document.is_none() && (name == DOCUMENT || name.ends_with("/document.xml"));
        if !is_document {
            if !name.ends_with('/') {
                others.push(ArchiveEntry { name, method, crc32, size: size_field, data: data.to_vec() });
            }
            continue;
        }
        document = Some(match method {
            0 => data.to_vec(),
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(data).take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut out).map_err(|_| damaged())?;
                if out.len() > MAX_INPUT_BYTES {
                    return Err("the script is larger than the size limit".into());
                }
                out
            }
            _ => return Err("the .fadein archive uses a compression method this app cannot read".into()),
        });
    }
    let document = document.ok_or_else(|| "the .fadein archive has no document.xml".to_string())?;
    Ok((document, others))
}

/// A `.fadein` archive holding `document` first, then `others` exactly as they were read.
/// Timestamps are fixed, so the same input always gives the same bytes.
pub fn write_document(document: &[u8], others: &[ArchiveEntry]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(document)?;
    let too_big = || std::io::Error::other("the script is too large for a .fadein archive");
    let main = ArchiveEntry {
        name: DOCUMENT.into(),
        method: 8,
        crc32: crc32fast::hash(document),
        size: u32::try_from(document.len()).map_err(|_| too_big())?,
        data: encoder.finish()?,
    };
    // 1980-01-01 00:00 in DOS format.
    let (time, date) = (0u16, 0x0021u16);
    let mut out = Vec::new();
    let mut central = Vec::new();
    let entries: Vec<&ArchiveEntry> = std::iter::once(&main).chain(others).collect();
    for entry in &entries {
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let name = entry.name.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let compressed = u32::try_from(entry.data.len()).map_err(|_| too_big())?;
        let mut common = Vec::with_capacity(26);
        common.extend_from_slice(&20u16.to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        common.extend_from_slice(&entry.method.to_le_bytes());
        common.extend_from_slice(&time.to_le_bytes());
        common.extend_from_slice(&date.to_le_bytes());
        common.extend_from_slice(&entry.crc32.to_le_bytes());
        common.extend_from_slice(&compressed.to_le_bytes());
        common.extend_from_slice(&entry.size.to_le_bytes());
        common.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&common);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&entry.data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&common);
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = u32::try_from(out.len()).map_err(|_| too_big())?;
    let central_size = u32::try_from(central.len()).map_err(|_| too_big())?;
    let count = u16::try_from(entries.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_round_trip() {
        let document = b"<?xml version=\"1.0\"?><document version=\"30\"></document>".repeat(20);
        let picture = ArchiveEntry { name: "images/one.png".into(), method: 0, crc32: crc32fast::hash(b"png"), size: 3, data: b"png".to_vec() };
        let archive = write_document(&document, std::slice::from_ref(&picture)).unwrap_or_default();
        assert!(is_zip(&archive));
        assert_eq!(read_document(&archive), Ok((document.clone(), vec![picture.clone()])), "other members come back unchanged");
        assert_eq!(write_document(&document, &[picture]).unwrap_or_default(), archive, "the same input gives the same bytes");
    }

    #[test]
    fn damaged_archives_are_refused() {
        assert!(read_document(b"PK\x03\x04 not really a zip").is_err());
        let archive = write_document(b"<document/>", &[]).unwrap_or_default();
        assert!(read_document(archive.get(..archive.len() - 10).unwrap_or_default()).is_err());
    }
}
