mod animation;
mod container;
mod decoder;
mod settings;
mod self_test;

use decoder::{Decoder, EXTENSIONS};
use serde::Serialize;
use settings::Settings;
use std::{fs, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}}};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::WindowExt;
use tokio::sync::Semaphore;

struct AppState {
    decoder: Decoder,
    settings_file: PathBuf,
    jobs: Arc<Semaphore>, thumbnails: Arc<Semaphore>, generation: AtomicU64,
    current_open: Mutex<Option<Arc<AtomicBool>>>,
}

// Managed before plugins start: Explorer can send a second launch while the
// first webview is still loading and has not registered its event listener.
struct StartupState(Mutex<StartupLifecycle>);

struct StartupLifecycle {
    pending_path: Option<String>,
    frontend_ready: bool,
    window_prepared: bool,
    window_shown: bool,
}

impl StartupLifecycle {
    fn new(initial: Option<String>) -> Self {
        Self { pending_path: initial, frontend_ready: false, window_prepared: false, window_shown: false }
    }

    fn open_request(&mut self, path: String) -> Option<String> {
        if self.frontend_ready { Some(path) }
        else { self.pending_path = Some(path); None }
    }

    fn ready(&mut self) -> Option<String> {
        self.frontend_ready = true;
        self.pending_path.take()
    }
}

