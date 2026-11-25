use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, io, path::{Path, PathBuf}, process::Command, sync::Mutex};
use tauri::{Emitter, Manager};

const REPO: &str = "fbdn-fantasy/Photo-Viewer-Advanced";
const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
static UPDATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

#[derive(Clone, Copy)]
enum Format { Windows, Deb, Rpm, Tar }

impl Format {
    fn filename(self, version: &Version) -> String {
        match self {
            Self::Windows => format!("Photo-Viewer-Advanced-V{version}-Setup.exe"),
            Self::Deb => format!("Photo-Viewer-Advanced-V{version}.deb"),
            Self::Rpm => format!("photo-viewer-advanced-{version}-1.x86_64.rpm"),
            Self::Tar => format!("Photo-Viewer-Advanced-V{version}.tar.gz"),
        }
    }
}

fn installed_format() -> Result<Format, String> {
    #[cfg(windows)]
    { Ok(Format::Windows) }
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        match fs::read_to_string(exe.with_file_name("install-format")).unwrap_or_default().trim() {
            "deb" => Ok(Format::Deb),
            "rpm" => Ok(Format::Rpm),
            "tar" => Ok(Format::Tar),
            _ => Err("Install the viewer from a release package to enable in-app updates.".into()),
        }
    }
}

fn newer_release(release: &Release, current: &Version) -> Result<Option<Version>, String> {
    // The previous release used the abbreviated tag V3 before version numbering restarted.
    if release.draft || release.prerelease || release.tag_name == "V3" { return Ok(None); }
    let tag = release.tag_name.trim_start_matches(['v', 'V']);
    let version = Version::parse(tag).map_err(|_| "The latest release has an unrecognized version tag.")?;
    Ok((version.pre.is_empty() && version > *current).then_some(version))
}

fn select_asset<'a>(release: &'a Release, format: Format, version: &Version) -> Result<&'a Asset, String> {
    let name = format.filename(version);
    let expected = format!("https://github.com/{REPO}/releases/download/{}/{name}", release.tag_name);
    release.assets.iter().find(|a| a.name == name && a.browser_download_url == expected)
        .ok_or_else(|| format!("V{version} is available. Its {name} installer has not been published yet."))
}

fn download(url: &str, path: &Path, limit: u64, timeout: &str) -> Result<String, String> {
    #[cfg(windows)]
    let program = PathBuf::from(std::env::var_os("SystemRoot").ok_or("Windows system directory unavailable.")?)
        .join("System32/curl.exe");
    #[cfg(target_os = "linux")]
    let program = PathBuf::from("/usr/bin/curl");
    let mut cmd = Command::new(program);
    #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
    let output = cmd.args(["--disable", "--silent", "--show-error", "--location", "--fail",
        "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "5", "--connect-timeout", "15",
        "--max-time", timeout, "--max-filesize", &limit.to_string(), "--user-agent", "Photo-Viewer-Advanced",
        "--write-out", "%{http_code}", "--output"]).arg(path).arg(url).output()
        .map_err(|_| "The system HTTPS downloader (curl) could not start.")?;
    let code = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() && code != "404" {
        return Err(match code.as_str() {
            "403" | "429" => "GitHub limited this request. Try again later.".into(),
            "000" => "Could not connect to GitHub. Check your internet connection and try again.".into(),
            _ => format!("GitHub download failed (HTTP {code}). Try again later."),
        });
    }
    Ok(code)
}

