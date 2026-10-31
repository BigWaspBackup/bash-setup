#![windows_subsystem = "windows"]
//! bash-setup.exe - the B.A.S.H installer wizard.

use std::{
    env,
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use bash_core::{
    download, install, locks,
    log::{log_line, log_path, set_log_name},
    paths::{self, SimbaPaths},
    profile,
    send_progress, send_status,
    shortcut::{self, ShortcutSpec},
    state::BashState,
    ui, versions, ProgressMessage, ProgressSender, DISCORD_URL,
};
use eframe::egui;
use egui::{vec2, Align, Button, CentralPanel, Color32, Layout, ProgressBar, RichText};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK};

/// Set once the installer window has drawn. A crash before this is the
/// "double-click does nothing" report: the process is a GUI app, so a failed
/// OpenGL startup exits with no console and no dialog.
static GUI_SHOWN: AtomicBool = AtomicBool::new(false);

struct SimbaInstaller {
    page: u8,
    paths: SimbaPaths,
    simba_exists: bool,
    install_progress: f32,
    is_installing: bool,
    installation_message: String,
    install_channel_rx: Option<mpsc::Receiver<ProgressMessage>>,
    is_error_message: bool,
    run_bash: bool,
    silent: bool,
    logo: Option<egui::TextureHandle>,
}

impl SimbaInstaller {
    fn new(silent: bool) -> Self {
        let paths = SimbaPaths::default_install();
        let simba_exists = paths.root.exists() && paths.root.is_dir();
        Self {
            page: 0,
            paths,
            simba_exists,
            install_progress: 0.0,
            is_installing: false,
            installation_message: "Ready to install...".to_string(),
            install_channel_rx: None,
            is_error_message: false,
            run_bash: true,
            silent,
            logo: None,
        }
    }

    fn show_discord_link(&self, ui: &mut egui::Ui) {
        ui.hyperlink_to("Join the B.A.S.H Discord", DISCORD_URL);
    }

