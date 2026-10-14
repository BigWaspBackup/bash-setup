#![windows_subsystem = "windows"]
use std::{
    env,
    fs::{remove_file, File, OpenOptions},
    io::{Cursor, Write},
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use eframe::{egui, NativeOptions};
use egui::{vec2, Align, Button, CentralPanel, Color32, IconData, Layout, ProgressBar, RichText};
use image::ImageReader;
use windows::{
    core::HSTRING,
    Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNO,
    },
};

/// Set once the installer window has drawn. A crash before this is the
/// "double-click does nothing" report: the process is a GUI app, so a failed
/// OpenGL startup exits with no console and no dialog.
static GUI_SHOWN: AtomicBool = AtomicBool::new(false);

mod utils;
use utils::{create_shortcut, download_file, download_launcher, prepare_directory, unzip};

const DISCORD_URL: &str = "https://discord.gg/qsmKs5uKfR";
const ICON_BYTES: &[u8] = include_bytes!("../assets/bash.png");

pub enum InstallMessage {
    Progress(f32),
    Status(String),
    Completed,
    Error(String),
}

struct SimbaInstaller {
    page: u8,
    simba_dir: PathBuf,
    simba_exists: bool,
    install_progress: f32,
    is_installing: bool,
    installation_message: String,
    install_channel_rx: Option<mpsc::Receiver<InstallMessage>>,
    pub is_error_message: bool,
    run_simba: bool,
    silent: bool,
    logo: Option<egui::TextureHandle>,
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    zipball_url: String,
}

fn download_simba_executable(
    client: &reqwest::blocking::Client,
    simba_dir: &PathBuf,
    tx: &mpsc::Sender<InstallMessage>,
    current_progress: &mut f32,
    weight: f32,
) -> bool {
    if !download_file(
        client,
        "https://github.com/Torwent/Simba/releases/latest/download/Simba-Win64.exe",
        "Simba64.exe",
        simba_dir,
        tx,
    ) {
        return false;
    }
    *current_progress += weight;
    let _ = tx.send(InstallMessage::Progress(*current_progress));
    true
}

fn download_github_zipball(
    client: &reqwest::blocking::Client,
    owner: &str,
    repo: &str,
    local_file_name: &str,
    install_name: &str,
    simba_dir: &PathBuf,
    tx: &mpsc::Sender<InstallMessage>,
    current_progress: &mut f32,
    weight: f32,
) -> bool {
    let _ = tx.send(InstallMessage::Status(format!(
        "Fetching {} source archive...",
        repo
    )));

    // Prefer latest release zipball when available; fall back to master branch.
    let release_url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    );
    let mut zipball_url = format!(
        "https://api.github.com/repos/{}/{}/zipball/master",
        owner, repo
    );

    if let Ok(response) = client.get(&release_url).send() {
        if response.status().is_success() {
            if let Ok(text) = response.text() {
                if let Ok(release) = serde_json::from_str::<GitHubRelease>(&text) {
                    zipball_url = release.zipball_url;
                }
            }
        }
    }

    let start_progress = *current_progress;
    let weight_value = weight / 2.0;
    if !download_file(client, &zipball_url, local_file_name, simba_dir, tx) {
        return false;
    }
    *current_progress += weight_value;
    let _ = tx.send(InstallMessage::Progress(*current_progress));

    let _ = tx.send(InstallMessage::Status(format!("Unzipping {}...", repo)));

    let zip_path = simba_dir.join(local_file_name);
    let zip_file = match File::open(&zip_path) {
        Ok(file) => file,
        Err(e) => {
            let _ = tx.send(InstallMessage::Error(format!(
                "Failed to open downloaded zip for {}: {}",
                repo, e
            )));
            return false;
        }
    };

    let includes_repo_path = simba_dir.join("Includes").join(install_name);
    if let Err(e) = unzip(zip_file, &includes_repo_path) {
        let _ = tx.send(InstallMessage::Error(format!(
            "Failed to extract zip for {}: {}",
            repo, e
        )));
        return false;
    }

    let _ = remove_file(&zip_path);
    *current_progress = start_progress + weight;
    let _ = tx.send(InstallMessage::Progress(*current_progress));
    true
}

fn simba_install_dir() -> PathBuf {
    if let Ok(local) = env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("Simba");
    }
    if let Ok(profile) = env::var("USERPROFILE") {
        return PathBuf::from(profile)
            .join("AppData")
            .join("Local")
            .join("Simba");
    }
    PathBuf::from(r"C:\Users\Public\Simba")
}

