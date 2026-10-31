//! BASH.exe - the bootstrapper behind the desktop shortcut.
//!
//! Checks Simba, SRL-B, BashLib and the launcher against their latest
//! releases, offers to update (closing RuneLite / Simba first when they hold
//! our files open) and then opens the B.A.S.H launcher. Scripts run in a
//! headless Simba; the IDE stays closed.

use std::{
    env,
    path::PathBuf,
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::{
    download, install,
    launcher::Launcher,
    locks,
    log::{log_line, log_path, set_log_name},
    paths::{self, SimbaPaths},
    profile,
    state::{self, BashState},
    ui, versions, DISCORD_URL,
};
use eframe::egui;
use egui::{Align, Button, CentralPanel, Color32, Label, Layout, ProgressBar, RichText, ScrollArea, Stroke};

#[derive(Debug, Clone)]
struct Question {
    body: String,
    yes_label: String,
    no_label: String,
    changelog: String,
}

#[derive(Debug, Clone)]
enum WorkerMsg {
    Status(String),
    Progress(f32),
    Ask(Question),
    /// Something went wrong; the user may still open the launcher.
    Error(String),
    /// A short note for the launcher status line.
    Notice(String),
    /// Updates finished. Open the launcher.
    Ready,
    /// Worker finished and the window should close (`--no-launch`, or the user gave up).
    Done,
}

#[derive(Clone)]
struct Options {
    force_update: bool,
    /// Download SRL-B and BashLib again even when the installed versions match.
    reinstall_libraries: bool,
    no_launch: bool,
    silent: bool,
    root: PathBuf,
}

impl Options {
    fn parse() -> Self {
        let mut opts = Options {
            force_update: false,
            reinstall_libraries: false,
            no_launch: false,
            silent: false,
            root: paths::simba_install_dir(),
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_ascii_lowercase().as_str() {
                "--update" | "/update" => opts.force_update = true,
                "--no-launch" | "/no-launch" => opts.no_launch = true,
                "--silent" | "/silent" => opts.silent = true,
                "--simba-dir" => {
                    if let Some(dir) = args.next() {
                        opts.root = PathBuf::from(dir);
                    }
                }
                _ => {}
            }
        }
        // When BASH.exe lives inside a Simba folder, manage that folder.
        if let Ok(exe) = env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join(paths::SIMBA_EXE).exists() || dir.join("Data").exists() {
                    opts.root = dir.to_path_buf();
                }
            }
        }
        opts
    }
}

#[derive(Debug, Clone)]
enum Action {
    Simba(versions::Release),
    Package { name: &'static str, zip_url: String, version: String, tag: String },
    Launcher,
}

#[derive(Debug, Clone)]
struct UpdateItem {
    label: String,
    installed: String,
    latest: String,
    action: Action,
}

impl UpdateItem {
    fn describe(&self) -> String {
        if self.installed.is_empty() {
            format!("{}: not installed -> {}", self.label, self.latest)
        } else {
            format!("{}: {} -> {}", self.label, self.installed, self.latest)
        }
    }
}

struct Worker {
    tx: mpsc::Sender<WorkerMsg>,
    answers: mpsc::Receiver<bool>,
    silent: bool,
}

impl Worker {
    fn status(&self, text: impl Into<String>) {
        let text = text.into();
        log_line(&text);
        let _ = self.tx.send(WorkerMsg::Status(text));
    }

    fn progress(&self, p: f32) {
        let _ = self.tx.send(WorkerMsg::Progress(p.clamp(0.0, 1.0)));
    }

    fn ask(&self, body: impl Into<String>, yes: &str, no: &str) -> bool {
        self.ask_with_changelog(body, yes, no, String::new())
    }

    fn ask_with_changelog(&self, body: impl Into<String>, yes: &str, no: &str, changelog: String) -> bool {
        if self.silent {
            return true;
        }
        let _ = self.tx.send(WorkerMsg::Ask(Question {
            body: body.into(),
            yes_label: yes.to_string(),
            no_label: no.to_string(),
            changelog,
        }));
        self.answers.recv().unwrap_or(false)
    }

    fn error(&self, text: impl Into<String>) {
        let text = text.into();
        log_line(&format!("ERROR {text}"));
        let _ = self.tx.send(WorkerMsg::Error(text));
    }