fn decoder_executable(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let resource = app.path().resource_dir().map_err(|e| e.to_string())?;
    let paths = [resource.join("resources/imagemagick/magick.exe"), resource.join("imagemagick/magick.exe"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/imagemagick/magick.exe")];
    paths.into_iter().find(|p| p.is_file()).ok_or("The bundled decoder is missing. Please reinstall the application.".into())
}

#[tauri::command]
async fn pick_path(app: tauri::AppHandle, folder: bool) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let builder = app.dialog().file().set_title(if folder { "Open image folder" } else { "Open image" });
        let selected = if folder { builder.blocking_pick_folder() } else { builder.add_filter("Images", EXTENSIONS).blocking_pick_file() };
        selected.map(|p| p.into_path().map(|p| decoder::path_string(&p)).map_err(|e| e.to_string())).transpose()
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_save_path(app: tauri::AppHandle, suggested: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&suggested);
        let mut dialog = app.dialog().file().set_title("Save image as")
            .add_filter("PNG — transparency, lossless", &["png"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .add_filter("WebP", &["webp"])
            .add_filter("TIFF — high bit depth", &["tif", "tiff"])
            .add_filter("Other image formats", decoder::SAVE_EXTENSIONS);
        if let Some(name) = path.file_name() { dialog = dialog.set_file_name(name.to_string_lossy()); }
        if let Some(parent) = path.parent().filter(|p| p.is_dir()) { dialog = dialog.set_directory(parent); }
        dialog.blocking_save_file().map(|p| p.into_path().map(|p| decoder::path_string(&p)).map_err(|e| e.to_string())).transpose()
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn path_exists(path: String) -> bool { Path::new(&path).exists() }

#[tauri::command]
fn frontend_ready(state: tauri::State<StartupState>) -> Result<Option<String>, String> {
    // The frontend awaits listener registration before calling this command.
    // Queue draining and enabling live delivery share the same lock.
    Ok(state.0.lock().map_err(|e| e.to_string())?.ready())
}

fn cloak_window(window: &tauri::WebviewWindow, cloaked: bool) -> Result<(), String> {
    #[cfg(windows)] {
        use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let value = i32::from(cloaked);
        let result = unsafe {
            DwmSetWindowAttribute(hwnd.0, DWMWA_CLOAK as u32, (&value as *const i32).cast(), std::mem::size_of::<i32>() as u32)
        };
        if result < 0 { return Err(format!("Could not prepare the window presentation (0x{:08X}).", result as u32)); }
    }
    #[cfg(not(windows))] let _ = (window, cloaked);
    Ok(())
}

fn restore_main_window(window: &tauri::WebviewWindow) {
    let _ = window.restore_state(tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION | tauri_plugin_window_state::StateFlags::MAXIMIZED);
}

#[tauri::command]
fn prepare_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("The main window is unavailable.")?;
    let state = app.state::<StartupState>();
    let startup = state.0.lock().map_err(|e| e.to_string())?;
    if startup.window_prepared || startup.window_shown { return Ok(()); }
    drop(startup);
    // DWM still composes a cloaked window, so WebView2 can submit its first
    // frame without exposing the native background. Merely hiding the HWND
    // prevents that first paint on some WebView2 versions.
    cloak_window(&window, true)?;
    // Remember the cloak even if a later native operation fails, so both the
    // ordinary reveal and the timeout know they must clear it.
    state.0.lock().map_err(|e| e.to_string())?.window_prepared = true;
    restore_main_window(&window);
    window.show().map_err(|e| e.to_string())?;
    Ok(())
}

fn reveal_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("The main window is unavailable.")?;
    let prepared = app.state::<StartupState>().0.lock().map_err(|e| e.to_string())?.window_prepared;
    if !prepared { restore_main_window(&window); }
    window.show().map_err(|e| e.to_string())?;
    // A failed/unsupported cloak leaves the ordinary visible-window fallback
    // available. A successful cloak must be cleared before marking it shown.
    if prepared { cloak_window(&window, false)?; }
    app.state::<StartupState>().0.lock().map_err(|e| e.to_string())?.window_shown = true;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    reveal_main_window(&app)
}

#[tauri::command]
async fn open_path(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<decoder::ImageInfo, String> {
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let canceled = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.current_open.lock() { if let Some(old) = slot.replace(canceled.clone()) { old.store(true, Ordering::Relaxed); } }
    let _permit = state.jobs.clone().acquire_owned().await.map_err(|e| e.to_string())?;
    if state.generation.load(Ordering::SeqCst) != generation { return Err("Opening canceled because a newer image was selected.".into()); }
    let mut decoder = state.decoder.clone(); decoder.canceled = Some(canceled);
    let info = tauri::async_runtime::spawn_blocking(move || {
        let selected = resolve_open_path(&path)?;
        decoder.open(&selected)
    }).await.map_err(|e| e.to_string())??;
    if state.generation.load(Ordering::SeqCst) != generation { return Err("Opening canceled because a newer image was selected.".into()); }
    app.asset_protocol_scope().allow_file(&info.path).map_err(|e| e.to_string())?;
    app.asset_protocol_scope().allow_file(&info.display_path).map_err(|e| e.to_string())?;
    Ok(info)
}

#[derive(Serialize)]
struct Entry { path: String, name: String }

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let a = a.to_lowercase(); let b = b.to_lowercase();
    let aa = a.as_bytes(); let bb = b.as_bytes(); let (mut i, mut j) = (0, 0);
    while i < aa.len() && j < bb.len() {
        if aa[i].is_ascii_digit() && bb[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < aa.len() && aa[i].is_ascii_digit() { i += 1; }
            while j < bb.len() && bb[j].is_ascii_digit() { j += 1; }
            let an = a[si..i].trim_start_matches('0'); let bn = b[sj..j].trim_start_matches('0');
            let c = an.len().cmp(&bn.len()).then_with(|| an.cmp(bn)).then_with(|| (i - si).cmp(&(j - sj)));
            if c != Equal { return c; }
        } else {
            let c = aa[i].cmp(&bb[j]); if c != Equal { return c; } i += 1; j += 1;
        }
    }
    aa.len().cmp(&bb.len())
}

fn folder_entries(path: &str) -> Result<Vec<Entry>, String> {
    let source = PathBuf::from(path);
    let folder = if source.is_dir() { source } else { source.parent().ok_or("This file has no containing folder")?.to_path_buf() };
    let mut result = Vec::new();
    for entry in fs::read_dir(folder).map_err(|e| format!("Cannot read this folder: {e}"))?.flatten() {
        let path = entry.path();
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) && EXTENSIONS.contains(&decoder::extension(&path).as_str()) {
            result.push(Entry { path: decoder::path_string(&path), name: entry.file_name().to_string_lossy().into_owned() });
        }
    }
    result.sort_by(|a, b| natural_cmp(&a.name, &b.name)); Ok(result)
}