impl SimbaInstaller {
    fn new(_cc: &eframe::CreationContext<'_>, silent: bool) -> Self {
        let simba_dir = simba_install_dir();
        let simba_exists = simba_dir.exists() && simba_dir.is_dir();

        Self {
            page: 0,
            simba_dir,
            simba_exists,
            install_progress: 0.0,
            is_installing: false,
            installation_message: "Ready to install...".to_string(),
            install_channel_rx: None,
            is_error_message: false,
            run_simba: false,
            silent,
            logo: None,
        }
    }

    fn ensure_logo(&mut self, ctx: &egui::Context) {
        if self.logo.is_some() {
            return;
        }
        if let Ok(img) = ImageReader::with_format(Cursor::new(ICON_BYTES), image::ImageFormat::Png)
            .decode()
        {
            let rgba = img.into_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            self.logo = Some(ctx.load_texture("bash_logo", color_image, Default::default()));
        }
    }

    fn show_logo(&self, ui: &mut egui::Ui, max_size: f32) {
        if let Some(logo) = &self.logo {
            ui.add(egui::Image::new(logo).max_size(vec2(max_size, max_size)));
            ui.add_space(8.0);
        }
    }

    fn show_discord_link(&self, ui: &mut egui::Ui) {
        ui.hyperlink_to("Join the B.A.S.H Discord", DISCORD_URL);
    }