    fn progress_sender(&self) -> (crate::ProgressSender, thread::JoinHandle<()>) {
        // Bridge ProgressMessage -> WorkerMsg so install helpers can report.
        let (ptx, prx) = mpsc::channel::<crate::ProgressMessage>();
        let tx = self.tx.clone();
        let handle = thread::spawn(move || {
            for msg in prx {
                match msg {
                    crate::ProgressMessage::Status(s) => {
                        let _ = tx.send(WorkerMsg::Status(s));
                    }
                    crate::ProgressMessage::Progress(p) => {
                        let _ = tx.send(WorkerMsg::Progress(p));
                    }
                    _ => {}
                }
            }
        });
        (ptx, handle)
    }
}

fn launch_simba(paths: &SimbaPaths) -> Result<(), String> {
    let launcher = paths.launcher();
    if !launcher.exists() {
        return Err(format!("Launcher not found at {}", launcher.display()));
    }
    Command::new(paths.simba_exe())
        .current_dir(&paths.root)
        .args(["--open", "--run"])
        .arg(&launcher)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to start Simba: {e}"))
}

/// After a newer BASH.exe has been downloaded, ask to restart into it now.
/// Returns true when this process should exit.
fn offer_launcher_restart(
    paths: &SimbaPaths,
    state: &BashState,
    worker: &Worker,
    opts: &Options,
    just_swapped: bool,
) -> bool {
    if opts.silent || opts.no_launch {
        return false;
    }
    let exe = paths.bootstrapper_exe();
    let staged = PathBuf::from(format!("{}.new", exe.display())).exists();
    if !staged && !just_swapped {
        return false;
    }
    let latest = state.bootstrapper.latest.trim();
    let body = if latest.is_empty() {
        "A new launcher update has been downloaded.\n\nRestart now to install it?".to_string()
    } else {
        format!("A new launcher update ({latest}) has been downloaded.\n\nRestart now to install it?")
    };
    let changelog = versions::latest_release_notes(versions::SETUP_REPO.0, versions::SETUP_REPO.1)
        .unwrap_or_else(|error| {
            log_line(&format!("launcher changelog: {error}"));
            "Could not load the changelog.".to_string()
        });
    if !worker.ask_with_changelog(body, "Yes", "No", changelog) {
        return false;
    }
    if staged && !install::finish_self_update(paths) {
        worker.error("Could not install the launcher update.");
        if !worker.answers.recv().unwrap_or(false) {
            let _ = worker.tx.send(WorkerMsg::Done);
            return true;
        }
        return false;
    }
    match Command::new(&exe).current_dir(&paths.root).spawn() {
        Ok(_) => {
            log_line("restarting into the new launcher");
            let _ = worker.tx.send(WorkerMsg::Done);
            true
        }
        Err(error) => {
            worker.error(format!("Could not restart the launcher: {error}"));
            if !worker.answers.recv().unwrap_or(false) {
                let _ = worker.tx.send(WorkerMsg::Done);
                return true;
            }
            false
        }
    }
}

fn center_launcher_window() -> bool {
    use std::mem::size_of;
    use windows::{
        core::BOOL,
        Win32::{
            Foundation::{HWND, LPARAM, RECT},
            Graphics::Gdi::{MonitorFromWindow, MONITORINFO, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST},
            System::Threading::GetCurrentProcessId,
            UI::WindowsAndMessaging::{
                EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
                IsWindowVisible, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
            },
        },
    };

    struct Hit {
        hwnd: HWND,
        width: i32,
        height: i32,
    }

    unsafe extern "system" fn find_launcher(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if lparam.0 == 0 {
            return BOOL(1);
        }
        let hit = &mut *(lparam.0 as *mut Option<Hit>);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != GetCurrentProcessId() || !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if title != crate::ui::WINDOW_TITLE {
            return BOOL(1);
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return BOOL(1);
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width < 1000 || height < 600 {
            return BOOL(1);
        }
        *hit = Some(Hit { hwnd, width, height });
        BOOL(0)
    }

    let mut hit: Option<Hit> = None;
    unsafe {
        let param = LPARAM(&mut hit as *mut Option<Hit> as isize);
        let _ = EnumWindows(Some(find_launcher), param);
        let Some(hit) = hit else {
            return false;
        };
        let monitor = MonitorFromWindow(hit.hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let area = info.rcWork;
        let x = area.left + (area.right - area.left - hit.width) / 2;
        let y = area.top + (area.bottom - area.top - hit.height) / 2;
        SetWindowPos(hit.hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE).is_ok()
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn run_worker(opts: &Options, worker: Worker) {
    let paths = SimbaPaths::new(opts.root.clone());
    let just_swapped = install::finish_self_update(&paths);
    install::clean_stale(&paths);
    let _ = install::ensure_directories(&paths);

    if !paths.simba_exe().exists() && !paths.launcher().exists() {
        worker.error(format!(
            "Simba is not installed in {}.\nRun bash-setup.exe to install B.A.S.H.",
            paths.root.display()
        ));
        return;
    }

    let mut state = BashState::load(&paths.state_file());
    let mut pending: Vec<UpdateItem> = Vec::new();
    let mut offline = false;

    let client = download::client(versions::USER_AGENT).ok();
    let no_redirect = download::client_no_redirect(versions::USER_AGENT).ok();

    worker.status(if opts.reinstall_libraries {
        "Reinstalling libraries..."
    } else {
        "Checking for updates..."
    });
    worker.progress(0.05);

    if let (Some(client), Some(no_redirect)) = (&client, &no_redirect) {
        // Simba
        match versions::latest_simba_release(no_redirect) {
            Ok(Some(release)) => {
                let installed_tag = state.simba_tag.clone();
                state.simba.latest = release.version();
                if !paths.simba_exe().exists() {
                    pending.push(UpdateItem {
                        label: "Simba".into(),
                        installed: String::new(),
                        latest: release.version(),
                        action: Action::Simba(release),
                    });
                } else if installed_tag.is_empty() {
                    // Pre-existing install with no record: assume current.
                    state.simba_tag = release.tag.clone();
                    state.simba.installed = release.version();
                } else if installed_tag != release.tag {
                    state.simba.installed = versions::normalize_version(&installed_tag);
                    pending.push(UpdateItem {
                        label: "Simba".into(),
                        installed: versions::normalize_version(&installed_tag),
                        latest: release.version(),
                        action: Action::Simba(release),
                    });
                } else {
                    state.simba.installed = release.version();
                }
            }
            Ok(None) => log_line("no Simba release found"),
            Err(e) => {
                log_line(&format!("simba check: {e}"));
                offline = true;
            }
        }
        worker.progress(0.15);

        // Libraries
        for (name, repo) in [(paths::SRLB_DIR, versions::SRLB_REPO), (paths::BASHLIB_DIR, versions::BASHLIB_REPO)] {
            let include = paths.include(name);
            let installed = versions::installed_lib_version(&include).unwrap_or_default();
            let component = if name == paths::SRLB_DIR { &mut state.srlb } else { &mut state.bashlib };
            component.installed = installed.clone();
            component.dev_checkout = false;
            match versions::library_target(client, no_redirect, repo.0, repo.1, &component.track) {
                Ok(Some(target)) => {
                    component.latest = target.stable_version.clone();
                    if component.track == "dev" {
                        component.dev_tag = target.dev_tag.clone();
                    }
                    let installed_this = if component.track.is_empty() {
                        include.exists() && installed == target.version
                    } else {
                        include.exists() && component.installed_tag == target.tag
                    };
                    let update_unpinned = opts.force_update && component.track.is_empty();
                    if !installed_this || opts.reinstall_libraries || update_unpinned {
                        pending.push(UpdateItem {
                            label: name.to_string(),
                            installed,
                            latest: target.version.clone(),
                            action: Action::Package {
                                name,
                                zip_url: target.zip_url,
                                version: target.version,
                                tag: target.tag,
                            },
                        });
                    }
                }
                Ok(None) => log_line(&format!("{name}: no release found for the selected version")),
                Err(e) => {
                    log_line(&format!("{name} check: {e}"));
                    offline = true;
                }
            }
        }
        worker.progress(0.25);

        // Launcher
        let installed_rev = versions::installed_launcher_revision(&paths.launcher()).unwrap_or(0);
        state.launcher.installed = if installed_rev > 0 { installed_rev.to_string() } else { String::new() };
        match versions::remote_launcher(client) {
            Ok(remote) => {
                state.launcher.latest = remote.revision.to_string();
                if remote.revision > installed_rev || !paths.launcher().exists() || opts.force_update {
                    pending.push(UpdateItem {
                        label: "B.A.S.H Launcher".into(),
                        installed: if installed_rev > 0 { format!("rev {installed_rev}") } else { String::new() },
                        latest: format!("rev {}", remote.revision),
                        action: Action::Launcher,
                    });
                }
            }
            Err(e) => {
                log_line(&format!("launcher check: {e}"));
                offline = true;
            }
        }
        worker.progress(0.30);
    } else {
        offline = true;
    }

    state.bootstrapper_version = versions::bootstrapper_version().to_string();
    state.touch();
    state::write_library_status(&mut state, offline);
    let _ = state.save(&paths.state_file());

    // Nothing to do (or offline): straight to Simba.
    let do_update = if pending.is_empty() {
        if offline {
            worker.status("Could not reach GitHub, starting Simba without updating.");
        }
        false
    } else {
        let lines: Vec<String> = pending.iter().map(|i| format!("  - {}", i.describe())).collect();
        let fresh = pending.iter().all(|i| i.installed.is_empty());
        if fresh || opts.force_update || opts.reinstall_libraries {
            // Missing components, or an explicit "--update" from the launcher's
            // Update button (already confirmed there): install without asking.
            true
        } else {
            let body = format!(
                "{} available:\n\n{}\n\nUpdating replaces library files that RuneLite and running scripts use.\nClose RuneLite and stop any running scripts first.",
                if opts.force_update { "Re-install / updates".to_string() } else { plural(pending.len(), "update") },
                lines.join("\n")
            );
            worker.ask(body, "Update now", "Skip this time")
        }
    };

    if do_update {
        let client = client.as_ref().unwrap();
        let needs_simba = pending.iter().any(|i| matches!(i.action, Action::Simba(_)));

        // When the launcher hands over to us it closes Simba right after
        // starting this process; give it a moment before complaining about it.
        if opts.force_update {
            let deadline = std::time::Instant::now() + Duration::from_secs(8);
            while std::time::Instant::now() < deadline
                && locks::find_update_lockers(&paths, needs_simba).iter().any(|l| l.is_simba())
            {
                worker.status("Waiting for Simba to close...");
                thread::sleep(Duration::from_millis(500));
            }
        }

        // RemoteInput injects into every RuneLite client, so library swaps need
        // every RuneLite closed — not only the ones Restart Manager already sees.
        let needs_libs = pending.iter().any(|item| matches!(item.action, Action::Package { .. }));
        let mut lockers = locks::find_update_lockers(&paths, needs_simba);
        let runelite_open = needs_libs && profile::runelite_running();
        let mut skip_update = false;
        if runelite_open || !lockers.is_empty() {
            let mut names: Vec<String> = lockers.iter().map(|l| l.friendly_name()).collect();
            if runelite_open {
                names.push("RuneLite".into());
            }
            names.sort();
            names.dedup();
            let body = format!(
                "{} {} using B.A.S.H files and must be closed before updating.\n\n\
                 RuneLite holds the RemoteInput plugin open. Every RuneLite client will be closed.\n\n\
                 Close {} and update now?",
                names.join(" and "),
                if names.len() == 1 { "is" } else { "are" },
                if names.len() == 1 { "it" } else { "them" }
            );
            if worker.ask(body, "Close and update", "Skip update") {
                worker.status(format!("Closing {}...", names.join(", ")));
                if runelite_open
                    || names.iter().any(|name| name.eq_ignore_ascii_case("RuneLite") || name.contains("RuneLite"))
                {
                    let _ = profile::close_runelite();
                    thread::sleep(Duration::from_millis(750));
                }
                let remaining = locks::close_processes(&lockers, Duration::from_secs(10));
                let files = locks::watched_files(&paths, needs_simba);
                if !remaining.is_empty() || !locks::wait_until_unlocked(&files, Duration::from_secs(15)) {
                    lockers = locks::find_update_lockers(&paths, needs_simba);
                } else {
                    lockers.clear();
                }
                if needs_libs && profile::runelite_running() {
                    lockers.push(locks::Locker {
                        pid: 0,
                        app_name: "RuneLite".into(),
                        exe_name: "RuneLite.exe".into(),
                    });
                }
            } else {
                skip_update = true;
            }
        }

        if skip_update || !lockers.is_empty() || (needs_libs && profile::runelite_running()) {
            let mut names: Vec<String> = lockers.iter().map(|l| l.friendly_name()).collect();
            if needs_libs && profile::runelite_running() {
                names.push("RuneLite".into());
            }
            names.sort();
            names.dedup();
            if names.is_empty() {
                worker.status("Update skipped.");
            } else {
                worker.status(format!("Update skipped: {} still running.", names.join(", ")));
            }
        } else {
            let (ptx, bridge) = worker.progress_sender();
            let total = pending.len().max(1) as f32;
            let mut failed: Vec<String> = Vec::new();
            for (idx, item) in pending.iter().enumerate() {
                let p0 = 0.30 + 0.65 * (idx as f32 / total);
                let p1 = 0.30 + 0.65 * ((idx + 1) as f32 / total);
                worker.status(format!("Updating {}...", item.label));
                let result = match &item.action {
                    Action::Simba(release) => install::install_executable(
                        client,
                        &release.asset_url(versions::SIMBA_ASSET),
                        &paths.simba_exe(),
                        Some(&ptx),
                        (p0, p1),
                    )
                    .map(|_| {
                        state.simba_tag = release.tag.clone();
                        state.simba.installed = release.version();
                    }),
                    Action::Package { name, zip_url, version, tag } => {
                        install::install_package(client, &paths, name, zip_url, Some(&ptx), (p0, p1)).map(|_| {
                            let component = if *name == paths::SRLB_DIR { &mut state.srlb } else { &mut state.bashlib };
                            component.installed = if tag.contains("-dev") {
                                version.clone()
                            } else {
                                versions::installed_lib_version(&paths.include(name)).unwrap_or(version.clone())
                            };
                            component.installed_tag = tag.clone();
                            if component.track == "dev" {
                                component.dev_tag = tag.clone();
                            }
                        })
                    }
                    Action::Launcher => install::download_and_install_launcher(client, &paths, Some(&ptx)).map(|rev| {
                        state.launcher.installed = rev.to_string();
                    }),
                };
                if let Err(e) = result {
                    log_line(&format!("{}: {e}", item.label));
                    failed.push(format!("{}: {e}", item.label));
                }
                worker.progress(p1);
            }
            drop(ptx);
            let _ = bridge.join();
            state::write_library_status(&mut state, offline);
            let _ = state.save(&paths.state_file());

            if !failed.is_empty() {
                worker.error(format!(
                    "Some updates failed:\n\n{}\n\nLog: {}",
                    failed.join("\n"),
                    log_path().display()
                ));
                // The UI offers "Launch anyway"; wait for the answer.
                if !worker.answers.recv().unwrap_or(false) {
                    let _ = worker.tx.send(WorkerMsg::Done);
                    return;
                }
            } else if opts.reinstall_libraries {
                let _ = worker.tx.send(WorkerMsg::Notice("Libraries reinstalled.".to_string()));
            }
        }
    }

    versions::refresh_app_version(&paths, &mut state, just_swapped);
    let _ = state.save(&paths.state_file());
    if offer_launcher_restart(&paths, &state, &worker, opts, just_swapped) {
        return;
    }

    worker.progress(1.0);
    if opts.reinstall_libraries && !pending.iter().any(|item| matches!(item.action, Action::Package { .. })) {
        let note = if offline {
            "Could not reach GitHub, so the libraries were not reinstalled."
        } else {
            "No SRL-B or BashLib release was found."
        };
        let _ = worker.tx.send(WorkerMsg::Notice(note.to_string()));
    }
    if opts.no_launch {
        worker.status("Done.");
        let _ = worker.tx.send(WorkerMsg::Done);
    } else {
        worker.status("Ready.");
        let _ = worker.tx.send(WorkerMsg::Ready);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Check,
    Launcher,
}

struct BashApp {
    opts: Options,
    rx: Option<mpsc::Receiver<WorkerMsg>>,
    answer_tx: Option<mpsc::Sender<bool>>,
    status: String,
    progress: f32,
    question: Option<Question>,
    error: Option<String>,
    logo: Option<egui::TextureHandle>,
    done: bool,
    phase: Phase,
    launcher: Option<Launcher>,
    launcher_started: bool,
    resized: bool,
    centered: bool,
    prompt_sized: bool,
    startup_theme: Option<ui::Theme>,
}

impl eframe::App for BashApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.logo.is_none() {
            self.logo = ui::load_logo(ctx);
        }
        if self.phase != Phase::Launcher {
            if self.startup_theme.is_none() {
                let saved = state::BashState::load(&SimbaPaths::new(self.opts.root.clone()).state_file()).theme;
                self.startup_theme = Some(ui::Theme::from_saved(&saved));
            }
            if let Some(theme) = self.startup_theme {
                ui::apply_theme(ctx, theme);
            }
        }

        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Status(s) => self.status = s,
                    WorkerMsg::Progress(p) => self.progress = p,
                    WorkerMsg::Ask(q) => self.question = Some(q),
                    WorkerMsg::Error(e) => self.error = Some(e),
                    WorkerMsg::Notice(note) => {
                        if let Some(launcher) = &mut self.launcher {
                            launcher.set_status(note);
                        }
                    }
                    WorkerMsg::Ready => {
                        self.phase = Phase::Launcher;
                        self.error = None;
                        self.question = None;
                        if let Some(launcher) = &mut self.launcher {
                            launcher.refresh_library_status();
                        }
                    }
                    WorkerMsg::Done => self.done = true,
                }
            }
        }
        if self.done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if self.phase == Phase::Launcher {
            if self.launcher.is_none() {
                self.launcher = Some(Launcher::new(SimbaPaths::new(self.opts.root.clone())));
            }
            let first_show = !self.launcher_started;
            let resize = !self.resized;
            let mut center_now = false;
            let mut recheck = false;
            let mut reinstall = false;
            let mut drew = false;
            if let Some(launcher) = &mut self.launcher {
                if first_show {
                    launcher.start();
                }
                launcher.tick();
                if launcher.reinstall_requested {
                    launcher.reinstall_requested = false;
                    launcher.recheck_requested = false;
                    reinstall = true;
                } else if launcher.recheck_requested {
                    launcher.recheck_requested = false;
                    recheck = true;
                } else {
                    if resize {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1040.0, 720.0)));
                        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(880.0, 560.0)));
                    }
                    if !self.centered && center_launcher_window() {
                        center_now = true;
                    }
                    launcher.ui(ctx);
                    launcher.output_windows(ctx);
                    drew = launcher.wants_fast_repaint();
                }
            }
            if first_show {
                self.launcher_started = true;
            }
            if resize {
                self.resized = true;
            }
            if center_now {
                self.centered = true;
            }
            if reinstall {
                self.begin_check(false, true);
            } else if recheck {
                self.begin_check(false, false);
            } else if self.phase == Phase::Launcher {
                // Idle frames follow input. A running script tails its log, so
                // that case keeps a short refresh. Repainting the whole window
                // many times a second is what flashed black under the cursor.
                let wait = if drew || !self.centered { 100 } else { 1500 };
                ctx.request_repaint_after(Duration::from_millis(wait));
                return;
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(6.0);
                ui::show_logo(ui, &self.logo, 72.0);
                ui.heading("B.A.S.H - BigAussie Script House");
                ui.add_space(10.0);

                if let Some(err) = self.error.clone() {
                    ui.label(RichText::new(err).color(Color32::from_rgb(230, 90, 90)));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([180.0, 30.0], Button::new("Open launcher anyway")).clicked() {
                            if let Some(tx) = &self.answer_tx {
                                let _ = tx.send(true);
                            }
                            self.error = None;
                        }
                        if ui.add_sized([140.0, 30.0], Button::new("Close")).clicked() {
                            if let Some(tx) = &self.answer_tx {
                                let _ = tx.send(false);
                            }
                            self.done = true;
                        }
                    });
                } else if let Some(q) = self.question.clone() {
                    if !q.changelog.is_empty() && !self.prompt_sized {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(520.0, 560.0)));
                        self.prompt_sized = true;
                    }
                    ui.add(Label::new(RichText::new(&q.body)).wrap());
                    ui.add_space(14.0);
                    let yes_clicked = centered_choice(ui, &q.yes_label, &q.no_label);
                    if yes_clicked == Some(true) {
                        if let Some(tx) = &self.answer_tx {
                            let _ = tx.send(true);
                        }
                        self.question = None;
                    } else if yes_clicked == Some(false) {
                        if let Some(tx) = &self.answer_tx {
                            let _ = tx.send(false);
                        }
                        self.question = None;
                    }
                    if !q.changelog.is_empty() {
                        ui.add_space(16.0);
                        ui.label(RichText::new("Changelog").strong());
                        ui.add_space(6.0);
                        ScrollArea::vertical()
                            .id_salt("update-changelog")
                            .max_height(220.0)
                            .max_width(460.0)
                            .show(ui, |ui| {
                                ui.set_min_width(440.0);
                                ui.set_max_width(440.0);
                                ui.add(Label::new(RichText::new(&q.changelog).size(13.0)).wrap());
                            });
                    }
                } else {
                    ui.label(&self.status);
                    ui.add_space(8.0);
                    ui.add_sized([400.0, 18.0], ProgressBar::new(self.progress).show_percentage());
                }

                ui.add_space(14.0);
                ui.hyperlink_to("Discord", DISCORD_URL);
            });
        });
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl BashApp {
    fn begin_check(&mut self, force_update: bool, reinstall_libraries: bool) {
        let mut opts = self.opts.clone();
        opts.force_update = force_update;
        opts.reinstall_libraries = reinstall_libraries;
        opts.no_launch = false;
        opts.silent = false;
        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        let (answer_tx, answer_rx) = mpsc::channel::<bool>();
        let worker = Worker { tx, answers: answer_rx, silent: false };
        thread::spawn(move || run_worker(&opts, worker));
        self.rx = Some(rx);
        self.answer_tx = Some(answer_tx);
        self.status = if reinstall_libraries {
            "Reinstalling libraries...".into()
        } else {
            "Checking for updates...".into()
        };
        self.progress = 0.0;
        self.question = None;
        self.error = None;
        self.done = false;
        self.prompt_sized = false;
        self.phase = Phase::Check;
    }
}

