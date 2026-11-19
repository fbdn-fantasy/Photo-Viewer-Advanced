//! Decoder boundary: native browser codecs receive their original bytes; established
//! ImageMagick delegates supply safe, color-converted previews for additional formats.
use crate::{animation, container};
use serde::{Deserialize, Serialize};
use std::{collections::hash_map::DefaultHasher, fs, hash::{Hash, Hasher}, io::Read,
    path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::{Duration, Instant, SystemTime}};

pub const EXTENSIONS: &[&str] = &["jpg","jpeg","jpe","jfif","png","apng","bmp","dib","tif","tiff","gif","webp","avif","heic","heif","hif","ico","jxl","svg","psd","dds","tga","exr","hdr","qoi","jp2","j2k","jpf","jpx","dng","cr2","cr3","nef","arw","raf","orf","rw2","pef","srw","ppm","pgm","pbm","pnm"];
pub const SAVE_EXTENSIONS: &[&str] = &["png","jpg","jpeg","webp","tif","tiff","bmp","qoi","tga","avif","jxl","jp2","ico","gif","exr","hdr","ppm"];

#[derive(Clone)]
pub struct Decoder { pub executable: PathBuf, pub cache: PathBuf, pub canceled: Option<Arc<AtomicBool>> }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub path: String, pub display_path: String, pub name: String,
    pub width: u32, pub height: u32, pub format: String,
    pub animated: bool, pub animation_supported: bool,
    pub warning: Option<String>, pub can_save_original: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Crop { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    pub path: String, pub destination: String, #[serde(default)] pub overwrite: bool,
    #[serde(default)] pub rotate: i32,
    #[serde(default)] pub flip_horizontal: bool, #[serde(default)] pub flip_vertical: bool,
    pub crop: Option<Crop>, pub frame_index: Option<u32>, pub frame_png: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct Metadata { pub width: u32, pub height: u32, pub format: String, pub depth: u32, pub image_count: u32 }

pub fn path_string(path: &Path) -> String {
    let s = path.to_string_lossy();
    if let Some(unc) = s.strip_prefix(r"\\?\UNC\") { format!(r"\\{}", unc) }
    else { s.strip_prefix(r"\\?\").unwrap_or(&s).to_string() }
}

pub fn source_path(value: &str) -> Result<PathBuf, String> {
    let p = fs::canonicalize(value).map_err(|e| format!("Cannot open this file: {e}"))?;
    if !p.is_file() { return Err("Choose an image file, or use Open folder to browse a folder.".into()); }
    if fs::metadata(&p).map_err(|e| e.to_string())?.len() == 0 { return Err("This file is empty and contains no image.".into()); }
    Ok(p)
}

pub fn extension(p: &Path) -> String { p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase() }

impl Decoder {
    pub fn new(executable: PathBuf, cache: PathBuf) -> Result<Self, String> {
        if !executable.is_file() { return Err("The bundled image decoder is missing. Please reinstall Photo Viewer Advanced.".into()); }
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        Ok(Self { executable, cache, canceled: None })
    }

    /// Invoke without a command shell. Each job has isolated input/output files and a deadline.
    pub fn execute(&self, args: &[String], working: &Path, seconds: u64) -> Result<String, String> {
        if self.is_canceled() { return Err("Opening canceled because a newer image was selected.".into()); }
        let mut cmd = Command::new(&self.executable);
        cmd.args(args).current_dir(working)
            .env("MAGICK_CONFIGURE_PATH", self.executable.parent().unwrap())
            .env("MAGICK_TEMPORARY_PATH", working)
            .env("MAGICK_THREAD_LIMIT", "2")
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start the image decoder: {e}"))?;
        fn read_bounded(mut stream: impl Read) -> Vec<u8> {
            let mut all = Vec::new(); let mut buf = [0; 8192];
            while let Ok(n) = stream.read(&mut buf) { if n == 0 { break; }
                let take = n.min(1024 * 1024usize - all.len()); all.extend_from_slice(&buf[..take]); }
            all
        }
        let stdout = child.stdout.take().unwrap(); let stderr = child.stderr.take().unwrap();
        let out_thread = thread::spawn(move || read_bounded(stdout));
        let err_thread = thread::spawn(move || read_bounded(stderr));
        let started = Instant::now();
        let status = loop {
            if self.is_canceled() { let _ = child.kill(); let _ = child.wait(); break Err("Opening canceled because a newer image was selected.".into()); }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if started.elapsed() < Duration::from_secs(seconds) => thread::sleep(Duration::from_millis(15)),
                Ok(None) => { let _ = child.kill(); let _ = child.wait(); break Err("This image exceeded the decoding time limit. It may be too complex or damaged.".to_string()); },
                Err(e) => { let _ = child.kill(); let _ = child.wait(); break Err(e.to_string()); }
            }
        };
        let stdout = out_thread.join().unwrap_or_default(); let stderr = err_thread.join().unwrap_or_default();
        let status = status?;
        if !status.success() {
            let detail = String::from_utf8_lossy(&stderr).chars().take(700).collect::<String>();
            return Err(format!("The image decoder could not read or write this image. This format variant may be unsupported, damaged, or exceed the resource limit.\n{detail}"));
        }
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    }

    fn is_canceled(&self) -> bool { self.canceled.as_ref().is_some_and(|v| v.load(Ordering::Relaxed)) }

    /// Safe names prevent ImageMagick interpreting brackets, percent signs, or coder prefixes
    /// in real filenames. Hard links avoid copying where the source volume permits them.
    fn stage(&self, source: &Path, dir: &Path) -> Result<PathBuf, String> {
        let mut header = [0u8; 512];
        let n = fs::File::open(source).and_then(|mut f| f.read(&mut header)).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&header[..n]);
        let markup = text.trim_start_matches('\u{feff}').trim_start().starts_with('<');
        let svg = extension(source) == "svg" || markup;
        if svg { validate_svg(source)?; }
        let ext = if svg { "svg".into() } else { extension(source) };
        let ext = if ext.chars().all(|c| c.is_ascii_alphanumeric()) { ext } else { "img".into() };
        let staged = dir.join(format!("source.{ext}"));
        if fs::hard_link(source, &staged).is_err() { fs::copy(source, &staged).map_err(|e| format!("Cannot prepare this image for decoding: {e}"))?; }
        Ok(staged)
    }

    fn input(&self, staged: &Path) -> String {
        let prefix = if extension(staged) == "svg" { "RSVG:" } else { "" };
        let index = container::icon_representation(staged).map(|icon| icon.0).unwrap_or(0);
        format!("{prefix}{}[{index}]", path_string(staged))
    }

    pub fn metadata(&self, source: &Path) -> Result<Metadata, String> {
        let work = tempfile::Builder::new().prefix("job-").tempdir_in(&self.cache).map_err(|e| e.to_string())?;
        let staged = self.stage(source, work.path())?;
        let input = self.input(&staged);
        let out = self.execute(&["identify".into(), "-ping".into(), "-format".into(), "%w|%h|%m|%[orientation]|%z|%W|%H".into(), input], work.path(), 30)?;
        let parts: Vec<_> = out.trim().split('|').collect();
        if parts.len() < 5 { return Err("The image decoder returned incomplete image information.".into()); }
        let mut width: u32 = parts[0].parse().map_err(|_| "Invalid image width")?;
        let mut height: u32 = parts[1].parse().map_err(|_| "Invalid image height")?;
        if parts[2] == "GIF" && parts.len() >= 7 {
            width = width.max(parts[5].parse().unwrap_or(0)); height = height.max(parts[6].parse().unwrap_or(0));
        }
        if ["LeftTop", "RightTop", "RightBottom", "LeftBottom"].contains(&parts[3]) { std::mem::swap(&mut width, &mut height); }
        if width == 0 || height == 0 { return Err("The image has no displayable pixels.".into()); }
        let format = if parts[2] == "RSVG" { "SVG".into() } else { parts[2].to_uppercase() };
        // JPEG XL does not expose its animation flag in a simple byte signature. The
        // bundled decoder inspects its complete bounded frame directory. AVIF/HEIF
        // counts also distinguish multi-still collections from a single image.
        let image_count = if ["JXL", "AVIF", "HEIC", "HEIF"].contains(&format.as_str()) {
            let count = self.execute(&["identify".into(), "-ping".into(), "-format".into(), "%n\\n".into(), path_string(&staged)], work.path(), 30)?;
            count.lines().next().and_then(|v|v.trim().parse().ok()).unwrap_or(1)
        } else { 1 };
        Ok(Metadata { width, height, format, depth: parts[4].parse().unwrap_or(8), image_count })
    }

    fn key(&self, source: &Path, kind: &str) -> Result<PathBuf, String> {
        let m = fs::metadata(source).map_err(|e| e.to_string())?;
        let mut h = DefaultHasher::new(); source.hash(&mut h); m.len().hash(&mut h); m.modified().ok().hash(&mut h); kind.hash(&mut h);
        Ok(self.cache.join(format!("{kind}-{:016x}.png", h.finish())))
    }

    pub fn preview(&self, source: &Path, thumbnail: bool) -> Result<PathBuf, String> {
        let target = self.key(source, if thumbnail { "thumb" } else { "preview" })?;
        if target.is_file() { return Ok(target); }
        self.prune();
        let work = tempfile::Builder::new().prefix("job-").tempdir_in(&self.cache).map_err(|e| e.to_string())?;
        let staged = self.stage(source, work.path())?;
        let output = work.path().join("preview.png");
        let mut args = vec![self.input(&staged), "-auto-orient".into()];
        // Explicit profile conversion through LCMS; this also handles CMYK previews.
        let srgb = self.executable.parent().unwrap().join("sRGB.icc");
        if srgb.exists() { args.extend(["-profile".into(), path_string(&srgb)]); }
        args.extend(["-colorspace".into(), "sRGB".into(), "-resize".into(), if thumbnail { "240x160>".into() } else { "16384x16384>".into() }]);
        if !thumbnail { args.extend(["-resize".into(), "32000000@>".into()]); }
        if thumbnail { args.extend(["-strip".into(), "-depth".into(), "8".into()]); }
        args.extend(["-define".into(), "png:compression-level=1".into(), path_string(&output)]);
        self.execute(&args, work.path(), 90)?;
        // If another job completed the same cache entry first it can be reused.
        if !target.exists() { fs::rename(&output, &target).map_err(|e| e.to_string())?; }
        Ok(target)
    }

    pub fn open(&self, value: &str) -> Result<ImageInfo, String> {
        let source = source_path(value)?; let m = self.metadata(&source)?;
        let animated = animation::is_animated(&source, &m.format) || (m.format == "JXL" && m.image_count > 1);
        let native = ["PNG", "APNG", "JPEG", "JPG", "GIF", "WEBP", "BMP", "AVIF"].contains(&m.format.as_str());
        let oversized_file = fs::metadata(&source).map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024;
        let extreme = m.width as u64 * m.height as u64 > 64_000_000 || m.width > 32768 || m.height > 32768 || oversized_file;
        let supported_animation = native && !extreme;
        let mut warnings = Vec::new();
        let multiple_icon = container::icon_representation(&source).is_some_and(|(_, count)| count > 1);
        let multiple_tiff = m.format == "TIFF" && container::multiple_tiff_pages(&source);
        let multiple_stills = ["AVIF", "HEIC", "HEIF"].contains(&m.format.as_str()) && !animated && m.image_count > 1;
        if multiple_icon { warnings.push("Showing the largest ICO representation. Save As exports this representation; the original multi-resolution icon is preserved.".into()); }
        if multiple_tiff { warnings.push("Showing the first TIFF page. Save As exports this page; the original multi-page TIFF is preserved.".into()); }
        if m.format == "PSD" { warnings.push("Showing the PSD composite preview. Layer editing is not supported; Save As exports the visible composite.".into()); }
        if multiple_stills { warnings.push("Showing the primary image from a multi-image collection. Save As exports this image; the original collection is preserved.".into()); }
        let display = if native && !extreme { source.clone() } else {
            if animated { warnings.push("This image contains an animation that the bundled display decoder cannot play. Only a labeled still preview is available; the original file is unchanged.".to_string()); }
            if oversized_file { warnings.push("This file exceeds the 256 MB direct-playback limit. A bounded still preview is displayed to protect memory; edits and saves use the original resolution.".into()); }
            else if extreme { warnings.push("A reduced-resolution preview is displayed to keep this very large image responsive. Edits and saves use the original resolution.".into()); }
            if m.depth > 8 { warnings.push("High-bit-depth source retained. The on-screen preview is converted to sRGB; HDR monitor output is not supported in V1.".into()); }
            self.preview(&source, false)?
        };
        let ext = extension(&source);
        Ok(ImageInfo { path: path_string(&source), display_path: path_string(&display), name: source.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            width: m.width, height: m.height, format: m.format, animated, animation_supported: supported_animation,
            warning: if warnings.is_empty() { None } else { Some(warnings.join(" ")) },
            can_save_original: SAVE_EXTENSIONS.contains(&ext.as_str()) && !animated && !multiple_icon && !multiple_tiff && !multiple_stills })
    }

    pub fn save(&self, req: &SaveRequest) -> Result<String, String> {
        let source = source_path(&req.path)?;
        let target = PathBuf::from(&req.destination);
        if !target.is_absolute() { return Err("Choose an absolute destination path using Save As.".into()); }
        let ext = extension(&target);
        if !SAVE_EXTENSIONS.contains(&ext.as_str()) { return Err("This output format is not supported. Use Save As with PNG, JPEG, WebP, TIFF, BMP, QOI, TGA, AVIF, JXL, JPEG 2000, ICO, GIF, EXR or HDR.".into()); }
        if target.exists() && !req.overwrite { return Err("The destination already exists. Confirm overwriting it before saving.".into()); }
        if req.rotate % 90 != 0 { return Err("Only 90-degree rotations are supported.".into()); }
        let m = self.metadata(&source)?;
        let multiple_pages = container::icon_representation(&source).is_some_and(|(_, count)| count > 1)
            || (m.format == "TIFF" && container::multiple_tiff_pages(&source)) || m.image_count > 1;
        if multiple_pages && fs::canonicalize(&target).ok().as_ref() == Some(&source) { return Err("Use Save As to preserve this file's additional pages or icon representations. Only the displayed image is exported.".into()); }
        let animated = animation::is_animated(&source, &m.format) || (m.format == "JXL" && m.image_count > 1) || req.frame_index.is_some() || req.frame_png.is_some();
        if animated && (req.frame_index.is_none() || req.frame_png.is_none()) { return Err("Animated images require an explicitly selected still frame from playback. Use Save frame as to keep the original animation intact.".into()); }
        // Even explicit still-frame editing must never replace the animated original.
        if animated && fs::canonicalize(&target).ok().as_ref() == Some(&source) { return Err("Save the selected still frame to a different file; the original animation cannot be overwritten.".into()); }
        let parent = target.parent().ok_or("Invalid destination folder")?;
        if !parent.is_dir() { return Err("The destination folder does not exist.".into()); }
        let work = tempfile::Builder::new().prefix("pva-save-").tempdir_in(parent).map_err(|e| format!("Cannot write to this folder: {e}"))?;
        let staged = if animated {
            let png = req.frame_png.as_ref().unwrap();
            if png.len() > 128 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") { return Err("The selected animation frame is not a valid bounded PNG image.".into()); }
            let p = work.path().join("selected-frame.png"); fs::write(&p, png).map_err(|e| e.to_string())?;
            let captured = self.metadata(&p)?;
            if captured.width != m.width || captured.height != m.height { return Err("The captured animation frame dimensions do not match the source image.".into()); }
            p
        } else { self.stage(&source, work.path())? };
        let mut args = vec![self.input(&staged)];
        args.push("-auto-orient".into());
        let rotation = req.rotate.rem_euclid(360);
        if rotation != 0 { args.extend(["-rotate".into(), rotation.to_string()]); }
        if req.flip_horizontal { args.push("-flop".into()); }
        if req.flip_vertical { args.push("-flip".into()); }
        if let Some(crop) = &req.crop {
            let (w, h) = if rotation == 90 || rotation == 270 { (m.height, m.width) } else { (m.width, m.height) };
            if crop.width == 0 || crop.height == 0 || crop.x.checked_add(crop.width).is_none_or(|v| v > w) || crop.y.checked_add(crop.height).is_none_or(|v| v > h) { return Err("The crop rectangle is outside the image.".into()); }
            args.extend(["-crop".into(), format!("{}x{}+{}+{}", crop.width, crop.height, crop.x, crop.y), "+repage".into()]);
        }
        if ext == "jpg" || ext == "jpeg" { args.extend(["-background".into(), "white".into(), "-alpha".into(), "remove".into(), "-alpha".into(), "off".into(), "-quality".into(), "95".into()]); }
        if ext == "png" { args.extend(["-define".into(), "png:exclude-chunk=sRGB".into(), "-define".into(), "png:include-chunk=iCCP".into()]); }
        let output = work.path().join(format!("result.{ext}")); args.push(path_string(&output));
        self.execute(&args, work.path(), 120)?;
        // Verify encoded output before touching the destination.
        self.metadata(&output)?;
        atomic_replace(&output, &target, req.overwrite)?;
        Ok(path_string(&target))
    }

    /// Bound disk cache; active jobs are directories and deliberately excluded.
    pub fn prune(&self) {
        let Ok(read) = fs::read_dir(&self.cache) else { return; };
        let mut files: Vec<_> = read.flatten().filter_map(|e| { let m = e.metadata().ok()?; if !m.is_file() { return None; } Some((e.path(), m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH))) }).collect();
        files.sort_by_key(|x| x.2); let mut total: u64 = files.iter().map(|x| x.1).sum();
        for (path, bytes, _) in files { if total <= 256 * 1024 * 1024 { break; } if fs::remove_file(path).is_ok() { total = total.saturating_sub(bytes); } }
    }
}

pub fn atomic_replace(source: &Path, target: &Path, overwrite: bool) -> Result<(), String> {
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH | if overwrite { MOVEFILE_REPLACE_EXISTING } else { 0 };
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 { return Err(format!("Could not finish saving. The original was retained: {}", std::io::Error::last_os_error())); }
        Ok(())
    }
    #[cfg(not(windows))] {
        if target.exists() && !overwrite { return Err("Destination exists; overwrite was not confirmed.".into()); }
        fs::rename(source, target).map_err(|e| e.to_string())
    }
}

fn validate_svg(path: &Path) -> Result<(), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 { return Err("This SVG exceeds the 16 MB safe document limit.".into()); }
    let xml = fs::read_to_string(path).map_err(|e| format!("Cannot read SVG: {e}"))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("This SVG is not a valid self-contained document: {e}"))?;
    if doc.root_element().tag_name().name() != "svg" { return Err("This document is not an SVG image.".into()); }
    for node in doc.descendants() {
        if ["script", "foreignObject"].contains(&node.tag_name().name()) { return Err("SVG scripts and embedded HTML are not supported. Open a self-contained static SVG.".into()); }
        for attr in node.attributes() {
            if ["href", "src"].contains(&attr.name()) && !attr.value().starts_with('#') && !attr.value().starts_with("data:image/") { return Err("This SVG refers to another file or network resource. Only self-contained SVG images can be opened offline.".into()); }
        }
    }
    let lower = xml.to_lowercase();
    for after in lower.split("url(").skip(1) {
        let target = after.trim_start().trim_start_matches(['\'', '"']);
        if !target.starts_with('#') { return Err("SVG CSS references to external resources are not supported.".into()); }
    }
    Ok(())
}