    fn show_welcome_page(&mut self, ui: &mut egui::Ui, width: f32, _height: f32) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            self.show_logo(ui, 160.0);
            ui.allocate_ui_with_layout(
                vec2(width / 1.2, 140.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.label(
                        "Welcome to the B.A.S.H Simba installer.\nBigAussie Script House - color scripts for OSRS.\nTo begin the installation click Next.",
                    );
                    ui.add_space(8.0);
                    self.show_discord_link(ui);
                    ui.add_space(10.0);
                    ui.label(format!(
                        "Install path is fixed to:\n{}",
                        self.simba_dir.display()
                    ));
                },
            );
        });

        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
            if ui.add_sized([100.0, 30.0], Button::new("Next")).clicked() {
                self.page += 1;
            }
        });
    }

    fn show_existing_install_warning_page(&mut self, ui: &mut egui::Ui) {
        if self.silent {
            self.page += 1;
            return;
        }

        if self.simba_exists {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.label(
                    "You seem to have a previous Simba installation.\nContinuing may result in loss of previous data.",
                );
                ui.allocate_ui_with_layout(
                    vec2(210.0, 120.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.with_layout(
                            Layout::left_to_right(Align::Center).with_main_wrap(false),
                            |ui| {
                                if ui.add_sized([100.0, 30.0], Button::new("Cancel")).clicked()
                                {
                                    self.page -= 1;
                                }
                                if ui.add_sized([100.0, 30.0], Button::new("Continue")).clicked()
                                {
                                    self.page += 1;
                                }
                            },
                        );
                    },
                );
            });
            return;
        }
        self.page += 1;
    }

    fn show_version_selection_page(&mut self, ui: &mut egui::Ui) {
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
                ui.label(
                    RichText::new(&self.installation_message)
                        .color(Color32::RED)
                        .strong(),
                );
            } else {
                ui.label(&self.installation_message);
            }

            ui.add_space(10.0);
            ui.add_sized(
                [400.0, 20.0],
                ProgressBar::new(self.install_progress).show_percentage(),
            );
            ui.add_space(20.0);

            if !self.is_installing && self.install_progress < 1.0 {
                self.is_installing = true;
                self.installation_message = "Starting installation process...".to_string();
                self.is_error_message = false;
                self.install_progress = 0.0;

                let (tx, rx) = mpsc::channel::<InstallMessage>();
                self.install_channel_rx = Some(rx);

                let simba_dir_for_thread = self.simba_dir.clone();
                let tx_for_thread = tx.clone();

                thread::spawn(move || {
                    SimbaInstaller::run_installation_thread(simba_dir_for_thread, tx_for_thread);
                });
            }

            if let Some(rx) = &self.install_channel_rx {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        InstallMessage::Progress(p) => {
                            self.install_progress = p;
                            ui.ctx().request_repaint();
                        }
                        InstallMessage::Status(s) => {
                            self.installation_message = s;
                            self.is_error_message = false;
                            ui.ctx().request_repaint();
                        }
                        InstallMessage::Completed => {
                            self.install_progress = 1.0;
                            self.is_installing = false;
                            self.installation_message = "Installation successful!".to_string();
                            self.is_error_message = false;
                            self.page += 1;
                            ui.ctx().request_repaint();
                        }
                        InstallMessage::Error(e) => {
                            self.install_progress = 1.0;
                            self.is_installing = false;
                            self.installation_message = format!("Installation failed: {}", e);
                            self.is_error_message = true;
                            ui.ctx().request_repaint();
                        }
                    }
                }
            }
        });
    }

    fn run_installation_thread(simba_dir: PathBuf, tx: mpsc::Sender<InstallMessage>) {
        let client = match reqwest::blocking::Client::builder()
            .user_agent("BASH-Installer")
            .timeout(Duration::from_secs(300))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                let _ = tx.send(InstallMessage::Error(format!(
                    "Failed to start the download client: {e}"
                )));
                return;
            }
        };

        let mut current_progress = 0.0;

        let initial_setup_weight = 0.05;
        let dir_prep_weight = 0.10;
        let simba_exe_download_weight = 0.15;
        let srl_b_download_weight = 0.3;
        let bashlib_download_weight = 0.3;
        let launcher_download_weight = 0.05;

        let _ = tx.send(InstallMessage::Status(
            "Initializing installation process...".to_string(),
        ));
        thread::sleep(Duration::from_millis(200));
        current_progress += initial_setup_weight;
        let _ = tx.send(InstallMessage::Progress(current_progress));

        let dir_prep_start_progress = current_progress;
        if !prepare_directory(
            &simba_dir,
            &tx,
            &mut current_progress,
            dir_prep_weight,
            dir_prep_start_progress,
        ) {
            return;
        }

        let start_progress = current_progress;
        if !download_simba_executable(
            &client,
            &simba_dir,
            &tx,
            &mut current_progress,
            simba_exe_download_weight,
        ) {
            return;
        }
        current_progress = start_progress + simba_exe_download_weight;
        let _ = tx.send(InstallMessage::Progress(current_progress));

        if !download_github_zipball(
            &client,
            "BigWaspBackup",
            "SRL-B",
            "SRL-B.zip",
            "SRL-B",
            &simba_dir,
            &tx,
            &mut current_progress,
            srl_b_download_weight,
        ) {
            return;
        }

        if !download_github_zipball(
            &client,
            "BigWaspBackup",
            "BashLib",
            "BashLib.zip",
            "BashLib",
            &simba_dir,
            &tx,
            &mut current_progress,
            bashlib_download_weight,
        ) {
            return;
        }

        if !download_launcher(
            &client,
            &simba_dir,
            &tx,
            &mut current_progress,
            launcher_download_weight,
        ) {
            return;
        }

        create_shortcut(
            simba_dir.join("Simba64.exe").to_str().unwrap(),
            "Desktop/Simba64",
            "Simba 64 bits",
        );

        create_shortcut(
            simba_dir.to_str().unwrap(),
            "Desktop/SimbaFolder",
            "Simba Folder",
        );

        let _ = tx.send(InstallMessage::Status(
            "Finalizing installation...".to_string(),
        ));
        thread::sleep(Duration::from_secs(1));
        current_progress = 1.0;
        let _ = tx.send(InstallMessage::Progress(current_progress));
        let _ = tx.send(InstallMessage::Status(
            "Installation complete!".to_string(),
        ));
        let _ = tx.send(InstallMessage::Completed);
    }

    fn show_final_page(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            self.show_logo(ui, 120.0);
            ui.label("Installation Finished");
            ui.allocate_ui_with_layout(vec2(320.0, 140.0), Layout::top_down(Align::Center), |ui| {
                ui.label("You can start using Simba now!");
                ui.label("Open BashLauncher.simba (or press play) to use B.A.S.H.");
                ui.add_space(10.0);
                self.show_discord_link(ui);
                ui.add_space(20.0);
                ui.checkbox(&mut self.run_simba, "Launch Simba");
            });
        });

        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
            if ui.add_sized([120.0, 35.0], Button::new("Finish")).clicked() {
                if self.run_simba {
                    let simba_path = self.simba_dir.join("Simba64.exe");
                    let _ = Command::new(simba_path).spawn();
                }
                std::process::exit(0);
            }
        });
    }
}

impl eframe::App for SimbaInstaller {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        GUI_SHOWN.store(true, Ordering::Relaxed);
        self.ensure_logo(ctx);

        CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();
            let width = size.x;
            let height = size.y;

            ui.heading("B.A.S.H - BigAussie Script House");
            ui.hyperlink_to("Discord", DISCORD_URL);
            ui.add_space(height / 8.0);

            match self.page {
                0 => self.show_welcome_page(ui, width, height),
                1 => self.show_existing_install_warning_page(ui),
                2 => self.show_version_selection_page(ui),
                3 => self.show_installation_page(ui),
                4 => self.show_final_page(ui),
                _ => {}
            }
        });
    }
}

