use std::{
    ffi::OsString,
    fs::{create_dir_all, remove_file},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr::null_mut,
    slice::from_raw_parts,
};

use windows::{
    core::{Interface, GUID, HSTRING, PCWSTR},
    Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, ShellLink,
            KF_FLAG_DEFAULT,
        },
    },
};

use crate::log::log_line;

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        match SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
            Ok(pwstr) => {
                let mut len = 0;
                while *pwstr.0.add(len) != 0 {
                    len += 1;
                }
                let slice = from_raw_parts(pwstr.0, len);
                let os_string = OsString::from_wide(slice);
                CoTaskMemFree(Some(pwstr.0 as *const _));
                Some(PathBuf::from(os_string))
            }
            Err(e) => {
                log_line(&format!("SHGetKnownFolderPath failed: {e}"));
                None
            }
        }
    }
}

pub fn desktop_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop)
}

pub fn start_menu_programs_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Programs)
}

pub struct ShortcutSpec<'a> {
    pub target: &'a str,
    pub arguments: &'a str,
    pub working_dir: &'a str,
    pub description: &'a str,
    pub icon: Option<&'a str>,
}

/// Create (or replace) `<dir>\<name>.lnk`.
pub fn create_shortcut_in(dir: &PathBuf, name: &str, spec: &ShortcutSpec) -> bool {
    if let Err(e) = create_dir_all(dir) {
        log_line(&format!("Failed to create shortcut dir {}: {e}", dir.display()));
        return false;
    }
    let shortcut_path = dir.join(format!("{name}.lnk"));
    if shortcut_path.exists() {
        if let Err(e) = remove_file(&shortcut_path) {
            log_line(&format!("Failed to remove existing shortcut: {e}"));
            return false;
        }
    }

    unsafe {
        if CoInitializeEx(Some(null_mut()), COINIT_APARTMENTTHREADED).is_err() {
            log_line("Failed to initialize COM.");
            return false;
        }

        let result = (|| {
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            shell_link.SetPath(&HSTRING::from(spec.target)).ok()?;
            shell_link.SetArguments(&HSTRING::from(spec.arguments)).ok()?;
            shell_link
                .SetWorkingDirectory(&HSTRING::from(spec.working_dir))
                .ok()?;
            shell_link
                .SetDescription(&HSTRING::from(spec.description))
                .ok()?;
            if let Some(icon) = spec.icon {
                let _ = shell_link.SetIconLocation(&HSTRING::from(icon), 0);
            }

            let persist_file: IPersistFile = shell_link.cast().ok()?;
            let wide_path: Vec<u16> = shortcut_path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            persist_file.Save(PCWSTR(wide_path.as_ptr()), true).ok()?;
            Some(())
        })();

        CoUninitialize();
        if result.is_none() {
            log_line(&format!("Failed to create shortcut {}", shortcut_path.display()));
        }
        result.is_some()
    }
}

/// Create a desktop shortcut. `name` is the `.lnk` file name without extension.
pub fn create_desktop_shortcut(name: &str, spec: &ShortcutSpec) -> bool {
    match desktop_dir() {
        Some(dir) => create_shortcut_in(&dir, name, spec),
        None => false,
    }
}

pub fn create_start_menu_shortcut(name: &str, spec: &ShortcutSpec) -> bool {
    match start_menu_programs_dir() {
        Some(dir) => create_shortcut_in(&dir.join("B.A.S.H"), name, spec),
        None => false,
    }
}

pub fn remove_desktop_shortcut(name: &str) {
    if let Some(dir) = desktop_dir() {
        let _ = remove_file(dir.join(format!("{name}.lnk")));
    }
}
