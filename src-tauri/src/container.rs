//! Small directory readers avoid decoding every representation in container formats.
use std::{fs::File, io::{Read, Seek, SeekFrom}, path::Path};

pub fn icon_representation(path: &Path) -> Option<(usize, usize)> {
    let mut file = File::open(path).ok()?;
    let mut head = [0u8; 6]; file.read_exact(&mut head).ok()?;
    if head[..4] != [0, 0, 1, 0] { return None; }
    let count = u16::from_le_bytes([head[4], head[5]]) as usize;
    if count == 0 || count > 1024 { return None; }
    let mut best = (0usize, 0u32, 0u16);
    for index in 0..count {
        let mut entry = [0u8; 16]; file.read_exact(&mut entry).ok()?;
        let width = if entry[0] == 0 { 256 } else { u32::from(entry[0]) };
        let height = if entry[1] == 0 { 256 } else { u32::from(entry[1]) };
        let depth = u16::from_le_bytes([entry[6], entry[7]]);
        if (width * height, depth) > (best.1, best.2) { best = (index, width * height, depth); }
    }
    Some((best.0, count))
}

pub fn multiple_tiff_pages(path: &Path) -> bool {
    fn inspect(path: &Path) -> Option<bool> {
        let mut file = File::open(path).ok()?; let file_len = file.metadata().ok()?.len();
        let mut head = [0u8; 16]; file.read_exact(&mut head).ok()?;
        let little = match &head[..2] { b"II" => true, b"MM" => false, _ => return None };
        let u16n = |b: [u8; 2]| if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) };
        let u32n = |b: [u8; 4]| if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
        let u64n = |b: [u8; 8]| if little { u64::from_le_bytes(b) } else { u64::from_be_bytes(b) };
        let magic = u16n(head[2..4].try_into().ok()?);
        let (offset, count_bytes, entry_bytes, offset_bytes) = match magic {
            42 => (u32n(head[4..8].try_into().ok()?) as u64, 2u64, 12u64, 4u64),
            43 if u16n(head[4..6].try_into().ok()?) == 8 => (u64n(head[8..16].try_into().ok()?), 8, 20, 8),
            _ => return None,
        };
        if offset < 8 || offset > file_len.saturating_sub(count_bytes) { return None; }
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut count_data = [0u8; 8]; file.read_exact(&mut count_data[..count_bytes as usize]).ok()?;
        let count = if count_bytes == 2 { u16n(count_data[..2].try_into().ok()?) as u64 } else { u64n(count_data) };
        let next_pos = offset.checked_add(count_bytes)?.checked_add(count.checked_mul(entry_bytes)?)?;
        if next_pos > file_len.saturating_sub(offset_bytes) { return None; }
        file.seek(SeekFrom::Start(next_pos)).ok()?;
        let mut next = [0u8; 8]; file.read_exact(&mut next[..offset_bytes as usize]).ok()?;
        let next = if offset_bytes == 4 { u32n(next[..4].try_into().ok()?) as u64 } else { u64n(next) };
        Some(next != 0 && next < file_len)
    }
    inspect(path).unwrap_or(false)
}
