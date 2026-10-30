//! Install / update steps shared by the installer and the bootstrapper. Every
//! replacement is staged next to the target and swapped with renames so a
//! failure half way leaves the previous files in place.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use reqwest::blocking::Client;

use crate::{
    download, log::log_line, paths::{self, SimbaPaths}, send_status, unzip, versions, ProgressSender,
};

fn status(tx: Option<&ProgressSender>, text: impl Into<String>) {
    match tx {
        Some(tx) => send_status(tx, text),
        None => log_line(&text.into()),
    }
}

fn is_in_use(err: &std::io::Error) -> bool {
    matches!(err.kind(), ErrorKind::PermissionDenied) || err.raw_os_error() == Some(32) || err.raw_os_error() == Some(5)
}

/// Remove a directory, retrying briefly (Windows keeps handles open for a
/// moment after a process exits).
pub fn remove_dir_retry(path: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..10 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last = Some(e);
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(last.unwrap())
}

fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..10 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(last.unwrap())
}

/// Delete leftovers from an interrupted swap (`*.old`, `*.new`, `*.part`).
pub fn clean_stale(paths: &SimbaPaths) {
    for include in [paths::SRLB_DIR, paths::BASHLIB_DIR] {
        for suffix in [".old", ".new"] {
            let stale = paths.includes().join(format!("{include}{suffix}"));
            if stale.exists() && !versions::is_dev_checkout(&stale) {
                let _ = remove_dir_retry(&stale);
            }
        }
    }
    for name in ["Simba64.exe.old", "Simba64.exe.new", "BASH.exe.old", "bash-launcher.simba.old"] {
        let _ = fs::remove_file(paths.root.join(name));
    }
    let _ = fs::remove_file(paths.scripts().join("bash-launcher.simba.old"));
    let _ = fs::remove_file(paths.scripts().join("bash-launcher.simba.new"));
}

