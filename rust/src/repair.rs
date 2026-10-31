//! Full Simba + RuneLite repair, run from Settings.
//!
//! The old force-update bat file closed this process and `rmdir /s` followed
//! directory junctions, which can delete a developer checkout. This does the
//! same reset from a copy of BASH that lives outside the Simba folder, and a
//! junction is only unlinked.

use std::{
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use egui::{Color32, ProgressBar, RichText, ScrollArea};
use windows::{
    core::HSTRING,
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

use crate::{
    download, log::log_line, paths::{self, SimbaPaths}, profile, shortcut, versions,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SETUP_ASSET: &str = "bash-setup.exe";
const RUNELITE_SETUP_URL: &str = "https://github.com/runelite/launcher/releases/latest/download/RuneLiteSetup.exe";
const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

pub const STEPS: &[&str] = &[
    "Closes Simba, RuneLite, and the Jagex Launcher.",
    "Backs up the Simba folder and RuneLite settings to %LOCALAPPDATA%\\BashBackups. The RuneLite game cache is left out.",
    "Deletes the Simba folder, then installs Simba, SRL-B, BashLib, and BASH again.",
    "Puts back the license key, Configs, launcher options, and BashLib overrides.",
    "Installed scripts are not put back. Install them again from the list.",
    "Uninstalls RuneLite, installs it again, and selects the BASH profile.",
    "Sets Windows display scaling to 100% when it is not already, and restarts Explorer.",
    "Adds Windows Defender exclusions for the Simba install folders.",
    "Installs the Visual C++ 2015-2022 runtime if it is missing.",
    "Sets .simba files to open with Simba64.exe.",
];

enum RepairMsg {
    Update(String, f32),
    Done(String),
    Failed(String),
}

#[allow(dead_code)]
enum Preserved {
    Junction { name: &'static str, target: PathBuf },
    Directory { name: &'static str, held: PathBuf },
}

struct RepairPaths {
    simba: SimbaPaths,
    force_update: PathBuf,
    backups: PathBuf,
    #[allow(dead_code)]
    hold: PathBuf,
    restore: PathBuf,
    runelite_home: PathBuf,
    runelite_app: PathBuf,
}

pub fn requested() -> bool {
    env::args().any(|arg| arg.eq_ignore_ascii_case("--repair"))
}

/// Copy the running exe out of the Simba folder and start it elevated.
/// The caller should exit so the Simba folder can be deleted.
pub fn launch_elevated() -> Result<(), String> {
    if image_running("BASH-repair.exe") {
        return Err("A repair is already running.".into());
    }
    let current = env::current_exe().map_err(|e| format!("Could not find BASH.exe: {e}"))?;
    let dir = local_dir("BashForceUpdate")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let dest = dir.join("BASH-repair.exe");
    fs::copy(&current, &dest).map_err(|e| format!("Could not prepare the repair program: {e}"))?;

    let mut params = String::from("--repair");
    if let Some(parent) = current.parent() {
        let setup = parent.join("bash-setup.exe");
        if setup.is_file() {
            params.push_str(" --setup \"");
            params.push_str(&setup.display().to_string());
            params.push('"');
        }
    }

    let launched = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("runas"),
            &HSTRING::from(dest.display().to_string()),
            &HSTRING::from(&params),
            None,
            SW_SHOWNORMAL,
        )
    };
    if launched.0 as isize > 32 {
        Ok(())
    } else {
        Err("Windows did not approve the repair. Nothing was changed.".into())
    }
}

pub fn run_window() {
    let setup = setup_arg();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let sender = tx;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute(setup, |status, progress| {
                let _ = sender.send(RepairMsg::Update(status.to_string(), progress));
            })
        }));
        let message = match result {
            Ok(Ok(summary)) => RepairMsg::Done(summary),
            Ok(Err(error)) => RepairMsg::Failed(error),
            Err(_) => RepairMsg::Failed("The repair stopped unexpectedly. See the repair log in %LOCALAPPDATA%\\BashBackups.".into()),
        };
        let _ = sender.send(message);
    });

    let app = RepairApp {
        rx,
        lines: Vec::new(),
        progress: 0.0,
        finished: None,
    };
    if let Err(error) = eframe::run_native(
        "B.A.S.H Repair",
        crate::ui::native_options(640.0, 480.0),
        Box::new(|_cc| Ok(Box::new(app))),
    ) {
        log_line(&format!("repair window failed: {error}"));
    }
}