    fn show_welcome_page(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui::show_logo(ui, &self.logo, 160.0);
            ui.allocate_ui_with_layout(vec2(width / 1.2, 140.0), Layout::top_down(Align::Center), |ui| {
                ui.label(
                    "Welcome to BASH Launcher 2.0 setup.\nBigAussie Script House - color scripts for OSRS.\nInstalls Simba, libs, and the desktop launcher.\nTo begin click Next.",
                );
                ui.add_space(8.0);
                self.show_discord_link(ui);
                ui.add_space(10.0);
                ui.label(format!("Install path is fixed to:\n{}", self.paths.root.display()));
            });
        });

        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
            if ui.add_sized([100.0, 30.0], Button::new("Next")).clicked() {
                self.page += 1;
            }
        });
    }

    fn show_existing_install_page(&mut self, ui: &mut egui::Ui) {
        if self.silent || !self.simba_exists {
            self.page += 1;
            return;
        }

        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.label(
                "An existing Simba installation was found.\n\nIt will be updated in place. Your credentials.simba, Configs folder,\nscripts and BashLib overrides.simba are kept.\n\nClose RuneLite and Simba before continuing.",
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(vec2(220.0, 40.0), Layout::left_to_right(Align::Center), |ui| {
                if ui.add_sized([100.0, 30.0], Button::new("Cancel")).clicked() {
                    self.page -= 1;
                }
                if ui.add_sized([100.0, 30.0], Button::new("Continue")).clicked() {
                    self.page += 1;
                }
            });
        });
    }

    fn show_confirm_page(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.label(
                "The installation might take a while.\n\nPlease don't close the installer midway to prevent corruption.",
            );
        });

        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
            if ui.add_sized([100.0, 30.0], Button::new("Install")).clicked() {
                self.page += 1;
            }
        });
    }

    fn show_installation_page(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.heading("Installation Progress");
            ui.add_space(20.0);

            if self.is_error_message {
                ui.label(RichText::new(&self.installation_message).color(Color32::RED).strong());
                ui.add_space(6.0);
                ui.label(format!("Log: {}", log_path().display()));
            } else {
                ui.label(&self.installation_message);
            }

            ui.add_space(10.0);
            ui.add_sized([400.0, 20.0], ProgressBar::new(self.install_progress).show_percentage());
            ui.add_space(20.0);

            if !self.is_installing && self.install_progress < 1.0 {
                self.is_installing = true;
                self.installation_message = "Starting installation process...".to_string();
                self.is_error_message = false;
                self.install_progress = 0.0;

                let (tx, rx) = mpsc::channel::<ProgressMessage>();
                self.install_channel_rx = Some(rx);
                let root = self.paths.root.clone();
                thread::spawn(move || {
                    run_installation(SimbaPaths::new(root), tx, false);
                });
            }

            if let Some(rx) = &self.install_channel_rx {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        ProgressMessage::Progress(p) => self.install_progress = p,
                        ProgressMessage::Status(s) => {
                            self.installation_message = s;
                            self.is_error_message = false;
                        }
                        ProgressMessage::Completed => {
                            self.install_progress = 1.0;
                            self.is_installing = false;
                            self.installation_message = "Installation successful!".to_string();
                            self.is_error_message = false;
                            self.page += 1;
                        }
                        ProgressMessage::Error(e) => {
                            self.install_progress = 1.0;
                            self.is_installing = false;
                            self.installation_message = format!("Installation failed: {}", e);
                            self.is_error_message = true;
                        }
                    }
                    ui.ctx().request_repaint();
                }
            }

            if self.is_error_message {
                ui.add_space(10.0);
                if ui.add_sized([120.0, 30.0], Button::new("Retry")).clicked() {
                    self.install_progress = 0.0;
                    self.is_error_message = false;
                }
            }
        });
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    fn show_final_page(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui::show_logo(ui, &self.logo, 120.0);
            ui.label("Installation Finished");
            ui.allocate_ui_with_layout(vec2(360.0, 140.0), Layout::top_down(Align::Center), |ui| {
                ui.label("Use the B.A.S.H shortcut on your desktop to start.");
                ui.label("It keeps Simba, SRL-B, BashLib and the launcher up to date.");
                ui.add_space(10.0);
                self.show_discord_link(ui);
                ui.add_space(20.0);
                ui.checkbox(&mut self.run_bash, "Launch BASH Launcher 2.0");
            });
        });

        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
            if ui.add_sized([120.0, 35.0], Button::new("Finish")).clicked() {
                if self.run_bash {
                    launch_bash(&self.paths);
                }
                std::process::exit(0);
            }
        });
    }
}

impl eframe::App for SimbaInstaller {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        GUI_SHOWN.store(true, Ordering::Relaxed);
        if self.logo.is_none() {
            self.logo = ui::load_logo(ctx);
        }

        CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();
            ui.heading("B.A.S.H - BigAussie Script House");
            ui.hyperlink_to("Discord", DISCORD_URL);
            ui.add_space(size.y / 8.0);

            match self.page {
                0 => self.show_welcome_page(ui, size.x),
                1 => self.show_existing_install_page(ui),
                2 => self.show_confirm_page(ui),
                3 => self.show_installation_page(ui),
                4 => self.show_final_page(ui),
                _ => {}
            }
        });
    }
}

fn launch_bash(paths: &SimbaPaths) {
    let bash = paths.bootstrapper_exe();
    if bash.exists() {
        let _ = Command::new(bash).current_dir(&paths.root).spawn();
    } else {
        let _ = Command::new(paths.simba_exe())
            .current_dir(&paths.root)
            .args(["--open", "--run"])
            .arg(paths.launcher())
            .spawn();
    }
}

fn fail(tx: &ProgressSender, message: impl Into<String>) {
    let message = message.into();
    log_line(&format!("ERROR {message}"));
    let _ = tx.send(ProgressMessage::Error(message));
}

fn copy_bootstrapper(from: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.new", dest.display()));
    fs::copy(from, &tmp).map_err(|e| format!("Failed to copy BASH.exe: {e}"))?;
    if dest.exists() {
        let old = PathBuf::from(format!("{}.old", dest.display()));
        let _ = fs::remove_file(&old);
        fs::rename(dest, &old).map_err(|e| format!("BASH.exe is in use: {e}"))?;
        fs::rename(&tmp, dest).map_err(|e| format!("Failed to install BASH.exe: {e}"))?;
        let _ = fs::remove_file(&old);
    } else {
        fs::rename(&tmp, dest).map_err(|e| format!("Failed to install BASH.exe: {e}"))?;
    }
    Ok(())
}

