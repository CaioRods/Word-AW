//! Local discovery for live AWcontrol docking. No document contents or credentials are published.
#[cfg(windows)]
pub fn register(port: u16) {
    let Some(base) = std::env::var_os("APPDATA") else { return };
    let Ok(exe) = std::env::current_exe() else { return };
    let folder = std::path::PathBuf::from(base).join("AlimentosWilson").join("sessions");
    if std::fs::create_dir_all(&folder).is_err() {
        return;
    }
    let pid = std::process::id();
    let Ok(bytes) = serde_json::to_vec(&serde_json::json!({"version":1,"pid":pid,"app":1,"port":port,"executable":exe})) else { return };
    // A crash may leave an entry; the receiver validates PID, image and authenticated live state.
    let path = folder.join(format!("{pid}.json"));
    let temp = folder.join(format!("{pid}.tmp"));
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(temp, path);
    }
}
#[cfg(not(windows))]
pub fn register(_port: u16) {}
