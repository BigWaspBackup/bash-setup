use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

static LOG_NAME: OnceLock<String> = OnceLock::new();
static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Set the log file name (without directory). Defaults to `bash-setup.log`.
/// Ignored once [`set_data_dir`] has chosen a launcher log file.
pub fn set_log_name(name: &str) {
    let _ = LOG_NAME.set(name.to_string());
}

/// Launcher files live under `Data\bash`. Logs go in `logs`, crash reports in `crashes`.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
    let logs = logs_dir();
    let _ = fs::create_dir_all(&logs);
    let _ = LOG_FILE.set(logs.join("launcher.log"));
}

pub fn log_path() -> PathBuf {
    if let Some(path) = LOG_FILE.get() {
        return path.clone();
    }
    let name = LOG_NAME.get().map(String::as_str).unwrap_or("bash-setup.log");
    env::temp_dir().join(name)
}

pub fn log_line(message: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {message}", timestamp());
    }
}

pub fn crash_dir() -> PathBuf {
    let dir = data_dir().join("crashes");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn crash_count() -> usize {
    crash_files().len()
}

/// Clipboard text a user can paste into Discord.
pub fn crash_report() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut files = crash_files();
    files.sort();
    let newest: Vec<_> = files.into_iter().rev().take(3).collect();
    let mut out = format!("BASH Launcher {version} crash report\nSaved in {}\n\n", crash_dir().display());
    if newest.is_empty() {
        out.push_str("No crash log has been saved.\nThis is the recent launcher log:\n\n");
        out.push_str(&read_limited(&log_path(), 20_000));
        return out;
    }
    for path in newest.iter().rev() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        out.push_str(&format!("===== {name} =====\n"));
        out.push_str(&read_limited(path, 20_000));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// Write a panic crash report and clear this process's session file.
pub fn record_panic(detail: &str) {
    log_line(&format!("panic: {detail}"));
    let _ = write_crash("panic", detail);
    mark_clean_exit();
}

/// If a previous BASH process disappeared without exiting cleanly, save a crash
/// report. A process that is still running is left alone.
pub fn note_previous_sessions() {
    let Ok(entries) = fs::read_dir(logs_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid) = name
            .strip_prefix("session-")
            .and_then(|rest| rest.strip_suffix(".txt"))
            .and_then(|text| text.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == process::id() || process_running(pid) {
            continue;
        }
        let _ = write_crash(
            "closed",
            &format!(
                "BASH closed without a clean exit (process {pid}).\n\
                 There is no panic message for this close. The log below is what was recorded before it closed."
            ),
        );
        let _ = fs::remove_file(entry.path());
    }
}

pub fn begin_session() {
    let path = session_path(process::id());
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, "running");
    rotate_log(&log_path());
}

pub fn mark_clean_exit() {
    let _ = fs::remove_file(session_path(process::id()));
}

fn write_crash(kind: &str, detail: &str) -> Option<PathBuf> {
    let dir = crash_dir();
    let path = dir.join(format!("crash-{}.txt", file_stamp()));
    let body = format!(
        "BASH Launcher {}\nWhen: {}\nKind: {kind}\n\n{detail}\n\n--- launcher log ---\n{}",
        env!("CARGO_PKG_VERSION"),
        timestamp(),
        log_tail(200),
    );
    fs::write(&path, body).ok()?;
    trim_crashes();
    Some(path)
}

fn data_dir() -> PathBuf {
    DATA_DIR.get().cloned().unwrap_or_else(|| env::temp_dir().join("bash-launcher"))
}

fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

fn session_path(pid: u32) -> PathBuf {
    logs_dir().join(format!("session-{pid}.txt"))
}

fn crash_files() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(crash_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect()
}

fn trim_crashes() {
    let mut files = crash_files();
    files.sort();
    while files.len() > 20 {
        if let Some(path) = files.first() {
            let _ = fs::remove_file(path);
        }
        files.remove(0);
    }
}

fn rotate_log(path: &Path) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() < 1_000_000 {
        return;
    }
    let previous = path.with_extension("log.1");
    let _ = fs::remove_file(&previous);
    let _ = fs::rename(path, previous);
}

fn log_tail(lines: usize) -> String {
    let text = fs::read_to_string(log_path()).unwrap_or_default();
    let kept: Vec<&str> = text.lines().rev().take(lines).collect();
    kept.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn read_limited(path: &Path, max_bytes: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return "(could not read this file)".to_string();
    };
    if text.len() <= max_bytes {
        return text;
    }
    let head = 2_000.min(max_bytes / 4);
    let tail = max_bytes.saturating_sub(head);
    let head_end = text.char_indices().take_while(|(i, _)| *i < head).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
    let tail_from = text.len().saturating_sub(tail);
    let tail_start = text.char_indices().find(|(i, _)| *i >= tail_from).map(|(i, _)| i).unwrap_or(text.len());
    format!("{}(middle omitted)\n{}", &text[..head_end], &text[tail_start..])
}

fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0);
    format_timestamp(secs)
}

fn file_stamp() -> String {
    timestamp().replace([' ', ':'], "-")
}

fn format_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let time = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Days since 1970-01-01 to a civil year, month, day. Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}

fn process_running(pid: u32) -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let _ = CloseHandle(handle);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn unix_epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn known_day_is_2020_01_01() {
        assert_eq!(civil_from_days(18_262), (2020, 1, 1));
    }
}
