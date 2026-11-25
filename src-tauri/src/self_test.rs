//! The release executable can run deterministic backend checks without starting WebView2:
//! PhotoViewerAdvanced.exe --self-test <fixture-directory>
use crate::decoder::{self, Crop, Decoder, SaveRequest};
use serde::Serialize;
use std::{fs, path::PathBuf, time::Instant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Check { name: String, passed: bool, elapsed_ms: u128, detail: String }

pub fn run_cli() -> bool {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--self-test") { return false; }
    let Some(folder) = args.get(2).map(PathBuf::from) else { std::process::exit(2); };
    let result = run(folder.clone());
    match result {
        Ok(checks) => {
            let passed = checks.iter().all(|c| c.passed);
            let report = serde_json::json!({ "backend": format!("Photo Viewer Advanced {}", env!("CARGO_PKG_VERSION")), "passed": passed,
                "scope": "Real bundled decoder, metadata, pixels, transforms and overwrite safety. Browser animation rendering and Windows integration require separate tests.", "checks": checks });
            let _ = fs::write(folder.join("backend-test-report.json"), serde_json::to_vec_pretty(&report).unwrap());
            std::process::exit(if passed { 0 } else { 1 });
        },
        Err(e) => { let _ = fs::write(folder.join("backend-test-error.txt"), e); std::process::exit(2); }
    }
}

fn run(folder: PathBuf) -> Result<Vec<Check>, String> {
    let folder = fs::canonicalize(folder).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().unwrap();
    let decoder_path = [parent.join("resources/imagemagick/magick.exe"), parent.join("imagemagick/magick.exe")].into_iter().find(|p| p.exists()).ok_or("Decoder was not found")?;
    let output = folder.join("test-output"); fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let decoder = Decoder::new(decoder_path, output.join("cache"))?;
    let mut checks = Vec::new();
    let mut paths: Vec<_> = fs::read_dir(&folder).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    if let Ok(raws) = fs::read_dir(folder.join("raw")) { paths.extend(raws.flatten().map(|e|e.path()).filter(|p|p.is_file())); }
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !decoder::EXTENSIONS.contains(&decoder::extension(&path).as_str()) && name != "unsupported.xyz" { continue; }
        let start = Instant::now();
        let expect_failure = name.starts_with("corrupted") || name.starts_with("unsupported");
        let result = decoder.open(&decoder::path_string(&path)).and_then(|info| {
            // Pixel decoding is required for format verification, not just successful header sniffing.
            decoder.preview(&path, true)?;
            Ok(format!("{} {}×{}, animated={}, nativePlaybackEligible={}", info.format, info.width, info.height, info.animated, info.animation_supported))
        });
        let (passed, detail) = match result {
            Ok(info) => (!expect_failure, info), Err(e) => (expect_failure, e),
        };
        checks.push(Check { name, passed, elapsed_ms: start.elapsed().as_millis(), detail });
    }
    let standard = folder.join("standard.png");
    if standard.exists() {
        let source = decoder::path_string(&standard);
        let source_meta = decoder.metadata(&standard)?;
        let target = output.join("crop-rotate-flip.png");
        let request = SaveRequest { path: source.clone(), destination: decoder::path_string(&target), overwrite: true, rotate: 90, flip_horizontal: true, flip_vertical: true, crop: Some(Crop{x:1,y:2,width:32,height:24}), frame_index: None, frame_png: None };
        let started = Instant::now();
        let result = decoder.save(&request).and_then(|_| decoder.metadata(&target));
        checks.push(Check { name: "crop + 90° rotate + horizontal/vertical flip + lossless save".into(), passed: result.as_ref().is_ok_and(|m| m.width == 32 && m.height == 24), elapsed_ms: started.elapsed().as_millis(), detail: format!("{result:?}") });
        let original = fs::read(&target).map_err(|e| e.to_string())?;
        let mut refuse = request.clone(); refuse.overwrite = false;
        let rejected = decoder.save(&refuse).is_err();
        let unchanged = fs::read(&target).ok().as_ref() == Some(&original);
        checks.push(Check{name:"overwrite requires explicit confirmation and leaves existing bytes intact".into(),passed:rejected && unchanged,elapsed_ms:0,detail:format!("rejected={rejected}, unchanged={unchanged}")});
        let mut invalid = request.clone(); invalid.crop = Some(Crop{x:source_meta.height,y:0,width:1,height:1});
        checks.push(Check{name:"out-of-bounds crop rejected".into(),passed:decoder.save(&invalid).is_err(),elapsed_ms:0,detail:"Crop beyond rotated source boundary must fail".into()});
        // Verify rotations/flips against an independently assembled expected ImageMagick image.
        let expected = output.join("expected.png");
        decoder.execute(&[source.clone(), "-auto-orient".into(), "-rotate".into(), "90".into(), "-flop".into(), "-flip".into(), "-crop".into(), "32x24+1+2".into(), "+repage".into(), decoder::path_string(&expected)], &output, 30)?;
        let diff = decoder.execute(&[decoder::path_string(&target), decoder::path_string(&expected), "-compose".into(), "difference".into(), "-composite".into(), "-format".into(), "%[fx:mean]".into(), "info:".into()], &output, 30)?;
        checks.push(Check{name:"saved transform pixels match expected image".into(),passed:diff.trim().parse::<f64>().is_ok_and(|d|d.abs()<0.000001),elapsed_ms:0,detail:format!("mean pixel difference={diff}")});
        for ext in ["jpg", "webp", "tiff", "bmp", "qoi", "tga", "avif", "jxl", "jp2", "ico", "gif", "exr", "hdr"] {
            let start = Instant::now(); let mut r = request.clone(); r.destination=decoder::path_string(&output.join(format!("saved.{ext}"))); r.crop=None; r.rotate=0;
            let result = decoder.save(&r).and_then(|p|decoder.metadata(&PathBuf::from(p)));
            checks.push(Check{name:format!("Save As {ext}"),passed:result.is_ok(),elapsed_ms:start.elapsed().as_millis(),detail:format!("{result:?}")});
        }
    }
    for file in ["timing-disposal.gif", "timing-disposal.webp", "timing-disposal.apng", "animated-avif.avif", "animated-unsupported.jxl"] {
        let p = folder.join(file); if !p.exists() { continue; }
        let req = SaveRequest { path:decoder::path_string(&p),destination:decoder::path_string(&output.join(format!("flatten-{file}.png"))),overwrite:true,rotate:0,flip_horizontal:false,flip_vertical:false,crop:None,frame_index:None,frame_png:None };
        checks.push(Check{name:format!("{file}: implicit flattening rejected"),passed:decoder.save(&req).is_err(),elapsed_ms:0,detail:"No explicit captured still frame supplied".into()});
    }
    let animated_jxl=folder.join("animated-unsupported.jxl");
    if animated_jxl.exists() {
        let info=decoder.open(&decoder::path_string(&animated_jxl))?;
        checks.push(Check{name:"Animated JXL is explicitly labeled unsupported playback; still editing disabled".into(),passed:info.animated && !info.animation_supported && !info.can_save_original && info.warning.as_deref().is_some_and(|s|s.contains("cannot play")),elapsed_ms:0,detail:format!("animated={}, animationSupported={}, canSaveOriginal={}, warning={:?}",info.animated,info.animation_supported,info.can_save_original,info.warning)});
    }
    let oversized=output.join("oversized-encoded.png");
    if folder.join("standard.png").exists() {
        fs::copy(folder.join("standard.png"), &oversized).map_err(|e|e.to_string())?;
        fs::OpenOptions::new().write(true).open(&oversized).map_err(|e|e.to_string())?.set_len(256 * 1024 * 1024 + 1).map_err(|e|e.to_string())?;
        let info=decoder.open(&decoder::path_string(&oversized))?;
        let guarded=info.display_path!=info.path && !info.animation_supported && info.warning.as_deref().is_some_and(|s|s.contains("256 MB"));
        checks.push(Check{name:"Encoded files over 256 MB use a bounded preview".into(),passed:guarded,elapsed_ms:0,detail:format!("preview={}, warning={:?}",info.display_path!=info.path,info.warning)});
        let _=fs::remove_file(&oversized);
    }
    for file in ["multi-resolution.ico", "multi-page.tiff"] {
        let p = folder.join(file); if !p.exists() { continue; }
        let info = decoder.open(&decoder::path_string(&p))?;
        let req = SaveRequest { path:decoder::path_string(&p),destination:decoder::path_string(&p),overwrite:true,rotate:90,flip_horizontal:false,flip_vertical:false,crop:None,frame_index:None,frame_png:None };
        let protected = !info.can_save_original && decoder.save(&req).is_err();
        let display = decoder.metadata(&PathBuf::from(&info.display_path))?;
        checks.push(Check{name:format!("{file}: representation selection, preview dimensions and original preservation"),passed:protected && display.width==info.width && display.height==info.height && (file!="multi-resolution.ico" || info.width==256),elapsed_ms:0,detail:format!("{}×{}, preview={}×{}, protected={protected}, warning={:?}",info.width,info.height,display.width,display.height,info.warning)});
    }
    for file in ["orientation-6.jpg", "icc-profile.png", "high-bit-depth.png"] {
        let p = folder.join(file); if !p.exists() { continue; }
        let destination=output.join(format!("preserved-{file}.png"));
        let req=SaveRequest {path:decoder::path_string(&p),destination:decoder::path_string(&destination),overwrite:true,rotate:0,flip_horizontal:false,flip_vertical:false,crop:None,frame_index:None,frame_png:None};
        decoder.save(&req)?;
        let saved=decoder.metadata(&destination)?; let source=decoder.metadata(&p)?;
        let mut passed=saved.width==source.width && saved.height==source.height && saved.depth>=source.depth;
        let mut detail=format!("source={}×{} depth{}, saved={}×{} depth{}",source.width,source.height,source.depth,saved.width,saved.height,saved.depth);
        if file=="icc-profile.png" {
            let original_profile=output.join("original.icc"); let saved_profile=output.join("saved.icc");
            decoder.execute(&[decoder::path_string(&p),format!("icc:{}",decoder::path_string(&original_profile))],&output,30)?;
            decoder.execute(&[decoder::path_string(&destination),format!("icc:{}",decoder::path_string(&saved_profile))],&output,30)?;
            let original=fs::read(&original_profile).map_err(|e|e.to_string())?; let saved=fs::read(&saved_profile).map_err(|e|e.to_string())?;
            let equal=original==saved && !original.is_empty(); passed &= equal; detail.push_str(&format!(", ICC preserved byte-for-byte={equal} ({} bytes)",original.len()));
        }
        if file=="orientation-6.jpg" {
            let orientation=decoder.execute(&["identify".into(),"-format".into(),"%[orientation]|%[EXIF:Orientation]".into(),decoder::path_string(&destination)],&output,30)?;
            passed &= orientation.trim()=="TopLeft|1";detail.push_str(&format!(", normalized orientation={orientation}"));
        }
        checks.push(Check{name:format!("{file}: metadata/bit-depth preservation on save"),passed,elapsed_ms:0,detail});
    }
    let folder_selection = crate::resolve_open_path(&decoder::path_string(&folder))?;
    let first = crate::folder_entries(&decoder::path_string(&folder))?.into_iter().next().ok_or("Missing folder fixture")?;
    checks.push(Check{name:"Open folder resolves first naturally-sorted image".into(),passed:folder_selection==first.path,elapsed_ms:0,detail:first.name});
    let empty=output.join("empty-folder");fs::create_dir_all(&empty).map_err(|e|e.to_string())?;
    checks.push(Check{name:"Empty folder produces informative error".into(),passed:crate::resolve_open_path(&decoder::path_string(&empty)).is_err(),elapsed_ms:0,detail:"No supported entries".into()});
    Ok(checks)
}