fn resolve_open_path(path: &str) -> Result<String, String> {
    if Path::new(path).is_dir() {
        folder_entries(path)?.into_iter().next().map(|e| e.path)
            .ok_or_else(|| "This folder contains no recognized image files. Choose another folder.".into())
    } else { Ok(path.to_string()) }
}

#[tauri::command]
async fn list_folder(path: String) -> Result<Vec<Entry>, String> {
    tauri::async_runtime::spawn_blocking(move || folder_entries(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn thumbnail(app: tauri::AppHandle, state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    let _permit = state.thumbnails.clone().acquire_owned().await.map_err(|e| e.to_string())?;
    let decoder = state.decoder.clone();
    let target = tauri::async_runtime::spawn_blocking(move || { let source = decoder::source_path(&path)?; decoder.preview(&source, true) }).await.map_err(|e| e.to_string())??;
    app.asset_protocol_scope().allow_file(&target).map_err(|e| e.to_string())?;
    Ok(decoder::path_string(&target))
}

#[tauri::command]
async fn save_image(state: tauri::State<'_, AppState>, request: decoder::SaveRequest) -> Result<String, String> {
    let _permit = state.jobs.clone().acquire_owned().await.map_err(|e| e.to_string())?;
    let decoder = state.decoder.clone();
    tauri::async_runtime::spawn_blocking(move || decoder.save(&request)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn load_settings(state: tauri::State<AppState>) -> Settings {
    let mut settings: Settings = fs::read(&state.settings_file).ok().and_then(|v| serde_json::from_slice(&v).ok()).unwrap_or_default();
    settings.validate(); settings
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, mut settings: Settings) -> Result<(), String> {
    settings.validate();
    let parent = state.settings_file.parent().unwrap(); fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    fs::write(temp.path(), serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    decoder::atomic_replace(temp.path(), &state.settings_file, true)
}

#[tauri::command]
async fn clear_thumbnail_cache(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _permit = state.thumbnails.clone().acquire_owned().await.map_err(|e| e.to_string())?;
    let cache = state.decoder.cache.clone();
    tauri::async_runtime::spawn_blocking(move || {
        for entry in fs::read_dir(cache).map_err(|e| e.to_string())?.flatten() {
            if entry.file_name().to_string_lossy().starts_with("thumb-") && entry.path().is_file() { let _ = fs::remove_file(entry.path()); }
        }
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_default_viewer() -> Result<(), String> {
    #[cfg(windows)] {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let uri: Vec<u16> = "ms-settings:defaultapps?registeredAppUser=Photo%20Viewer%20Advanced\0".encode_utf16().collect();
        let result = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), uri.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
        if (result as isize) <= 32 { return Err("Windows Default Apps settings could not be opened.".into()); }
    }
    Ok(())
}

fn launch_path(args: impl IntoIterator<Item = String>, cwd: Option<&str>) -> Option<String> {
    args.into_iter().skip(1).find(|arg| !arg.starts_with('-')).map(|arg| {
        let p = PathBuf::from(arg);
        decoder::path_string(&if p.is_absolute() { p } else { Path::new(cwd.unwrap_or(".")).join(p) })
    })
}

pub fn run() {
    tauri::Builder::default()
        .manage(StartupState(Mutex::new(StartupLifecycle::new(launch_path(std::env::args(), None)))))
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let state = app.state::<StartupState>();
            let show_window = if let Ok(mut startup) = state.0.lock() {
                if let Some(path) = launch_path(args, Some(&cwd)).and_then(|path| startup.open_request(path)) {
                    let _ = app.emit("open-file", path);
                }
                startup.window_shown
            } else { false };
            if show_window {
                if let Some(window) = app.get_webview_window("main") { let _ = window.unminimize(); let _ = window.show(); let _ = window.set_focus(); }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().with_state_flags(tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION | tauri_plugin_window_state::StateFlags::MAXIMIZED).skip_initial_state("main").build())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                // Restore dimensions before layout, but defer maximize because
                // that native operation may reveal the window immediately.
                let _ = window.restore_state(tauri_plugin_window_state::StateFlags::SIZE | tauri_plugin_window_state::StateFlags::POSITION);
            }
            let cache = app.path().app_cache_dir()?.join("images-v1");
            let decoder = Decoder::new(decoder_executable(app.handle())?, cache)?;
            let jobs = Arc::new(Semaphore::new(1));
            let thumbnails = Arc::new(Semaphore::new(1));
            // Hold both decoder queues until pruning completes. This moves disk
            // work off the UI thread without deleting a preview while it loads.
            let prune_job = jobs.clone().try_acquire_owned()?;
            let prune_thumbnails = thumbnails.clone().try_acquire_owned()?;
            let cache_decoder = decoder.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _permits = (prune_job, prune_thumbnails);
                cache_decoder.prune();
            });
            let settings_file = app.path().app_config_dir()?.join("settings.json");
            app.manage(AppState { decoder, settings_file, jobs, thumbnails, generation: AtomicU64::new(0), current_open: Mutex::new(None) });
            // A failed frontend must not strand an invisible app. The static
            // HTML shell remains available even if the module cannot start.
            let fallback_app = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                let reveal_app = fallback_app.clone();
                // Serialize with frontend commands so a timeout cannot reveal
                // the window just before a late preparation cloaks it again.
                let _ = fallback_app.run_on_main_thread(move || {
                    let shown = reveal_app.state::<StartupState>().0.lock().map(|s| s.window_shown).unwrap_or(true);
                    if !shown {
                        if let Some(window) = reveal_app.get_webview_window("main") {
                            let _ = window.eval("{ const status = document.getElementById('startup-message'); if (status) status.textContent = 'Photo Viewer Advanced is taking longer than expected to start. Please close and reopen the app.'; }");
                        }
                        let _ = reveal_main_window(&reveal_app);
                    }
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![pick_path,pick_save_path,path_exists,frontend_ready,prepare_main_window,show_main_window,open_path,list_folder,thumbnail,save_image,load_settings,save_settings,clear_thumbnail_cache,set_default_viewer])
        .run(tauri::generate_context!())
        .expect("Could not start Photo Viewer Advanced");
}

pub fn run_cli() -> bool { self_test::run_cli() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn natural_folder_order() {
        let mut names = ["image10.png", "image2.png", "image1.png", "image02.png"];
        names.sort_by(|a,b| natural_cmp(a,b));
        assert_eq!(names, ["image1.png", "image2.png", "image02.png", "image10.png"]);
    }
    #[test] fn unicode_launch_path_preserved() {
        let args = vec!["viewer.exe".into(), r"C:\pictures\Café & 测试 [1].png".into()];
        assert_eq!(launch_path(args, None).unwrap(), r"C:\pictures\Café & 测试 [1].png");
    }
    #[test] fn explorer_launches_before_listener_registration_keep_the_latest_path() {
        let mut startup = StartupLifecycle::new(Some("first.png".into()));
        assert_eq!(startup.open_request("second.jpg".into()), None);
        assert_eq!(startup.open_request("Café & 测试 [1].webp".into()), None);
        assert!(!startup.window_shown, "Early launches must not expose an unpainted window");
        assert_eq!(startup.ready().as_deref(), Some("Café & 测试 [1].webp"));
        assert_eq!(startup.ready(), None, "The startup path must be delivered only once");
    }
    #[test] fn registered_frontend_receives_new_launches_even_before_window_reveal() {
        let mut startup = StartupLifecycle::new(None);
        assert_eq!(startup.ready(), None);
        assert!(!startup.window_shown);
        assert_eq!(startup.open_request("new.png".into()).as_deref(), Some("new.png"));
        assert_eq!(startup.pending_path, None);
    }
}