struct RepairApp {
    rx: mpsc::Receiver<RepairMsg>,
    lines: Vec<String>,
    progress: f32,
    finished: Option<Result<String, String>>,
}

impl eframe::App for RepairApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                RepairMsg::Update(text, progress) => {
                    self.lines.push(text);
                    self.progress = progress;
                }
                RepairMsg::Done(text) => self.finished = Some(Ok(text)),
                RepairMsg::Failed(text) => self.finished = Some(Err(text)),
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Repair installation");
            ui.add_space(8.0);
            ui.add(ProgressBar::new(self.progress).show_percentage());
            ui.add_space(8.0);
            ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for line in &self.lines {
                    ui.label(line);
                }
                if let Some(result) = &self.finished {
                    ui.add_space(10.0);
                    match result {
                        Ok(text) => {
                            ui.label(RichText::new(text).color(Color32::from_rgb(120, 190, 120)));
                        }
                        Err(text) => {
                            ui.label(RichText::new(text).color(Color32::from_rgb(220, 90, 80)));
                        }
                    }
                }
            });
            if self.finished.is_some() {
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
        if self.finished.is_none() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

fn execute(setup: Option<PathBuf>, mut report: impl FnMut(&str, f32)) -> Result<String, String> {
    if !is_admin() {
        return Err("The repair needs administrator approval. Nothing was changed.".into());
    }

    let paths = repair_paths()?;
    fs::create_dir_all(&paths.force_update).map_err(|e| format!("Could not create the repair folder: {e}"))?;
    fs::create_dir_all(&paths.backups).map_err(|e| format!("Could not create the backup folder: {e}"))?;
    fs::create_dir_all(&paths.simba.root.parent().unwrap_or(&paths.simba.root))
        .map_err(|e| format!("Could not create the Simba parent folder: {e}"))?;

    let stamp = local_stamp();
    let log_path = paths.backups.join(format!("BashUpdate_{stamp}.log"));
    let mut log = fs::File::create(&log_path).map_err(|e| format!("Could not write {}: {e}", log_path.display()))?;
    let _ = writeln!(log, "B.A.S.H repair {stamp}");

    let mut note = |text: &str, progress: f32| {
        log_line(text);
        let _ = writeln!(log, "{text}");
        let _ = log.flush();
        report(text, progress);
    };

    note("Starting the repair.", 0.02);
    rotate_named(&paths.backups, "BashUpdate_", ".log", 10);
    rotate_named(&paths.backups, "Simba_RuneLite_Backup_", ".zip", 10);

    if vc_installed() {
        note("Visual C++ 2015-2022 is already installed.", 0.05);
    } else {
        note("Installing Visual C++ 2015-2022.", 0.05);
        install_vc(&paths.force_update)?;
    }

    note("Removing the old Simba App Paths registry key.", 0.08);
    let _ = hidden("reg")
        .args(["delete", r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Simba", "/f"])
        .status();

    note("Closing Simba, RuneLite, and the Jagex Launcher.", 0.1);
    kill_install_processes();

    if scaling_is_100() {
        note("Display scaling is already 100%.", 0.14);
    } else {
        note("Setting display scaling to 100% and restarting Explorer.", 0.14);
        set_scaling_100()?;
    }

    note("Adding Windows Defender exclusions.", 0.16);
    add_defender_exclusions(&paths);

    note("Flushing the DNS cache.", 0.18);
    let _ = hidden("ipconfig").arg("/flushdns").status();

    let preserved = preserve_dev_libraries(&paths)?;
    let preserved_record = paths.force_update.join("preserved-links.txt");
    write_preserved_record(&preserved_record, &preserved)?;

    let session = paths.backups.join(format!("Backup_{stamp}"));
    let zip_path = paths.backups.join(format!("Simba_RuneLite_Backup_{stamp}.zip"));
    note("Backing up Simba and RuneLite settings.", 0.24);
    if let Err(error) = backup_install(&paths, &session) {
        put_back_moved(&paths, &preserved)?;
        let _ = fs::remove_file(&preserved_record);
        return Err(format!("{error} Simba was not removed."));
    }
    note("Compressing the backup.", 0.36);
    if let Err(error) = zip_dir(&session, &zip_path) {
        put_back_moved(&paths, &preserved)?;
        let _ = fs::remove_file(&preserved_record);
        return Err(format!("{error} Simba was not removed."));
    }

    note("Saving the license key, Configs, and overrides.", 0.4);
    if let Err(error) = stage_user_files(&paths, &preserved) {
        put_back_moved(&paths, &preserved)?;
        let _ = fs::remove_file(&preserved_record);
        return Err(format!("{error} Simba was not removed."));
    }
    let install_id = saved_install_id(&paths.simba.launcher_config());

    note("Removing the Simba folder.", 0.44);
    if paths.simba.root.exists() {
        if let Err(error) = remove_tree_retry(&paths.simba.root) {
            let _ = restore_preserved(&paths, &preserved);
            return Err(format!(
                "{error} The backup is at {}. Local library folders were put back when possible.",
                zip_path.display()
            ));
        }
    }

    if let Err(error) = restore_preserved(&paths, &preserved) {
        return Err(format!(
            "{error} The backup is at {}. Simba was not reinstalled.",
            zip_path.display()
        ));
    }

    note("Uninstalling RuneLite.", 0.5);
    uninstall_runelite(&paths);

    note("Reinstalling Simba, SRL-B, BashLib, and BASH. This can take a few minutes.", 0.55);
    let setup_exe = match setup_executable(setup, &paths.force_update) {
        Ok(path) => path,
        Err(error) => {
            return Err(format!("{error} The backup is at {}.", zip_path.display()));
        }
    };
    let installed = hidden(&setup_exe).arg("--silent").status();
    match installed {
        Ok(status) if status.success() => {}
        Ok(status) => {
            return Err(format!(
                "The Simba installer stopped ({status}). The backup is at {}.",
                zip_path.display()
            ));
        }
        Err(error) => {
            return Err(format!(
                "Could not start the Simba installer: {error}. The backup is at {}.",
                zip_path.display()
            ));
        }
    }
    note("Simba reinstalled.", 0.78);

    let mut warnings = Vec::new();
    note("Installing RuneLite.", 0.82);
    if let Err(error) = install_runelite(&paths.force_update) {
        warnings.push(error);
    }
    if install_id.is_empty() {
        warnings.push("No install id was saved, so the BASH RuneLite profile was not written.".into());
    } else {
        note("Selecting the BASH RuneLite profile.", 0.88);
        if let Err(error) = profile::install(&install_id) {
            warnings.push(error);
        } else if let Err(error) = profile::set_active(&install_id) {
            warnings.push(error);
        }
    }

    note("Putting back the license key, Configs, and overrides.", 0.92);
    if let Err(error) = restore_user_files(&paths) {
        warnings.push(error);
    }

    note("Setting the .simba file association.", 0.95);
    if let Err(error) = associate_simba(&paths.simba.simba_exe()) {
        warnings.push(error);
    }
    let _ = fs::remove_file(paths.simba.root.join("Simba32.exe"));
    shortcut::remove_desktop_shortcut("Simba64");

    let _ = remove_tree(&session);
    let _ = remove_tree(&paths.restore);
    let _ = fs::remove_file(&preserved_record);

    if paths.simba.bootstrapper_exe().is_file() {
        note("Opening BASH.", 0.98);
        let _ = Command::new(paths.simba.bootstrapper_exe())
            .current_dir(&paths.simba.root)
            .spawn();
    }

    let mut summary = format!("Repair finished. Backup: {}", zip_path.display());
    if !warnings.is_empty() {
        summary.push_str("\n");
        summary.push_str(&warnings.join("\n"));
    }
    note("Repair finished.", 1.0);
    Ok(summary)
}

fn repair_paths() -> Result<RepairPaths, String> {
    let home = env::var("USERPROFILE").map_err(|_| "Could not find the user profile folder.".to_string())?;
    let runelite_app = local_dir("RuneLite")?;
    Ok(RepairPaths {
        simba: SimbaPaths::default_install(),
        force_update: local_dir("BashForceUpdate")?,
        backups: local_dir("BashBackups")?,
        hold: local_dir("BashForceUpdate")?.join("preserved"),
        restore: local_dir("BashForceUpdate")?.join("restore"),
        runelite_home: PathBuf::from(home).join(".runelite"),
        runelite_app,
    })
}

fn local_dir(name: &str) -> Result<PathBuf, String> {
    let local = env::var("LOCALAPPDATA").map_err(|_| "Could not find LOCALAPPDATA.".to_string())?;
    Ok(PathBuf::from(local).join(name))
}

fn setup_arg() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg.eq_ignore_ascii_case("--setup") {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

fn is_admin() -> bool {
    hidden("net").arg("session").status().map(|status| status.success()).unwrap_or(false)
}

fn image_running(image: &str) -> bool {
    hidden("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains(&image.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn kill_install_processes() {
    for image in ["Simba32.exe", "Simba64.exe", "RuneLite.exe", "JagexLauncher.exe", "bash-setup.exe", "BASH.exe"] {
        let _ = hidden("taskkill").args(["/F", "/IM", image]).status();
    }
}

fn vc_installed() -> bool {
    let output = hidden("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64", "/v", "Installed"])
        .output();
    match output {
        Ok(output) => reg_dwords(&String::from_utf8_lossy(&output.stdout)).first().copied() == Some(1),
        Err(_) => false,
    }
}

fn install_vc(dir: &Path) -> Result<(), String> {
    let dest = dir.join("vc_redist.x64.exe");
    let client = download::client(versions::USER_AGENT)?;
    download::download_to_file(&client, VC_REDIST_URL, &dest, None, (0.0, 1.0))?;
    let status = Command::new(&dest)
        .args(["/quiet", "/norestart"])
        .status()
        .map_err(|e| format!("Could not start the Visual C++ installer: {e}"))?;
    let _ = fs::remove_file(&dest);
    if status.success() {
        Ok(())
    } else {
        Err(format!("The Visual C++ installer stopped ({status})."))
    }
}

fn scaling_is_100() -> bool {
    if let Ok(output) = hidden("reg")
        .args(["query", r"HKCU\Control Panel\Desktop\PerMonitorSettings", "/s", "/v", "DpiValue"])
        .output()
    {
        let values = reg_dwords(&String::from_utf8_lossy(&output.stdout));
        if !values.is_empty() {
            return values.iter().all(|value| *value == 0);
        }
    }
    if let Ok(output) = hidden("reg")
        .args(["query", r"HKCU\Control Panel\Desktop", "/v", "LogPixels"])
        .output()
    {
        if let Some(value) = reg_dwords(&String::from_utf8_lossy(&output.stdout)).first() {
            return *value == 96;
        }
    }
    true
}

fn set_scaling_100() -> Result<(), String> {
    for (key, name) in [
        (r"HKCU\Control Panel\Desktop", "LogPixels"),
        (r"HKCU\Control Panel\Desktop", "Win8DpiScaling"),
        (r"HKCU\Control Panel\Desktop\WindowMetrics", "AppliedDPI"),
    ] {
        let value = if name == "Win8DpiScaling" { "1" } else { "96" };
        let status = hidden("reg")
            .args(["add", key, "/v", name, "/t", "REG_DWORD", "/d", value, "/f"])
            .status()
            .map_err(|e| format!("Could not update display scaling: {e}"))?;
        if !status.success() {
            return Err("Could not update display scaling.".into());
        }
    }
    let script = paths_script();
    let script_path = local_dir("BashForceUpdate")?.join("dpi.ps1");
    fs::write(&script_path, script).map_err(|e| format!("Could not write the display script: {e}"))?;
    let _ = hidden("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .status();
    let _ = hidden("taskkill").args(["/f", "/im", "explorer.exe"]).status();
    thread::sleep(Duration::from_secs(2));
    let _ = Command::new("explorer.exe").spawn();
    thread::sleep(Duration::from_secs(2));
    Ok(())
}

fn paths_script() -> &'static str {
    r#"Add-Type -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, IntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out IntPtr lpdwResult);
[DllImport("user32.dll")]
public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, IntPtr pvParam, uint fWinIni);
'@ -Name BashDpi -Namespace BashRepair -PassThru | Out-Null
$notify = [BashRepair.BashDpi]
[void]$notify::SystemParametersInfo(0x009F, 0, [IntPtr]::Zero, 0x03)
$result = [IntPtr]::Zero
[void]$notify::SendMessageTimeout([IntPtr]0xFFFF, 0x001A, [IntPtr]::Zero, 'WindowMetrics', 0x0002, 5000, [ref]$result)
[void]$notify::SendMessageTimeout([IntPtr]0xFFFF, 0x001A, [IntPtr]::Zero, 'ImmersiveColorSet', 0x0002, 5000, [ref]$result)
[void]$notify::SendMessageTimeout([IntPtr]0xFFFF, 0x007E, [IntPtr]::Zero, $null, 0x0002, 5000, [ref]$result)
"#
}

fn add_defender_exclusions(paths: &RepairPaths) {
    for dir in [&paths.simba.root, &paths.force_update, &paths.backups] {
        let _ = hidden("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Add-MpPreference -ExclusionPath '{}'", dir.display()),
            ])
            .status();
    }
}

fn preserve_dev_libraries(_paths: &RepairPaths) -> Result<Vec<Preserved>, String> {
    Ok(Vec::new())
}

fn put_back_moved(paths: &RepairPaths, saved: &[Preserved]) -> Result<(), String> {
    for item in saved {
        let Preserved::Directory { name, held } = item else {
            continue;
        };
        let dest = paths.simba.include(name);
        if dest.exists() {
            return Err(format!(
                "Could not move {name} back because {} already exists. The copy is still at {}.",
                dest.display(),
                held.display()
            ));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
        }
        fs::rename(held, &dest).map_err(|e| {
            format!("Could not move {name} back from {}. That copy was not deleted: {e}", held.display())
        })?;
    }
    Ok(())
}

fn restore_preserved(paths: &RepairPaths, saved: &[Preserved]) -> Result<(), String> {
    if saved.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(paths.simba.includes()).map_err(|e| format!("Could not create Includes: {e}"))?;
    for item in saved {
        match item {
            Preserved::Junction { name, target } => {
                let link = paths.simba.include(name);
                if link.symlink_metadata().is_ok() {
                    remove_tree(&link)?;
                }
                let status = hidden("cmd")
                    .args(["/c", "mklink", "/J", &link.display().to_string(), &target.display().to_string()])
                    .status()
                    .map_err(|e| format!("Could not link {name}: {e}"))?;
                if !status.success() {
                    return Err(format!(
                        "Could not link {name} back to {}. That folder was not deleted.",
                        target.display()
                    ));
                }
            }
            Preserved::Directory { name, held } => {
                let dest = paths.simba.include(name);
                if dest.exists() {
                    remove_tree(&dest)?;
                }
                fs::rename(held, &dest).map_err(|e| {
                    format!(
                        "Could not move {name} back from {}. That copy was not deleted: {e}",
                        held.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn backup_install(paths: &RepairPaths, session: &Path) -> Result<(), String> {
    if session.exists() {
        remove_tree(session)?;
    }
    fs::create_dir_all(session).map_err(|e| format!("Could not create the backup folder: {e}"))?;
    if paths.simba.root.exists() {
        copy_tree(&paths.simba.root, &session.join("Simba"), false)?;
    }
    if paths.runelite_home.exists() {
        copy_tree(&paths.runelite_home, &session.join(".runelite"), true)?;
    }
    Ok(())
}

fn stage_user_files(paths: &RepairPaths, preserved: &[Preserved]) -> Result<(), String> {
    if paths.restore.exists() {
        remove_tree(&paths.restore)?;
    }
    fs::create_dir_all(&paths.restore).map_err(|e| format!("Could not create the restore folder: {e}"))?;
    copy_if_file(&paths.simba.root.join("credentials.simba"), &paths.restore.join("credentials.simba"))?;
    let configs = paths.simba.configs();
    if configs.exists() {
        copy_tree(&configs, &paths.restore.join("Configs"), false)?;
    }
    copy_if_file(&paths.simba.state_file(), &paths.restore.join("state.json"))?;
    if let Some(overrides) = overrides_source(paths, preserved) {
        copy_if_file(&overrides, &paths.restore.join("overrides.simba"))?;
    }
    Ok(())
}

fn overrides_source(paths: &RepairPaths, preserved: &[Preserved]) -> Option<PathBuf> {
    for item in preserved {
        if let Preserved::Directory { name, held } = item {
            if *name == paths::BASHLIB_DIR {
                let path = held.join("overrides.simba");
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    for relative in [
        PathBuf::from("Includes").join("BashLib").join("overrides.simba"),
        PathBuf::from("Includes").join("WaspLib").join("overrides.simba"),
    ] {
        let path = paths.simba.root.join(relative);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn restore_user_files(paths: &RepairPaths) -> Result<(), String> {
    copy_if_file(&paths.restore.join("credentials.simba"), &paths.simba.root.join("credentials.simba"))?;
    let configs = paths.restore.join("Configs");
    if configs.exists() {
        copy_tree(&configs, &paths.simba.configs(), false)?;
    }
    copy_if_file(&paths.restore.join("state.json"), &paths.simba.state_file())?;
    let bashlib = paths.simba.include(paths::BASHLIB_DIR);
    let overrides = paths.restore.join("overrides.simba");
    if overrides.is_file() {
        fs::create_dir_all(&bashlib).map_err(|e| format!("Could not create BashLib: {e}"))?;
        copy_if_file(&overrides, &bashlib.join("overrides.simba"))?;
    }
    Ok(())
}

fn saved_install_id(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return String::new();
    };
    value
        .get("install_id")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn uninstall_runelite(paths: &RepairPaths) {
    let uninstaller = paths.runelite_app.join("unins000.exe");
    if uninstaller.is_file() {
        let _ = Command::new(uninstaller).arg("/SILENT").status();
    }
}

fn install_runelite(dir: &Path) -> Result<(), String> {
    let dest = dir.join("RuneLiteSetup.exe");
    let client = download::client(versions::USER_AGENT)?;
    download::download_to_file(&client, RUNELITE_SETUP_URL, &dest, None, (0.0, 1.0))?;
    let status = Command::new(&dest)
        .arg("/SILENT")
        .status()
        .map_err(|e| format!("Could not start the RuneLite installer: {e}"))?;
    let _ = fs::remove_file(&dest);
    let installed = local_dir("RuneLite")?.join("RuneLite.exe").is_file();
    if installed {
        Ok(())
    } else {
        Err(format!("RuneLite did not finish installing ({status})."))
    }
}

fn setup_executable(preferred: Option<PathBuf>, dir: &Path) -> Result<PathBuf, String> {
    if let Some(path) = preferred {
        if path.is_file() {
            return Ok(path);
        }
    }
    let dest = dir.join(SETUP_ASSET);
    let client = download::client(versions::USER_AGENT)?;
    let no_redirect = download::client_no_redirect(versions::USER_AGENT)?;
    let release = versions::latest_release(&no_redirect, versions::SETUP_REPO.0, versions::SETUP_REPO.1)?
        .ok_or_else(|| "No bash-setup release was found.".to_string())?;
    download::download_to_file(&client, &release.asset_url(SETUP_ASSET), &dest, None, (0.0, 1.0))?;
    Ok(dest)
}

fn associate_simba(simba_exe: &Path) -> Result<(), String> {
    if !simba_exe.is_file() {
        return Err("Simba64.exe is not installed, so .simba files were not associated.".into());
    }
    let ftype = format!("ftype simba.script=\"{}\" \"%1\"", simba_exe.display());
    let ftype_status = hidden("cmd")
        .args(["/c", &ftype])
        .status()
        .map_err(|e| format!("Could not set the Simba file type: {e}"))?;
    let assoc_status = hidden("cmd")
        .args(["/c", "assoc", ".simba=simba.script"])
        .status()
        .map_err(|e| format!("Could not associate .simba files: {e}"))?;
    if ftype_status.success() && assoc_status.success() {
        Ok(())
    } else {
        Err("Could not set .simba files to open with Simba64.exe.".into())
    }
}

fn copy_if_file(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.is_file() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    fs::copy(src, dest).map(|_| ()).map_err(|e| format!("Could not copy {}: {e}", src.display()))
}

fn copy_tree(src: &Path, dest: &Path, skip_heavy: bool) -> Result<(), String> {
    let meta = fs::symlink_metadata(src).map_err(|e| format!("Could not read {}: {e}", src.display()))?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        if skip_heavy && is_heavy_dir(src) {
            return Ok(());
        }
        fs::create_dir_all(dest).map_err(|e| format!("Could not create {}: {e}", dest.display()))?;
        for entry in fs::read_dir(src).map_err(|e| format!("Could not read {}: {e}", src.display()))? {
            let entry = entry.map_err(|e| format!("Could not read {}: {e}", src.display()))?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()), skip_heavy)?;
        }
        return Ok(());
    }
    copy_if_file(src, dest)
}

fn is_heavy_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("jagexcache" | "cache" | "cache2" | "repository2")
    )
}

fn remove_tree_retry(path: &Path) -> Result<(), String> {
    let started = Instant::now();
    let mut last = None;
    while started.elapsed() < Duration::from_secs(20) {
        match remove_tree(path) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last = Some(error);
                thread::sleep(Duration::from_millis(400));
            }
        }
    }
    Err(last.unwrap_or_else(|| format!("Could not delete {}.", path.display())))
}

fn remove_tree(path: &Path) -> Result<(), String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not read {}: {error}", path.display())),
    };
    if meta.file_type().is_symlink() {
        return remove_link(path);
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| format!("Could not read {}: {e}", path.display()))? {
            let entry = entry.map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            remove_tree(&entry.path())?;
        }
        return remove_dir_retry(path);
    }
    remove_file_retry(path)
}

fn remove_link(path: &Path) -> Result<(), String> {
    if fs::remove_dir(path).is_ok() || fs::remove_file(path).is_ok() {
        return Ok(());
    }
    clear_readonly(path);
    if fs::remove_dir(path).is_ok() || fs::remove_file(path).is_ok() {
        Ok(())
    } else {
        Err(format!("Could not unlink {}.", path.display()))
    }
}

fn remove_dir_retry(path: &Path) -> Result<(), String> {
    let mut last = None;
    for _ in 0..8 {
        match fs::remove_dir(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                clear_readonly(path);
                last = Some(error);
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(format!(
        "Could not delete {}: {}",
        path.display(),
        last.map(|error| error.to_string()).unwrap_or_else(|| "unknown error".into())
    ))
}

fn remove_file_retry(path: &Path) -> Result<(), String> {
    let mut last = None;
    for _ in 0..8 {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                clear_readonly(path);
                last = Some(error);
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(format!(
        "Could not delete {}: {}",
        path.display(),
        last.map(|error| error.to_string()).unwrap_or_else(|| "unknown error".into())
    ))
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            let _ = fs::set_permissions(path, permissions);
        }
    }
}

fn zip_dir(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    let file = fs::File::create(dest).map_err(|e| format!("Could not create {}: {e}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    add_zip_dir(&mut zip, src, src, options)?;
    zip.finish().map_err(|e| format!("Could not finish the backup: {e}"))?;
    Ok(())
}

fn add_zip_dir(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    options: zip::write::FileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("Could not read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("Could not read {}: {e}", dir.display()))?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = zip_name(root, &path);
        if meta.is_dir() {
            add_zip_dir(zip, root, &path, options)?;
        } else {
            zip.start_file(name, options)
                .map_err(|e| format!("Could not add {} to the backup: {e}", path.display()))?;
            let mut file = fs::File::open(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            io::copy(&mut file, zip).map_err(|e| format!("Could not write {} into the backup: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn zip_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn rotate_named(dir: &Path, prefix: &str, suffix: &str, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(suffix) {
                let modified = entry.metadata().and_then(|meta| meta.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                Some((modified, entry.path()))
            } else {
                None
            }
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in files.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

fn write_preserved_record(path: &Path, saved: &[Preserved]) -> Result<(), String> {
    let mut lines = Vec::new();
    for item in saved {
        match item {
            Preserved::Junction { name, target } => {
                lines.push(format!("junction\t{name}\t{}", target.display()));
            }
            Preserved::Directory { name, held } => {
                lines.push(format!("directory\t{name}\t{}", held.display()));
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    fs::write(path, lines.join("\n")).map_err(|e| format!("Could not record local library links: {e}"))
}

#[allow(dead_code)]
fn normalize_link(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let trimmed = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\??\"))
        .unwrap_or(text.as_ref());
    PathBuf::from(trimmed)
}

fn reg_dwords(text: &str) -> Vec<u32> {
    let mut values = Vec::new();
    for line in text.lines() {
        let Some(index) = line.find("REG_DWORD") else {
            continue;
        };
        let token = line[index + "REG_DWORD".len()..].split_whitespace().next().unwrap_or("");
        let parsed = if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            token.parse().ok()
        };
        if let Some(value) = parsed {
            values.push(value);
        }
    }
    values
}

fn local_stamp() -> String {
    let mut time = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    unsafe { GetLocalTime(&mut time) };
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        time.year, time.month, time.day, time.hour, time.minute, time.second
    )
}

#[repr(C)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(time: *mut SystemTime);
}

#[cfg(test)]
mod tests {
    use super::{is_heavy_dir, reg_dwords, remove_tree};
    use std::{fs, process::Command};

    #[test]
    fn reads_reg_dwords() {
        let text = "    DpiValue    REG_DWORD    0x2\r\n    LogPixels    REG_DWORD    0x60\r\n";
        assert_eq!(reg_dwords(text), vec![2, 96]);
    }

    #[test]
    fn cache_dirs_are_heavy() {
        assert!(is_heavy_dir(std::path::Path::new(r"C:\Users\me\.runelite\jagexcache")));
        assert!(!is_heavy_dir(std::path::Path::new(r"C:\Users\me\.runelite\profiles2")));
    }

    #[test]
    fn remove_tree_unlinks_a_junction_without_deleting_the_target() {
        let root = std::env::temp_dir().join(format!("bash-repair-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        let parent = root.join("parent");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&parent).unwrap();
        fs::write(target.join("secret.txt"), b"keep").unwrap();
        let link = parent.join("link");
        let status = Command::new("cmd")
            .args(["/c", "mklink", "/J", &link.display().to_string(), &target.display().to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
        remove_tree(&parent).unwrap();
        let secret = fs::read_to_string(target.join("secret.txt")).unwrap();
        assert_eq!(secret, "keep");
        assert!(!link.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