fn load_icon_from_embedded_png(bytes: &[u8]) -> Result<IconData, String> {
    let img = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png)
        .decode()
        .map_err(|e| format!("Failed to decode embedded icon image: {}", e))?;
    // The source art is 705x690. CreateIcon rejects or crashes on icons that large,
    // which aborts startup before any window exists.
    let img = img.resize(256, 256, image::imageops::FilterType::Triangle);
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    Ok(IconData {
        rgba: pixels,
        width,
        height,
    })
}

fn log_line(message: &str) {
    let path = env::temp_dir().join("bash-setup.log");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{stamp} {message}");
    }
}

fn show_message(message: &str, style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE) {
    log_line(message);
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from("B.A.S.H Setup"),
            style,
        );
    }
}

fn show_error(message: &str) {
    show_message(message, MB_OK | MB_ICONERROR);
}

fn is_silent_arg(arg: &str) -> bool {
    arg.eq_ignore_ascii_case("--silent") || arg.eq_ignore_ascii_case("/silent")
}

fn run_silent() -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<InstallMessage>();
    let simba_dir = simba_install_dir();
    thread::spawn(move || {
        SimbaInstaller::run_installation_thread(simba_dir, tx);
    });

    let mut completed = false;
    for msg in rx {
        match msg {
            InstallMessage::Completed => completed = true,
            InstallMessage::Error(err) => return Err(err),
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
    let answer = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(&prompt),
            &HSTRING::from("B.A.S.H Setup"),
            MB_YESNO | MB_ICONERROR,
        )
    };
    log_line(&format!("gui failed: {error}"));
    if answer != IDYES {
        return;
    }

    unsafe {
        AllocConsole();
    }
    let mut console = match OpenOptions::new().write(true).open("CONOUT$") {
        Ok(file) => file,
        Err(err) => {
            show_error(&format!("Could not open a console window: {err}"));
            return;
        }
    };
    let _ = writeln!(
        console,
        "Installing B.A.S.H. This can take a few minutes. Leave this window open."
    );

    let (tx, rx) = mpsc::channel::<InstallMessage>();
    let simba_dir = simba_install_dir();
    thread::spawn(move || {
        SimbaInstaller::run_installation_thread(simba_dir, tx);
    });

    let mut completed = false;
    let mut failed = None;
    for msg in rx {
        match msg {
            InstallMessage::Status(status) => {
                let _ = writeln!(console, "{status}");
            }
            InstallMessage::Progress(progress) => {
                let _ = writeln!(console, "{:.0}%", progress * 100.0);
            }
            InstallMessage::Completed => {
                completed = true;
                let _ = writeln!(console, "Installation complete.");
            }
            InstallMessage::Error(err) => {
                let _ = writeln!(console, "Installation failed: {err}");
                failed = Some(err);
            }
        }
    }

    if let Some(err) = failed {
        show_error(&format!("Installation failed:\n\n{err}"));
    } else if completed {
        show_message(
            "Installation complete. Open Simba from the desktop shortcut.",
            MB_OK | MB_ICONINFORMATION,
        );
    } else {
        show_error(
            "Installation stopped before it finished.\n\nDetails were written to %TEMP%\\bash-setup.log",
        );
    }
}

fn run_gui() -> Result<(), String> {
    let icon_data = match load_icon_from_embedded_png(ICON_BYTES) {
        Ok(icon) => Some(icon),
        Err(err) => {
            log_line(&format!("icon skipped: {err}"));
            None
        }
    };

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(eframe::egui::vec2(800.0, 600.0));
    if let Some(icon_data) = icon_data {
        viewport = viewport.with_icon(icon_data);
    }

    // eframe creates the window hidden and only shows it after the first swap.
    // With vsync on, some Windows drivers never complete that swap for a hidden
    // window, so the process sits there and the user sees nothing.
    let options = NativeOptions {
        viewport,
        vsync: false,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "B.A.S.H Simba Installer",
        options,
        Box::new(|cc| Ok(Box::new(SimbaInstaller::new(cc, false)))),
    )
    .map_err(|err| err.to_string())
}

fn main() {
    log_line("bash-setup starting");
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_line(&format!("panic: {info}"));
        previous_hook(info);
    }));

    let silent = std::env::args().skip(1).any(|arg| is_silent_arg(&arg));
    if silent {
        if let Err(err) = run_silent() {
            show_error(&format!("Installation failed:\n\n{err}"));
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
            show_error(&format!("B.A.S.H Setup hit an error:\n\n{err}"));
        } else {
            offer_headless_install(&err);
        }
    }
}
