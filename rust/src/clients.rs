//! RuneLite windows the launcher can target.
//!
//! The handle is what Simba's `--target` option expects: the top-level window,
//! the same one `run.ps1` passes.

use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        },
    },
};

#[derive(Debug, Clone)]
pub struct ClientWindow {
    pub hwnd: isize,
    pub title: String,
    pub pid: u32,
}

impl ClientWindow {
    pub fn label(&self) -> String {
        format!("{}  (pid {})", self.title, self.pid)
    }

    /// Name shown in the RuneLite title once the account is logged in.
    pub fn account(&self) -> String {
        if let Some(rest) = self.title.strip_prefix("RuneLite - ") {
            let name = rest.split(" - ").next().unwrap_or(rest).trim();
            if name.is_empty() {
                "RuneLite".to_string()
            } else {
                name.to_string()
            }
        } else if self.title == "RuneLite" {
            "Not logged in".to_string()
        } else {
            self.title.clone()
        }
    }
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if lparam.0 != 0 {
        let list = &mut *(lparam.0 as *mut Vec<HWND>);
        list.push(hwnd);
    }
    BOOL(1)
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn is_client_title(title: &str) -> bool {
    if title == "RuneLite Launcher" {
        return false;
    }
    title == "RuneLite" || title.starts_with("RuneLite ")
}

/// Visible game clients. The RuneLite launcher window is not a client.
pub fn game_clients() -> Vec<ClientWindow> {
    let mut hwnds: Vec<HWND> = Vec::new();
    let param = LPARAM(&mut hwnds as *mut Vec<HWND> as isize);
    if unsafe { EnumWindows(Some(collect_window), param) }.is_err() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for hwnd in hwnds {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            continue;
        }
        let title = window_text(hwnd);
        if !is_client_title(&title) {
            continue;
        }
        // Skip tool windows that happen to share the title.
        let class = class_name(hwnd);
        if class.is_empty() {
            continue;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            continue;
        }
        out.push(ClientWindow {
            hwnd: hwnd.0 as isize,
            title,
            pid,
        });
    }
    out.sort_by(|a, b| a.title.cmp(&b.title).then(a.pid.cmp(&b.pid)));
    out
}