fn verify(path: &Path, asset: &Asset) -> Result<(), String> {
    let expected = asset.digest.as_deref().and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("GitHub has not provided a SHA-256 digest for this installer. Try again later.")?;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let size = io::copy(&mut file, &mut hash).map_err(|e| e.to_string())?;
    if size != asset.size || !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected) {
        return Err("The downloaded installer failed its integrity check. Please try again.".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install(format: Format, path: &Path, version: &Version) -> Result<(), String> {
    let mut command = match format {
        Format::Deb | Format::Rpm => {
            let mut command = Command::new("/usr/bin/pkexec");
            if matches!(format, Format::Deb) {
                command.args(["/usr/bin/apt-get", "--no-remove", "-y", "install"]);
            } else {
                command.args(["/usr/bin/dnf", "-y", "install"]);
            }
            command.arg(path);
            command
        },
        Format::Tar => {
            let root = path.parent().ok_or("Update directory unavailable.")?;
            let result = Command::new("/usr/bin/tar").arg("-xzf").arg(path).arg("-C").arg(root)
                .args(["--no-same-owner", "--no-same-permissions"]).status().map_err(|e| e.to_string())?;
            if !result.success() { return Err("Could not unpack the verified update.".into()); }
            let mut command = Command::new("/bin/sh");
            command.arg(root.join(format!("Photo-Viewer-Advanced-V{version}")).join("install-user.sh"));
            command
        },
        Format::Windows => return Err("Incompatible package.".into()),
    };
    let status = command.status().map_err(|_| "Could not start the system package installer.")?;
    if !status.success() { return Err("Installation was cancelled or the package manager could not finish. You can retry the update.".into()); }
    Ok(())
}

fn update(app: &tauri::AppHandle) -> Result<String, String> {
    let _guard = UPDATE_LOCK.try_lock().map_err(|_| "An update check is already running.")?;
    let current = &app.package_info().version;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let dir = tempfile::tempdir_in(&cache).map_err(|e| e.to_string())?;
    let metadata = dir.path().join("release.json");
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let code = download(&url, &metadata, 4 * 1024 * 1024, "30")?;
    if code == "404" { return Ok("There are no published releases to install.".into()); }
    let data = fs::read(metadata).map_err(|e| e.to_string())?;
    let release: Release = serde_json::from_slice(&data).map_err(|_| "GitHub returned unreadable release information.")?;
    let Some(version) = newer_release(&release, current)? else {
        return Ok(format!("You’re up to date. Installed version: V{current}."));
    };
    let format = installed_format()?;
    let asset = select_asset(&release, format, &version)?;
    if asset.size == 0 || asset.size > MAX_PACKAGE { return Err("Unexpected installer size.".into()); }
    let _ = app.emit("update-status", format!("Downloading V{version}…"));
    let path = dir.path().join(&asset.name);
    if download(&asset.browser_download_url, &path, asset.size, "1200")? != "200" {
        return Err("The installer is no longer available. Try checking again.".into());
    }
    verify(&path, asset)?;
    let _ = app.emit("update-status", format!("Installing V{version}…"));
    #[cfg(windows)]
    {
        // NSIS needs its executable after the viewer exits. Reuse one cache slot on subsequent updates.
        let installer = cache.join("verified-setup.exe");
        fs::rename(&path, &installer).map_err(|e| format!("Could not stage the installer: {e}"))?;
        Command::new(installer).args(["/P", "/R"]).spawn().map_err(|_| "Could not launch the downloaded installer.")?;
        app.exit(0);
    }
    #[cfg(target_os = "linux")]
    install(format, &path, &version)?;
    Ok(format!("V{version} installed. Reopen Photo Viewer Advanced to use the update."))
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || update(&app)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> Release {
        Release { tag_name: tag.into(), draft: false, prerelease: false, assets: vec![] }
    }
    #[test]
    fn stable_versions_only() {
        let current = Version::new(1, 0, 9);
        for tag in ["V1.0.9", "v1.0.8", "v1.0.10-beta.1", "V3"] {
            assert!(newer_release(&release(tag), &current).unwrap().is_none());
        }
        assert_eq!(newer_release(&release("V1.0.10"), &current).unwrap(), Some(Version::new(1,0,10)));
    }
    #[test]
    fn verifies_download_and_pinned_asset_url() {
        let version = Version::new(1,0,1);
        for format in [Format::Windows, Format::Deb, Format::Rpm, Format::Tar] {
            let mut r = release("V1.0.1");
            let name = format.filename(&version);
            r.assets.push(Asset { browser_download_url: format!("https://github.com/{REPO}/releases/download/V1.0.1/{name}"), name, size: 3, digest: Some(format!("sha256:{:x}", Sha256::digest(b"abc"))) });
            let f = tempfile::NamedTempFile::new().unwrap();
            fs::write(f.path(), b"abc").unwrap();
            verify(f.path(), select_asset(&r, format, &version).unwrap()).unwrap();
            fs::write(f.path(), b"abd").unwrap();
            assert!(verify(f.path(), &r.assets[0]).is_err());
            r.assets[0].browser_download_url = "https://example.org/setup".into();
            assert!(select_asset(&r, format, &version).is_err());
        }
    }
}

