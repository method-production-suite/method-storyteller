#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Every kind of file the app opens, damaged at random: opening it either works or says why,
//! and never panics or runs away. Byte flips, cuts, insertions and repeated runs reach the ZIP
//! headers of `.mstory`, `.fadein` and `.highland` files as well as the XML and the text.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use storyteller_core::fountain;
use storyteller_core::layout::Layout;
use storyteller_engine::files::{self, FileFormat};

fn rng(mut seed: u64) -> impl FnMut(u64) -> u64 {
    move |n| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n.max(1)
    }
}

/// A stored ZIP with one member, as a `.highland` file may be.
fn stored_zip(member: &str, data: &[u8]) -> Vec<u8> {
    let name = member.as_bytes();
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    let crc = !crc;
    let (n, d) = (name.len() as u16, data.len() as u32);
    let mut out = Vec::new();
    out.write_all(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0").unwrap();
    for field in [crc, d, d] {
        out.write_all(&field.to_le_bytes()).unwrap();
    }
    out.write_all(&n.to_le_bytes()).unwrap();
    out.write_all(&[0, 0]).unwrap();
    out.write_all(name).unwrap();
    out.write_all(data).unwrap();
    let central = out.len() as u32;
    out.write_all(b"PK\x01\x02\x14\0\x14\0\0\0\0\0\0\0\0\0").unwrap();
    for field in [crc, d, d] {
        out.write_all(&field.to_le_bytes()).unwrap();
    }
    out.write_all(&n.to_le_bytes()).unwrap();
    out.write_all(&[0; 12]).unwrap();
    out.write_all(&0u32.to_le_bytes()).unwrap();
    out.write_all(name).unwrap();
    let size = out.len() as u32 - central;
    out.write_all(b"PK\x05\x06\0\0\0\0\x01\0\x01\0").unwrap();
    out.write_all(&size.to_le_bytes()).unwrap();
    out.write_all(&central.to_le_bytes()).unwrap();
    out.write_all(&[0, 0]).unwrap();
    out
}

/// One file of each kind the app opens, from a small script.
fn samples() -> Vec<(&'static str, Vec<u8>)> {
    let source = include_str!("../../core/tests/fixtures/lighthouse.fountain");
    let script = fountain::parse(source).unwrap();
    let layout = Layout::default();
    let encode = |format| files::encode(&script, format, &layout).unwrap();
    vec![
        ("fountain", source.as_bytes().to_vec()),
        ("fdx", encode(FileFormat::Fdx)),
        ("osf", encode(FileFormat::Osf)),
        ("fadein", encode(FileFormat::FadeIn)),
        ("mstory", encode(FileFormat::Mstory)),
        ("highland", stored_zip("Pilot.textbundle/text.fountain", source.as_bytes())),
    ]
}

/// `bytes` damaged in one of several ways.
fn damage(bytes: &[u8], next: &mut impl FnMut(u64) -> u64) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let len = out.len().max(1) as u64;
    match next(5) {
        0 => {
            for _ in 0..=next(8) {
                let at = next(len) as usize;
                if let Some(b) = out.get_mut(at) {
                    *b = next(256) as u8;
                }
            }
        }
        1 => out.truncate(next(len) as usize),
        2 => {
            let at = next(len) as usize;
            let junk: Vec<u8> = (0..next(64)).map(|_| next(256) as u8).collect();
            out.splice(at.min(out.len())..at.min(out.len()), junk);
        }
        3 => {
            // A run repeated: nesting, and ZIP records pointing at each other.
            let from = next(len) as usize;
            let to = (from + next(200) as usize).min(out.len());
            let run = out[from.min(to)..to].to_vec();
            for _ in 0..next(20) {
                out.splice(to..to, run.iter().copied());
            }
        }
        _ => {
            // A ZIP size or offset field set to something large.
            let at = next(len.saturating_sub(4).max(1)) as usize;
            if at + 4 <= out.len() {
                out[at..at + 4].copy_from_slice(&(u32::MAX - next(1 << 20) as u32).to_le_bytes());
            }
        }
    }
    out
}

#[test]
fn damaged_files_of_every_kind_open_or_say_why() {
    let dir = tempfile::tempdir().unwrap();
    let mut next = rng(0x5eed_f11e);
    let started = Instant::now();
    for (extension, bytes) in samples() {
        let path = dir.path().join(format!("sample.{extension}"));
        std::fs::write(&path, &bytes).unwrap();
        assert!(files::open(&path).is_ok(), "the undamaged .{extension} opens");
        for round in 0..1000 {
            let damaged = damage(&bytes, &mut next);
            std::fs::write(&path, &damaged).unwrap();
            let opened = std::panic::catch_unwind(|| files::open(Path::new(&path)).map(|(script, _)| script.elements.len()));
            assert!(opened.is_ok(), "damaged .{extension} (round {round}) panicked");
        }
    }
    assert!(started.elapsed() < Duration::from_secs(60), "damaged files open quickly: {:?}", started.elapsed());
}