/// Install BASH.exe (BASH Launcher 2.0). Prefers a sibling BASH.exe from a
/// local release build, otherwise copies this installer (it runs as the
/// launcher when named BASH.exe), otherwise downloads BASH.exe from GitHub.
fn obtain_bootstrapper(client: &reqwest::blocking::Client, no_redirect: &reqwest::blocking::Client, paths: &SimbaPaths, tx: &ProgressSender) -> Result<(), String> {
    let dest = paths.bootstrapper_exe();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(paths::BOOTSTRAPPER_EXE);
            if sibling.exists() && sibling != dest {
                send_status(tx, "Installing BASH Launcher 2.0...");
                return copy_bootstrapper(&sibling, &dest);
            }
        }
        let own_name = exe.file_name().and_then(|name| name.to_str()).unwrap_or("");
        if !own_name.eq_ignore_ascii_case(paths::BOOTSTRAPPER_EXE) && exe != dest {
            send_status(tx, "Installing BASH Launcher 2.0...");
            return copy_bootstrapper(&exe, &dest);
        }
    }

    let release = versions::latest_release(no_redirect, versions::SETUP_REPO.0, versions::SETUP_REPO.1)?
        .ok_or_else(|| "No bash-setup release found for BASH.exe".to_string())?;
    install::install_executable(
        client,
        &release.asset_url(versions::BOOTSTRAPPER_ASSET),
        &dest,
        Some(tx),
        (0.90, 0.94),
    )
}

