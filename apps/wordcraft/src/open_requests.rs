//! Pass Windows Explorer's file-open requests to the app's existing window.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_REQUEST: u64 = 1024 * 1024;

pub struct OpenRequests {
    folder: PathBuf,
    _lock: File,
}

impl OpenRequests {
    /// Hold the user's lock for the lifetime of the window, or atomically hand off this launch.
    pub fn start(app: &str, paths: &[PathBuf]) -> io::Result<Option<Self>> {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| io::Error::other("LOCALAPPDATA unavailable"))?;
        let folder = PathBuf::from(local).join(app).join("OpenRequests");
        fs::create_dir_all(&folder)?;
        let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(folder.join("instance.lock"))?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(Self { folder, _lock: lock })),
            Err(TryLockError::WouldBlock) => {
                let absolute = paths.iter().map(|path| {
                    std::path::absolute(path).map(|path| path.as_os_str().encode_wide().collect::<Vec<_>>())
                }).collect::<io::Result<Vec<_>>>()?;
                let data = serde_json::to_vec(&absolute)?;
                if data.len() as u64 > MAX_REQUEST {
                    return Err(io::Error::other("file-open request is too large"));
                }
                let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
                let pending = folder.join(format!("{}-{stamp:032}.tmp", std::process::id()));
                fs::write(&pending, data)?;
                let request = pending.with_extension("json");
                fs::rename(&pending, &request)?;
                // If the previous window quit during this write, open the launch arguments
                // directly. Drop our queued copy to avoid opening a template twice.
                match lock.try_lock() {
                    Ok(()) => {
                        let _ = fs::remove_file(request);
                        Ok(Some(Self { folder, _lock: lock }))
                    }
                    Err(TryLockError::WouldBlock) => Ok(None),
                    Err(TryLockError::Error(error)) => Err(error),
                }
            }
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    pub fn poll(&self, ctx: &egui::Context, mut open: impl FnMut(&Path)) {
        ctx.request_repaint_after(Duration::from_millis(350));
        let Ok(entries) = fs::read_dir(&self.folder) else { return };
        let mut requested = false;
        for entry in entries.flatten().filter(|e| e.path().extension().is_some_and(|ext| ext == "json")).take(16) {
            let path = entry.path();
            let Ok(bytes) = read_request(&path) else {
                let _ = fs::remove_file(&path);
                continue;
            };
            if fs::remove_file(&path).is_err() {
                continue;
            }
            let Ok(paths) = serde_json::from_slice::<Vec<Vec<u16>>>(&bytes) else { continue };
            requested = true;
            for path in paths {
                let path = PathBuf::from(std::ffi::OsString::from_wide(&path));
                if path.is_absolute() {
                    open(&path);
                }
            }
        }
        if requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
}

fn read_request(path: &Path) -> io::Result<Vec<u8>> {
    if !path.symlink_metadata()?.is_file() {
        return Err(io::Error::other("file-open request is not a regular file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_REQUEST + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_REQUEST {
        return Err(io::Error::other("file-open request is too large"));
    }
    Ok(bytes)
}
