//! Who is holding our files open? Uses the Windows Restart Manager, which is
//! the same mechanism Windows Installer uses to find processes that lock files.

use std::{
    ffi::OsString,
    fs,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, ERROR_MORE_DATA, ERROR_SUCCESS},
        System::{
            RestartManager::{
                RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
                RM_PROCESS_INFO,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    },
};

use crate::{log::log_line, paths::SimbaPaths};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locker {
    pub pid: u32,
    pub app_name: String,
    pub exe_name: String,
}

impl Locker {
    /// Name shown to the user.
    pub fn friendly_name(&self) -> String {
        let exe = self.exe_name.to_ascii_lowercase();
        if exe == "runelite.exe" {
            "RuneLite".to_string()
        } else if exe == "java.exe" || exe == "javaw.exe" {
            "RuneLite (Java)".to_string()
        } else if exe.starts_with("simba") {
            "Simba".to_string()
        } else if exe == "jagexlauncher.exe" {
            "Jagex Launcher".to_string()
        } else if !self.app_name.trim().is_empty() {
            self.app_name.trim().to_string()
        } else if !self.exe_name.is_empty() {
            self.exe_name.clone()
        } else {
            format!("process {}", self.pid)
        }
    }

    pub fn is_simba(&self) -> bool {
        self.exe_name.to_ascii_lowercase().starts_with("simba")
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn exe_name_for_pid(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buffer = vec![0u16; 1024];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return String::new();
        }
        let full = OsString::from_wide(&buffer[..len as usize]);
        PathBuf::from(full)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// Files whose lock state matters for an update: plugin DLLs (held by
/// RuneLite through RemoteInput and by Simba) and the Simba executable.
pub fn watched_files(paths: &SimbaPaths, include_simba_exe: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for include in [crate::paths::SRLB_DIR, crate::paths::BASHLIB_DIR] {
        let plugins = paths.include(include).join("plugins");
        collect_files(&plugins, &mut files, 0);
    }
    if include_simba_exe && paths.simba_exe().exists() {
        files.push(paths.simba_exe());
    }
    files
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out, depth + 1);
        } else {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if matches!(ext.as_str(), "dll" | "exe") {
                out.push(path);
            }
        }
    }
}

/// Ask the Restart Manager which processes hold any of `files` open.
pub fn find_lockers(files: &[PathBuf]) -> Result<Vec<Locker>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }

    let wide_paths: Vec<Vec<u16>> = files.iter().map(|p| wide(p)).collect();
    let pcwstrs: Vec<PCWSTR> = wide_paths.iter().map(|w| PCWSTR(w.as_ptr())).collect();

    unsafe {
        let mut session: u32 = 0;
        let mut key = vec![0u16; CCH_RM_SESSION_KEY as usize + 1];
        let err = RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr()));
        if err != ERROR_SUCCESS {
            return Err(format!("RmStartSession failed ({})", err.0));
        }

        let result = (|| {
            let err = RmRegisterResources(session, Some(&pcwstrs), None, None);
            if err != ERROR_SUCCESS {
                return Err(format!("RmRegisterResources failed ({})", err.0));
            }

            let mut needed: u32 = 0;
            let mut count: u32 = 0;
            let mut reasons: u32 = 0;
            let err = RmGetList(session, &mut needed, &mut count, None, &mut reasons);
            if err == ERROR_SUCCESS || needed == 0 {
                return Ok(Vec::new());
            }
            if err != ERROR_MORE_DATA {
                return Err(format!("RmGetList failed ({})", err.0));
            }

            let mut infos: Vec<RM_PROCESS_INFO> = vec![RM_PROCESS_INFO::default(); needed as usize + 4];
            count = infos.len() as u32;
            let err = RmGetList(
                session,
                &mut needed,
                &mut count,
                Some(infos.as_mut_ptr()),
                &mut reasons,
            );
            if err != ERROR_SUCCESS {
                return Err(format!("RmGetList (2) failed ({})", err.0));
            }
            infos.truncate(count as usize);

            let mut lockers: Vec<Locker> = Vec::new();
            for info in infos {
                let pid = info.Process.dwProcessId;
                if pid == std::process::id() {
                    continue;
                }
                let name_len = info
                    .strAppName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.strAppName.len());
                let app_name = String::from_utf16_lossy(&info.strAppName[..name_len]);
                let locker = Locker {
                    pid,
                    app_name,
                    exe_name: exe_name_for_pid(pid),
                };
                if !lockers.iter().any(|l| l.pid == pid) {
                    lockers.push(locker);
                }
            }
            Ok(lockers)
        })();

        let _ = RmEndSession(session);
        result
    }
}

/// Lockers for the standard watched file set.
pub fn find_update_lockers(paths: &SimbaPaths, include_simba_exe: bool) -> Vec<Locker> {
    let files = watched_files(paths, include_simba_exe);
    match find_lockers(&files) {
        Ok(lockers) => {
            if !lockers.is_empty() {
                log_line(&format!("lockers: {:?}", lockers));
            }
            lockers
        }
        Err(e) => {
            log_line(&format!("find_lockers: {e}"));
            Vec::new()
        }
    }
}

fn taskkill(pid: u32, force: bool) {
    let mut cmd = Command::new("taskkill");
    if force {
        cmd.arg("/F");
    }
    cmd.arg("/PID").arg(pid.to_string());
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();
}

fn pid_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut code: u32 = 0;
                let running = windows::Win32::System::Threading::GetExitCodeProcess(handle, &mut code)
                    .map(|_| code == 259) // STILL_ACTIVE
                    .unwrap_or(false);
                let _ = CloseHandle(handle);
                running
            }
            Err(_) => false,
        }
    }
}

/// Close the given processes: ask nicely first (WM_CLOSE via `taskkill`),
/// then force after `grace`. Returns processes that are still alive.
pub fn close_processes(lockers: &[Locker], grace: Duration) -> Vec<Locker> {
    for locker in lockers {
        log_line(&format!("closing {} (pid {})", locker.friendly_name(), locker.pid));
        taskkill(locker.pid, false);
    }

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if lockers.iter().all(|l| !pid_alive(l.pid)) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }

    for locker in lockers {
        if pid_alive(locker.pid) {
            log_line(&format!("force killing {} (pid {})", locker.friendly_name(), locker.pid));
            taskkill(locker.pid, true);
        }
    }

    thread::sleep(Duration::from_millis(500));
    lockers
        .iter()
        .filter(|l| pid_alive(l.pid))
        .cloned()
        .collect()
}

/// Wait until none of `files` are locked (or `timeout` passes).
pub fn wait_until_unlocked(files: &[PathBuf], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match find_lockers(files) {
            Ok(lockers) if lockers.is_empty() => return true,
            Err(_) => return true,
            _ => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(500));
    }
}
