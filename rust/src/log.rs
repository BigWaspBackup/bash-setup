use std::{
    env,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

static LOG_NAME: OnceLock<String> = OnceLock::new();

/// Set the log file name (without directory). Defaults to `bash-setup.log`.
pub fn set_log_name(name: &str) {
    let _ = LOG_NAME.set(name.to_string());
}

pub fn log_path() -> PathBuf {
    let name = LOG_NAME.get().map(String::as_str).unwrap_or("bash-setup.log");
    env::temp_dir().join(name)
}

pub fn log_line(message: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(file, "{stamp} {message}");
    }
}
