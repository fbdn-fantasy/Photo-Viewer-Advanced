//! Small container scanners. No pixel buffers are allocated here.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn skip_blocks(f: &mut File) -> std::io::Result<()> {
    loop { let mut n = [0]; f.read_exact(&mut n)?; if n[0] == 0 { return Ok(()); }
        f.seek(SeekFrom::Current(n[0] as i64))?; }
}

pub fn is_animated(path: &Path, format: &str) -> bool {
    let Ok(mut f) = File::open(path) else { return false; };
    match format {
        "GIF" => {
            let mut header = [0u8; 13]; if f.read_exact(&mut header).is_err() { return false; }
            if header[10] & 0x80 != 0 { let _ = f.seek(SeekFrom::Current(3 * (1i64 << ((header[10] & 7) + 1)))); }
            let mut count = 0;
            // Stop at two frames. We only need to distinguish a still from an animation.
            loop {
                let mut tag = [0]; if f.read_exact(&mut tag).is_err() { break; }
                match tag[0] {
                    0x2c => {
                        count += 1; if count > 1 { return true; }
                        let mut desc = [0u8; 9]; if f.read_exact(&mut desc).is_err() { break; }
                        if desc[8] & 0x80 != 0 { let _ = f.seek(SeekFrom::Current(3 * (1i64 << ((desc[8] & 7) + 1)))); }
                        if f.seek(SeekFrom::Current(1)).is_err() || skip_blocks(&mut f).is_err() { break; }
                    },
                    0x21 => { if f.seek(SeekFrom::Current(1)).is_err() || skip_blocks(&mut f).is_err() { break; } },
                    _ => break,
                }
            }
            false
        },
        "PNG" | "APNG" => {
            let _ = f.seek(SeekFrom::Start(8));
            for _ in 0..100_000 {
                let mut chunk = [0u8; 8]; if f.read_exact(&mut chunk).is_err() { break; }
                let len = u32::from_be_bytes(chunk[0..4].try_into().unwrap()) as i64;
                if &chunk[4..8] == b"acTL" { let mut n = [0; 4]; return f.read_exact(&mut n).is_ok() && u32::from_be_bytes(n) > 1; }
                if &chunk[4..8] == b"IEND" { break; }
                if f.seek(SeekFrom::Current(len + 4)).is_err() { break; }
            }
            false
        },
        "WEBP" => {
            let _ = f.seek(SeekFrom::Start(12));
            for _ in 0..100_000 {
                let mut chunk = [0u8; 8]; if f.read_exact(&mut chunk).is_err() { break; }
                let len = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as i64;
                if &chunk[0..4] == b"ANIM" { return true; }
                if f.seek(SeekFrom::Current(len + (len % 2))).is_err() { break; }
            }
            false
        },
        "AVIF" | "HEIC" | "HEIF" => {
            // avis/msf1 are sequence brands; do not silently flatten these files.
            let mut buf = [0u8; 512]; let n = f.read(&mut buf).unwrap_or(0);
            buf[..n].windows(4).any(|v| v == b"avis" || v == b"msf1" || v == b"hevs" || v == b"hevm")
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn damaged_files_do_not_panic() {
        let file = tempfile::NamedTempFile::new().unwrap();
        for format in ["GIF", "PNG", "WEBP", "AVIF", "HEIC"] { assert!(!is_animated(file.path(), format)); }
    }
    #[test] fn scans_apng_frame_count() {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend_from_slice(&8u32.to_be_bytes()); b.extend_from_slice(b"acTL");
        b.extend_from_slice(&2u32.to_be_bytes()); b.extend_from_slice(&0u32.to_be_bytes());
        let file = tempfile::NamedTempFile::new().unwrap(); std::fs::write(file.path(), b).unwrap();
        assert!(is_animated(file.path(), "PNG"));
    }
}