fn run_installation(paths: SimbaPaths, tx: ProgressSender, silent: bool) {
    let client = match download::client("BASH-Installer") {
        Ok(c) => c,
        Err(e) => return fail(&tx, e),
    };
    let no_redirect = match download::client_no_redirect("BASH-Installer") {
        Ok(c) => c,
        Err(e) => return fail(&tx, e),
    };

    send_status(&tx, "Preparing installation directory...");
    if let Err(e) = install::ensure_directories(&paths) {
        return fail(&tx, e);
    }
    install::clean_stale(&paths);
    if let Err(e) = install::write_data_files(&paths) {
        return fail(&tx, e);
    }
    send_progress(&tx, 0.05);

    // Anything holding our DLLs or Simba64.exe open has to go first.
    // RemoteInput injects into every RuneLite client, so always close all of them.
    let lockers = locks::find_update_lockers(&paths, true);
    let runelite_open = profile::runelite_running();
    if runelite_open || !lockers.is_empty() {
        let mut unique: Vec<String> = lockers.iter().map(|l| l.friendly_name()).collect();
        if runelite_open {
            unique.push("RuneLite".into());
        }
        unique.sort();
        unique.dedup();
        let proceed = silent
            || ui::ask_yes_no(&format!(
                "These programs are using Simba files and must be closed to continue:\n\n{}\n\n\
                 Every RuneLite client will be closed (RemoteInput holds the library DLLs open).\n\n\
                 Close them now?",
                unique.join("\n")
            ));
        if !proceed {
            return fail(&tx, format!("Close {} and run the installer again.", unique.join(", ")));
        }
        send_status(&tx, format!("Closing {}...", unique.join(", ")));
        if runelite_open {
            let _ = profile::close_runelite();
            thread::sleep(Duration::from_millis(750));
        }
        let remaining = locks::close_processes(&lockers, Duration::from_secs(10));
        if !remaining.is_empty() || profile::runelite_running() {
            let mut names: Vec<String> = remaining.iter().map(|l| l.friendly_name()).collect();
            if profile::runelite_running() {
                names.push("RuneLite".into());
            }
            names.sort();
            names.dedup();
            return fail(&tx, format!("Could not close {}. Close it manually and try again.", names.join(", ")));
        }
    }

    let mut state = BashState::load(&paths.state_file());

    // Simba
    send_status(&tx, "Checking for the latest Simba...");
    let simba_release = match versions::latest_simba_release(&no_redirect) {
        Ok(Some(r)) => r,
        Ok(None) => return fail(&tx, "No Simba release could be found on GitHub."),
        Err(e) => return fail(&tx, e),
    };
    if let Err(e) = install::install_executable(
        &client,
        &simba_release.asset_url(versions::SIMBA_ASSET),
        &paths.simba_exe(),
        Some(&tx),
        (0.05, 0.20),
    ) {
        return fail(&tx, e);
    }
    state.simba_tag = simba_release.tag.clone();
    state.simba.installed = simba_release.version();
    state.simba.latest = simba_release.version();

    // Libraries
    for (idx, (name, repo)) in [(paths::SRLB_DIR, versions::SRLB_REPO), (paths::BASHLIB_DIR, versions::BASHLIB_REPO)]
        .into_iter()
        .enumerate()
    {
        let range = if idx == 0 { (0.20, 0.50) } else { (0.50, 0.80) };
        send_status(&tx, format!("Checking the latest {name} release..."));
        let (zip_url, version) = match versions::latest_release(&no_redirect, repo.0, repo.1) {
            Ok(Some(release)) => (release.zip_url(), release.version()),
            Ok(None) => (
                format!("https://github.com/{}/{}/archive/refs/heads/master.zip", repo.0, repo.1),
                "master".to_string(),
            ),
            Err(e) => return fail(&tx, e),
        };
        match install::install_package(&client, &paths, name, &zip_url, Some(&tx), range) {
            Ok(_) => {}
            Err(e) => return fail(&tx, e),
        }
        let installed = versions::installed_lib_version(&paths.include(name)).unwrap_or(version.clone());
        let component = if idx == 0 { &mut state.srlb } else { &mut state.bashlib };
        component.installed = installed;
        component.latest = version;
        component.dev_checkout = false;
    }

    // Launcher
    match install::download_and_install_launcher(&client, &paths, Some(&tx)) {
        Ok(rev) => {
            state.launcher.installed = rev.to_string();
            state.launcher.latest = rev.to_string();
        }
        Err(e) => return fail(&tx, e),
    }
    send_progress(&tx, 0.90);

    // BASH.exe (non-fatal: the shortcut falls back to Simba directly)
    let have_bootstrapper = match obtain_bootstrapper(&client, &no_redirect, &paths, &tx) {
        Ok(()) => true,
        Err(e) => {
            log_line(&format!("BASH.exe not installed: {e}"));
            send_status(&tx, "BASH.exe unavailable, shortcut will open Simba directly.");
            false
        }
    };
    send_progress(&tx, 0.95);

    // Shortcuts
    send_status(&tx, "Creating shortcuts...");
    let root = paths.root.to_string_lossy().to_string();
    let launcher_arg = format!("--open --run \"{}\"", paths.launcher().display());
    let (target, arguments) = if have_bootstrapper {
        (paths.bootstrapper_exe().to_string_lossy().to_string(), String::new())
    } else {
        (paths.simba_exe().to_string_lossy().to_string(), launcher_arg)
    };
    let spec = ShortcutSpec {
        target: &target,
        arguments: &arguments,
        working_dir: &root,
        description: "BASH Launcher 2.0 - BigAussie Script House",
        icon: None,
    };
    shortcut::create_desktop_shortcut("B.A.S.H", &spec);
    shortcut::create_start_menu_shortcut("B.A.S.H", &spec);
    shortcut::remove_desktop_shortcut("Simba64");
    shortcut::create_desktop_shortcut(
        "Simba Folder",
        &ShortcutSpec {
            target: &root,
            arguments: "",
            working_dir: &root,
            description: "Simba Folder",
            icon: None,
        },
    );

    state.bootstrapper_version = versions::bootstrapper_version().to_string();
    state.touch();
    if let Err(e) = state.save(&paths.state_file()) {
        log_line(&e);
    }

    send_status(&tx, "Installation complete!");
    send_progress(&tx, 1.0);
    let _ = tx.send(ProgressMessage::Completed);
}

fn is_silent_arg(arg: &str) -> bool {
    arg.eq_ignore_ascii_case("--silent") || arg.eq_ignore_ascii_case("/silent")
}

