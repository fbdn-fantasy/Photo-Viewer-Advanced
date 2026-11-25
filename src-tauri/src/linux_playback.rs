use std::{
    fs, io::{BufRead, BufReader, Write}, path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio}, sync::{Arc, Mutex},
};
use tempfile::TempDir;

pub type Playback = Arc<Mutex<Option<Session>>>;

pub struct Session {
    path: PathBuf,
    dir: TempDir,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Session {
    fn new(path: &Path, format: &str) -> Result<Self, String> {
        let dir = tempfile::Builder::new().prefix("pva-animation-").tempdir().map_err(|e| e.to_string())?;
        let mut child = Command::new("/usr/bin/python3")
            .args(["-I", "-c", include_str!("linux_frames.py")]).arg(path).arg(format)
            .current_dir(dir.path()).stdin(Stdio::piped()).stdout(Stdio::piped())
            .spawn().map_err(|e| format!("Could not start the animation decoder: {e}"))?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Self { path: path.to_owned(), dir, child, input, output })
    }

    fn frame(&mut self, index: u32) -> Result<tauri::ipc::Response, String> {
        writeln!(self.input, "{index}").map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.output.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.is_empty() { return Err("The animation decoder stopped or exceeded its time limit.".into()); }
        if let Ok(error) = serde_json::from_str::<String>(&line) { return Err(error); }
        let (count, repeats, duration): (u32, i32, u32) = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let frame = fs::read(self.dir.path().join("frame.png")).map_err(|e| e.to_string())?;
        let mut bytes = Vec::with_capacity(frame.len() + 12);
        bytes.extend(count.to_le_bytes());
        bytes.extend(repeats.to_le_bytes());
        bytes.extend(duration.to_le_bytes());
        bytes.extend(frame);
        Ok(tauri::ipc::Response::new(bytes))
    }
}

pub fn read(state: &Playback, path: PathBuf, format: &str, index: u32) -> Result<tauri::ipc::Response, String> {
    let mut slot = state.lock().map_err(|e| e.to_string())?;
    if !slot.as_ref().is_some_and(|session| session.path == path) {
        *slot = Some(Session::new(&path, format)?);
    }
    let result = slot.as_mut().unwrap().frame(index);
    if result.is_err() { *slot = None; }
    result
}