/// Yes on the left, No on the right, sized to their labels and centered.
/// `Some(true)` is Yes, `Some(false)` is No.
fn centered_choice(ui: &mut egui::Ui, yes: &str, no: &str) -> Option<bool> {
    let height = 34.0;
    let gap = 12.0;
    let yes_w = choice_width(ui, yes);
    let no_w = choice_width(ui, no);
    let total = yes_w + no_w + gap;
    let mut answer = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
        let yes_button = Button::new(RichText::new(yes).strong().color(Color32::from_rgb(28, 28, 32)))
            .fill(Color32::from_rgb(232, 196, 96))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(196, 156, 64)));
        if ui.add_sized([yes_w, height], yes_button).clicked() {
            answer = Some(true);
        }
        if ui.add_sized([no_w, height], Button::new(no)).clicked() {
            answer = Some(false);
        }
    });
    answer
}

fn choice_width(ui: &egui::Ui, label: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text = ui.fonts(|fonts| fonts.layout_no_wrap(label.to_string(), font, Color32::WHITE));
    let pad = ui.spacing().button_padding.x * 2.0 + 28.0;
    (text.size().x + pad).clamp(96.0, 180.0)
}

pub fn run() {
    set_log_name("bash-bootstrapper.log");
    log_line("BASH.exe starting");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_line(&format!("panic: {info}"));
        previous_hook(info);
    }));

    if crate::repair::requested() {
        log_line("repair mode");
        crate::repair::run_window();
        return;
    }

    let opts = Options::parse();
    let (tx, rx) = mpsc::channel::<WorkerMsg>();
    let (answer_tx, answer_rx) = mpsc::channel::<bool>();
    let worker = Worker { tx, answers: answer_rx, silent: opts.silent };

    if opts.silent {
        run_worker(&opts, worker);
        return;
    }

    let worker_opts = opts.clone();
    let fallback_root = opts.root.clone();
    thread::spawn(move || run_worker(&worker_opts, worker));

    let app = BashApp {
        opts,
        rx: Some(rx),
        answer_tx: Some(answer_tx),
        status: "Checking for updates...".to_string(),
        progress: 0.0,
        question: None,
        error: None,
        logo: None,
        done: false,
        phase: Phase::Check,
        launcher: None,
        launcher_started: false,
        resized: false,
        centered: false,
        prompt_sized: false,
        startup_theme: None,
    };
    let result = eframe::run_native(
        ui::WINDOW_TITLE,
        ui::native_options(520.0, 320.0),
        Box::new(|_cc| Ok(Box::new(app))),
    );
    if let Err(err) = result {
        log_line(&format!("gui failed: {err}"));
        // No window possible: fall back to launching Simba directly.
        let paths = SimbaPaths::new(fallback_root);
        if let Err(e) = launch_simba(&paths) {
            ui::show_error(&format!("B.A.S.H could not open its window or start Simba.\n\n{err}\n{e}"));
        }
    }
}