fn run_silent() -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<ProgressMessage>();
    let paths = SimbaPaths::default_install();
    thread::spawn(move || run_installation(paths, tx, true));

    let mut completed = false;
    for msg in rx {
        match msg {
            ProgressMessage::Completed => completed = true,
            ProgressMessage::Error(err) => return Err(err),
            _ => {}
        }
    }
    if completed {
        Ok(())
    } else {
        Err("Installation stopped before it finished.".to_string())
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn AllocConsole() -> i32;
}

fn offer_headless_install(error: &str) {
    let prompt = format!(
        "B.A.S.H Setup couldn't open its window, so it looked like nothing happened.\n\n\
         {error}\n\n\
         Install anyway without the window?\n\
         This downloads Simba into your local AppData\\Simba folder and can take a few minutes.\n\
         A console window will show progress."
    );
    log_line(&format!("gui failed: {error}"));
    if !ui::ask_yes_no(&prompt) {
        return;
    }

    unsafe {
        AllocConsole();
    }
    let mut console = match fs::OpenOptions::new().write(true).open("CONOUT$") {
        Ok(file) => file,
        Err(err) => {
            ui::show_error(&format!("Could not open a console window: {err}"));
            return;
        }
    };
    let _ = writeln!(console, "Installing B.A.S.H. This can take a few minutes. Leave this window open.");

    let (tx, rx) = mpsc::channel::<ProgressMessage>();
    let paths = SimbaPaths::default_install();
    thread::spawn(move || run_installation(paths, tx, false));

    let mut completed = false;
    let mut failed = None;
    for msg in rx {
        match msg {
            ProgressMessage::Status(status) => {
                let _ = writeln!(console, "{status}");
            }
            ProgressMessage::Progress(progress) => {
                let _ = writeln!(console, "{:.0}%", progress * 100.0);
            }
            ProgressMessage::Completed => {
                completed = true;
                let _ = writeln!(console, "Installation complete.");
            }
            ProgressMessage::Error(err) => {
                let _ = writeln!(console, "Installation failed: {err}");
                failed = Some(err);
            }
        }
    }

    if let Some(err) = failed {
        ui::show_error(&format!("Installation failed:\n\n{err}"));
    } else if completed {
        ui::message_box(
            "Installation complete. Use the B.A.S.H shortcut on your desktop.",
            MB_OK | MB_ICONINFORMATION,
        );
    } else {
        ui::show_error(&format!(
            "Installation stopped before it finished.\n\nDetails were written to {}",
            log_path().display()
        ));
    }
}

fn run_gui() -> Result<(), String> {
    eframe::run_native(
        "BASH Launcher 2.0 Setup",
        ui::native_options(800.0, 600.0),
        Box::new(|_cc| Ok(Box::new(SimbaInstaller::new(false)))),
    )
    .map_err(|err| err.to_string())
}

fn launched_as_bootstrapper() -> bool {
    env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().eq_ignore_ascii_case(paths::BOOTSTRAPPER_EXE)))
        .unwrap_or(false)
}

fn main() {
    if launched_as_bootstrapper() {
        bash_core::bootstrap::run();
        return;
    }
    set_log_name("bash-setup.log");
    log_line("bash-setup starting");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_line(&format!("panic: {info}"));
        previous_hook(info);
    }));

    let silent = env::args().skip(1).any(|arg| is_silent_arg(&arg));
    if silent {
        if let Err(err) = run_silent() {
            ui::show_error(&format!("Installation failed:\n\n{err}"));
            std::process::exit(1);
        }
        return;
    }

    let gui_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_gui));
    let startup_error = match gui_result {
        Ok(Ok(())) => None,
        Ok(Err(err)) => Some(err),
        Err(_) => Some(
            "The installer crashed before the window opened. A graphics driver problem is the usual cause.".to_string(),
        ),
    };

    if let Some(err) = startup_error {
        if GUI_SHOWN.load(Ordering::Relaxed) {
            ui::show_error(&format!("B.A.S.H Setup hit an error:\n\n{err}"));
        } else {
            offer_headless_install(&err);
        }
    }
}