pub fn ensure_directories(paths: &SimbaPaths) -> Result<(), String> {
    for dir in [
        paths.root.clone(),
        paths.data(),
        paths.data().join("packages"),
        paths.bash_data(),
        paths.downloads(),
        paths.includes(),
        paths.scripts(),
        paths.fonts(),
        paths.configs(),
    ] {
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Write `Data\default.simba` and the `packages.ini` files. Never touches
/// credentials, Configs or overrides.
pub fn write_data_files(paths: &SimbaPaths) -> Result<(), String> {
    let default_path = paths.default_script();
    fs::write(&default_path, paths::default_simba_body())
        .map_err(|e| format!("Failed to write {}: {e}", default_path.display()))?;

    let install_dir = paths.root.to_string_lossy().to_string();
    let ini = paths::packages_ini_body(&install_dir);
    for target in [
        paths.data().join("packages.ini"),
        paths.data().join("packages").join("packages.ini"),
    ] {
        fs::write(&target, &ini).map_err(|e| format!("Failed to write {}: {e}", target.display()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageInstallResult {
    Installed,
    SkippedDevCheckout,
}

/// Download a release zip and atomically swap it into `Includes\<name>`.
/// Preserves `overrides.simba` from the previous install. Returns an error
/// with the word "in use" when files are locked.
pub fn install_package(
    client: &Client,
    paths: &SimbaPaths,
    name: &str,
    zip_url: &str,
    tx: Option<&ProgressSender>,
    progress_range: (f32, f32),
) -> Result<PackageInstallResult, String> {
    let target = paths.include(name);
    if versions::is_dev_checkout(&target) {
        status(tx, format!("{name}: developer checkout detected, not touching it."));
        return Ok(PackageInstallResult::SkippedDevCheckout);
    }

    let staging = paths.includes().join(format!("{name}.new"));
    let old = paths.includes().join(format!("{name}.old"));
    let zip_path = paths.downloads().join(format!("{name}.zip"));

    let _ = remove_dir_retry(&staging);
    let _ = remove_dir_retry(&old);

    let (p0, p1) = progress_range;
    let download_end = p0 + (p1 - p0) * 0.6;
    download::download_to_file(client, zip_url, &zip_path, tx, (p0, download_end))?;

    status(tx, format!("Extracting {name}..."));
    unzip::unzip_file_strip_root(&zip_path, &staging).map_err(|e| {
        let _ = remove_dir_retry(&staging);
        e
    })?;
    let _ = fs::remove_file(&zip_path);

    // Sanity check: a library always has a top-level .simba file.
    let has_simba = fs::read_dir(&staging)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("simba"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !has_simba {
        let _ = remove_dir_retry(&staging);
        return Err(format!("The {name} download did not contain a Simba library."));
    }

    if let Some(tx) = tx {
        crate::send_progress(tx, p0 + (p1 - p0) * 0.85);
    }

    // Carry user overrides across.
    let overrides = target.join("overrides.simba");
    if overrides.exists() {
        if let Err(e) = fs::copy(&overrides, staging.join("overrides.simba")) {
            log_line(&format!("overrides.simba copy failed: {e}"));
        }
    }

    status(tx, format!("Installing {name}..."));
    if target.exists() {
        if let Err(e) = rename_retry(&target, &old) {
            let _ = remove_dir_retry(&staging);
            if is_in_use(&e) {
                return Err(format!(
                    "{name} is in use. Close RuneLite and any other Simba windows, then try again."
                ));
            }
            return Err(format!("Failed to move the old {name} aside: {e}"));
        }
    }

    if let Err(e) = rename_retry(&staging, &target) {
        // Put the old library back.
        if old.exists() {
            let _ = rename_retry(&old, &target);
        }
        let _ = remove_dir_retry(&staging);
        return Err(format!("Failed to install {name}: {e}"));
    }

    if old.exists() {
        if let Err(e) = remove_dir_retry(&old) {
            log_line(&format!("could not delete {}: {e} (will retry next start)", old.display()));
        }
    }

    if let Some(tx) = tx {
        crate::send_progress(tx, p1);
    }
    Ok(PackageInstallResult::Installed)
}

/// Write the launcher script atomically.
pub fn install_launcher_body(paths: &SimbaPaths, body: &str) -> Result<(), String> {
    if body.len() < 500 {
        return Err("Launcher download was empty.".to_string());
    }
    let dest = paths.launcher();
    fs::create_dir_all(paths.scripts())
        .map_err(|e| format!("Failed to create {}: {e}", paths.scripts().display()))?;
    let tmp = paths.scripts().join("bash-launcher.simba.new");
    fs::write(&tmp, body).map_err(|e| format!("Failed to write {}: {e}", tmp.display()))?;
    if dest.exists() {
        let old = paths.scripts().join("bash-launcher.simba.old");
        let _ = fs::remove_file(&old);
        rename_retry(&dest, &old).map_err(|e| {
            format!("Failed to replace the launcher (is it open in Simba?): {e}")
        })?;
        if let Err(e) = rename_retry(&tmp, &dest) {
            let _ = rename_retry(&old, &dest);
            return Err(format!("Failed to install the launcher: {e}"));
        }
        let _ = fs::remove_file(&old);
    } else {
        rename_retry(&tmp, &dest).map_err(|e| format!("Failed to install the launcher: {e}"))?;
    }
    Ok(())
}

pub fn download_and_install_launcher(
    client: &Client,
    paths: &SimbaPaths,
    tx: Option<&ProgressSender>,
) -> Result<u32, String> {
    status(tx, "Downloading the B.A.S.H launcher...");
    let remote = versions::remote_launcher(client)?;
    install_launcher_body(paths, &remote.body)?;
    status(
        tx,
        format!("Launcher revision {} installed from {}.", remote.revision, remote.source),
    );
    Ok(remote.revision)
}

/// Replace an executable using download -> `.new` -> rename swap. Works for a
/// running executable too (Windows allows renaming a running image).
pub fn install_executable(
    client: &Client,
    url: &str,
    dest: &Path,
    tx: Option<&ProgressSender>,
    progress_range: (f32, f32),
) -> Result<(), String> {
    let new = PathBuf::from(format!("{}.new", dest.display()));
    let old = PathBuf::from(format!("{}.old", dest.display()));
    let _ = fs::remove_file(&new);
    let _ = fs::remove_file(&old);

    download::download_to_file(client, url, &new, tx, progress_range)?;

    let size = fs::metadata(&new).map(|m| m.len()).unwrap_or(0);
    if size < 1024 * 1024 {
        let _ = fs::remove_file(&new);
        return Err(format!(
            "Downloaded {} looks too small ({} bytes).",
            dest.file_name().unwrap_or_default().to_string_lossy(),
            size
        ));
    }

    if dest.exists() {
        rename_retry(dest, &old).map_err(|e| {
            let _ = fs::remove_file(&new);
            if is_in_use(&e) {
                format!(
                    "{} is in use. Close Simba and try again.",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                )
            } else {
                format!("Failed to replace {}: {e}", dest.display())
            }
        })?;
    }
    if let Err(e) = rename_retry(&new, dest) {
        if old.exists() {
            let _ = rename_retry(&old, dest);
        }
        return Err(format!("Failed to install {}: {e}", dest.display()));
    }
    let _ = fs::remove_file(&old);
    Ok(())
}

/// Finish a pending self-update: if `BASH.exe.new` exists next to the running
/// exe, swap it in. The swapped-in version is used on the *next* start.
pub fn finish_self_update(paths: &SimbaPaths) -> bool {
    let exe = paths.bootstrapper_exe();
    let new = PathBuf::from(format!("{}.new", exe.display()));
    let old = PathBuf::from(format!("{}.old", exe.display()));
    let _ = fs::remove_file(&old);
    if !new.exists() {
        return false;
    }
    if exe.exists() && rename_retry(&exe, &old).is_err() {
        return false;
    }
    if rename_retry(&new, &exe).is_err() {
        if old.exists() {
            let _ = rename_retry(&old, &exe);
        }
        return false;
    }
    true
}
