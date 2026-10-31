//! The window users stay in. Pick a client, install a script, run it in
//! headless Simba, and read its output here. The Simba IDE is not opened.

use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use egui::{
    Align, Button, Color32, Frame, Image, Label, Layout, Margin, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, UiBuilder,
};

use crate::{
    agility,
    catalog::{self, RemoteScript, ScriptEntry},
    scriptprofile,
    clients::{self, ClientWindow},
    log::log_line,
    paths::SimbaPaths,
    profile, ui as chrome,
    versions,
};

const STORE_URL: &str = "https://bashscripts.shop/";
const DISCORD_URL: &str = "https://discord.gg/qsmKs5uKfR";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Same values the Simba editor writes to a script's stdin.
const SIMBA_PAUSED: u32 = 0;
const SIMBA_STOP: u32 = 1;
/// How long a Stop request may run its own cleanup before the process is killed.
/// Killing sooner skips the handler that turns real mouse and keyboard input back on.
const STOP_GRACE: Duration = Duration::from_secs(12);
/// How long the follow-up release may take before it is closed.
const RELEASE_GRACE: Duration = Duration::from_secs(25);
const SIMBA_RUNNING: u32 = 2;
const CTRL_PAUSE: u8 = 1;
const CTRL_RESUME: u8 = 2;
const LIBRARY_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

enum ExtraAction {
    Open(PathBuf),
    Reinstall,
    ClearCache,
    Repair,
    InstallLib(String),
    Copied,
}

enum NetMsg {
    Catalog { gen: u64, result: Result<(Vec<RemoteScript>, String), String> },
    Download { id: String, result: Result<PathBuf, String> },
    Releases { name: String, result: Result<Vec<versions::ListedRelease>, String> },
    LibInstalled { name: String, result: Result<String, String> },
    Discord(catalog::DiscordLogin),
    Changelog { gen: u64, result: Result<Vec<catalog::ChangelogEntry>, String> },
}

struct Running {
    id: u64,
    name: String,
    log_path: PathBuf,
    log_text: String,
    /// Bytes the stdout/stderr readers have pulled off the pipes since the last drain.
    incoming: Arc<Mutex<Vec<u8>>>,
    /// Reader threads still open. The exit line waits until this hits zero so it stays last.
    output_left: Arc<AtomicU8>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    hwnd: isize,
    pid: u32,
    account: String,
    /// 0 none, 1 pause, 2 resume. Stop uses `stop`.
    ctrl: Arc<AtomicU8>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    stop_sent: bool,
    stop_at: Option<Instant>,
    dismiss: Arc<AtomicBool>,
    /// Set when the output window is closed while the script is still running.
    dismiss_after: Arc<AtomicBool>,
    exit_note: Option<String>,
    note_shown: bool,
    /// When this Simba process was started.
    started: Instant,
    /// Seconds of this session already stored in `play_seconds`.
    booked: u64,
    /// Latest status reported by the script, such as "On a break" or "Farm run".
    activity: String,
    /// Output that has not reached a newline yet. Status lines are pulled out of this.
    line_buf: String,
    /// The output window is closed, and the script keeps running.
    output_hidden: Arc<AtomicBool>,
    /// Where the output window sits relative to its RuneLite window.
    anchor: OutputAnchor,
    /// Short Simba process that turns real input back on after a forced stop.
    release: Option<Child>,
    release_at: Option<Instant>,
}

impl Running {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// The script is still in Simba, or the follow-up release has not finished.
    fn active(&self) -> bool {
        self.child.is_some() || self.release.is_some()
    }
}

pub struct Launcher {
    paths: SimbaPaths,
    clients: Vec<ClientWindow>,
    selected_hwnd: Option<isize>,
    remote: Vec<RemoteScript>,
    scripts: Vec<ScriptEntry>,
    selected: Option<usize>,
    filter: String,
    license_key: String,
    install_id: String,
    license_status: String,
    status: String,
    loading: bool,
    tx: mpsc::Sender<NetMsg>,
    rx: mpsc::Receiver<NetMsg>,
    fetch_gen: u64,
    runs: Vec<Running>,
    next_run: u64,
    last_client_scan: Option<Instant>,
    pub recheck_requested: bool,
    pub reinstall_requested: bool,
    /// "up_to_date", "available", "offline", or empty before the startup check.
    library_status: String,
    library_detail: String,
    last_library_check: Option<Instant>,
    library_check_rx: Option<mpsc::Receiver<()>>,
    /// First launch asks to install the RuneLite profile inside the window.
    profile_prompt_open: bool,
    discord_prompt_open: bool,
    settings_open: bool,
    repair_confirm: bool,
    /// "up_to_date", "available", "restart", "offline", or empty before the check.
    app_status: String,
    app_latest: String,
    logo: Option<egui::TextureHandle>,
    icons: HashMap<String, egui::TextureHandle>,
    icons_loaded: bool,
    category: String,
    skill: String,
    /// Script ids pinned to the top of the list.
    favorites: Vec<String>,
    releases: HashMap<String, Vec<versions::ListedRelease>>,
    release_choice: HashMap<String, String>,
    releases_loading: bool,
    releases_done: u8,
    release_error: String,
    installing_lib: String,
    discord_pending: bool,
    changelog_open: bool,
    changelog_name: String,
    changelog_loading: bool,
    changelog_error: String,
    changelog_entries: Vec<catalog::ChangelogEntry>,
    changelog_gen: u64,
    /// All-time script seconds per account name.
    play_seconds: HashMap<String, u64>,
    /// The script output window follows its RuneLite window.
    anchor_output: bool,
    theme: chrome::Theme,
    /// Virtual clicks inside the client. Stored in wasplib.json.
    remote_input: bool,
    /// When on, the real mouse and keyboard are ignored while a script runs.
    block_real_input: bool,
    /// RuneLite account names are shown as Hidden outside account settings.
    anonymous: bool,
    /// Accepted license key is visible until the launcher is closed.
    show_license: bool,
    /// Script to run after its update download finishes.
    run_after_update: Option<String>,
    /// Shared script settings, and the AIO Agility run setup.
    profile_editor: Option<scriptprofile::Editor>,
    /// Account key for the setup that just pressed Start.
    agility_account: Option<String>,
}

impl Launcher {
    pub fn new(paths: SimbaPaths) -> Self {
        let config = catalog::load_config(&paths.launcher_config());
        let play_seconds = crate::state::BashState::load(&paths.state_file()).play_seconds;
        let (tx, rx) = mpsc::channel();
        Self {
            paths,
            clients: Vec::new(),
            selected_hwnd: None,
            remote: Vec::new(),
            scripts: Vec::new(),
            selected: None,
            filter: String::new(),
            license_key: config.license_key,
            install_id: config.install_id,
            license_status: "Loading scripts...".into(),
            status: String::new(),
            loading: false,
            tx,
            rx,
            fetch_gen: 0,
            runs: Vec::new(),
            next_run: 1,
            last_client_scan: None,
            recheck_requested: false,
            reinstall_requested: false,
            library_status: String::new(),
            library_detail: String::new(),
            last_library_check: None,
            library_check_rx: None,
            profile_prompt_open: false,
            discord_prompt_open: false,
            settings_open: false,
            repair_confirm: false,
            app_status: String::new(),
            app_latest: String::new(),
            logo: None,
            icons: HashMap::new(),
            icons_loaded: false,
            category: "all".into(),
            skill: String::new(),
            favorites: Vec::new(),
            releases: HashMap::new(),
            release_choice: HashMap::new(),
            releases_loading: false,
            releases_done: 0,
            release_error: String::new(),
            installing_lib: String::new(),
            discord_pending: false,
            changelog_open: false,
            changelog_name: String::new(),
            changelog_loading: false,
            changelog_error: String::new(),
            changelog_entries: Vec::new(),
            changelog_gen: 0,
            play_seconds,
            anchor_output: true,
            theme: chrome::Theme::Dark,
            remote_input: true,
            block_real_input: true,
            anonymous: false,
            show_license: false,
            run_after_update: None,
            profile_editor: None,
            agility_account: None,
        }
    }

    /// A running script's log, or a catalog fetch, needs frames without the user moving the mouse.
    pub fn wants_fast_repaint(&self) -> bool {
        self.loading
            || self.script_running()
            || self.releases_loading
            || !self.installing_lib.is_empty()
            || self.discord_pending
            || self.changelog_loading
    }

    pub fn start(&mut self) {
        let _ = catalog::save_config(&self.paths.launcher_config(), &self.license_key, &self.install_id);
        self.refresh_clients();
        self.reload_catalog();
        self.refresh_library_status();
        let state = crate::state::BashState::load(&self.paths.state_file());
        self.favorites = state.favorites;
        self.anchor_output = state.anchor_output;
        self.theme = chrome::Theme::from_saved(&state.theme);
        let remote = crate::wasplib::load(&self.paths.wasplib_config());
        self.anonymous = state.anonymous;
        self.remote_input = remote.enabled;
        self.block_real_input = remote.block_real_input;
        self.profile_prompt_open = !state.prompted_profile && !profile::is_installed(&self.install_id);
        if let Some(client_open) = profile::fix_launcher_scale() {
            self.status = if client_open {
                "RuneLite scaling was set to 1.0. Close RuneLite and open it again so the client uses it.".into()
            } else {
                "RuneLite scaling was set to 1.0.".into()
            };
        }
        let logged_in = !state.discord_user.trim().is_empty();
        let has_key = !self.license_key.trim().is_empty();
        self.discord_prompt_open = !logged_in && !has_key;
    }

    fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|saved| saved == id)
    }

    fn toggle_favorite(&mut self, id: &str) {
        if let Some(index) = self.favorites.iter().position(|saved| saved == id) {
            self.favorites.remove(index);
        } else {
            self.favorites.insert(0, id.to_string());
        }
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.favorites = self.favorites.clone();
        if let Err(e) = state.save(&path) {
            self.status = e;
        }
    }

    fn mark_profile_prompted(&mut self) {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        if state.prompted_profile {
            return;
        }
        state.prompted_profile = true;
        let _ = state.save(&path);
    }

    fn profile_prompt_window(&mut self, ctx: &egui::Context) {
        if !self.profile_prompt_open {
            return;
        }
        let mut install = false;
        let mut later = false;
        egui::Window::new("RuneLite profile")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Install the BASH RuneLite profile?");
                ui.label("Scripts expect this profile. You can also install it later with Install RuneLite profile.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Install").clicked() {
                        install = true;
                    }
                    if ui.button("Not now").clicked() {
                        later = true;
                    }
                });
            });
        if install || later {
            self.profile_prompt_open = false;
            self.mark_profile_prompted();
        }
        if install {
            self.install_runelite_profile();
        }
    }

    fn remember_discord_user(&mut self, username: &str) {
        let name = if username.trim().is_empty() { "Discord" } else { username.trim() };
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.discord_user = name.to_string();
        let _ = state.save(&path);
        self.discord_prompt_open = false;
    }

    fn discord_prompt_window(&mut self, ctx: &egui::Context) {
        if !self.discord_prompt_open || self.profile_prompt_open {
            return;
        }
        let mut login = false;
        let mut skip = false;
        egui::Modal::new(egui::Id::new("discord-login-prompt")).show(ctx, |ui| {
            ui.set_min_width(420.0);
            ui.set_max_width(420.0);
            ui.label(RichText::new("Log in with Discord").strong().size(18.0));
            ui.add_space(6.0);
            let note = "Log in with your Discord account to automatically retrieve your license key and log in.";
            let galley = egui::WidgetText::from(note).into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                400.0,
                egui::TextStyle::Body,
            );
            ui.add(Label::new(galley));
            ui.add_space(12.0);
            let label = if self.discord_pending {
                "Waiting for Discord..."
            } else {
                "Log in with Discord"
            };
            if ui.add_enabled(!self.discord_pending, Button::new(label)).clicked() {
                login = true;
            }
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add(Button::new(RichText::new("Skip").size(12.0).weak()))
                    .clicked()
                {
                    skip = true;
                }
            });
        });
        if login {
            self.start_discord_login();
        }
        if skip {
            self.discord_prompt_open = false;
        }
    }

    fn open_changelog(&mut self, script: &ScriptEntry) {
        self.changelog_open = true;
        self.changelog_name = script.name.clone();
        self.changelog_loading = true;
        self.changelog_error.clear();
        self.changelog_entries.clear();
        self.changelog_gen += 1;
        let gen = self.changelog_gen;
        let id = script.id.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = catalog::fetch_changelog(&id);
            let _ = tx.send(NetMsg::Changelog { gen, result });
        });
    }

    fn changelog_window(&mut self, ctx: &egui::Context) {
        if !self.changelog_open {
            return;
        }
        let mut open = true;
        let name = self.changelog_name.clone();
        let loading = self.changelog_loading;
        let error = self.changelog_error.clone();
        let entries = self.changelog_entries.clone();
        egui::Window::new("Changelog")
            .collapsible(false)
            .resizable(true)
            .default_width(480.0)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.label(RichText::new(&name).strong().size(16.0));
                ui.add_space(8.0);
                if loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading changelog...");
                    });
                } else if !error.is_empty() {
                    ui.label(RichText::new(&error).color(Color32::from_rgb(230, 90, 90)));
                } else if entries.is_empty() {
                    ui.label("No changelog yet.");
                } else {
                    ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (index, entry) in entries.iter().enumerate() {
                            if index > 0 {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);
                            }
                            let when = entry.at.split('T').next().unwrap_or("").trim();
                            let heading = match (entry.revision, when.is_empty()) {
                                (Some(revision), false) => format!("Revision {revision}  ·  {when}"),
                                (Some(revision), true) => format!("Revision {revision}"),
                                (None, false) => when.to_string(),
                                (None, true) => "Update".to_string(),
                            };
                            ui.label(RichText::new(heading).strong());
                            ui.add_space(4.0);
                            let galley = egui::WidgetText::from(entry.notes.as_str()).into_galley(
                                ui,
                                Some(egui::TextWrapMode::Wrap),
                                440.0,
                                egui::TextStyle::Body,
                            );
                            ui.add(Label::new(galley));
                        }
                    });
                }
            });
        self.changelog_open = open;
    }

    fn version_label(&self) -> (String, Color32) {
        let version = versions::bootstrapper_version();
        if self.app_status == "restart" || self.update_staged() {
            (
                format!("Version {version} · restart to finish the update"),
                Color32::from_rgb(230, 176, 72),
            )
        } else if self.app_status == "available" {
            let latest = if self.app_latest.is_empty() {
                "update available".to_string()
            } else {
                format!("{} available", self.app_latest)
            };
            (format!("Version {version} · {latest}"), Color32::from_rgb(230, 176, 72))
        } else {
            let color = if self.theme.is_dark() {
                Color32::from_rgb(160, 160, 168)
            } else {
                Color32::from_rgb(80, 84, 92)
            };
            (format!("Version {version}"), color)
        }
    }

    fn update_staged(&self) -> bool {
        let mut exes = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            exes.push(exe);
        }
        exes.push(self.paths.bootstrapper_exe());
        exes.into_iter()
            .any(|exe| PathBuf::from(format!("{}.new", exe.display())).exists())
    }

    fn running_installed_copy(&self) -> bool {
        std::env::current_exe().ok().as_ref() == Some(&self.paths.bootstrapper_exe())
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        self.load_releases();
        let mut open = true;
        let mut changed = false;
        let mut action = None;
        let max_height = (ctx.screen_rect().height() - 96.0).max(280.0);
        egui::Window::new("Settings")
            .collapsible(false)
            .auto_sized()
            .vscroll(true)
            .max_height(max_height)
            .constrain(true)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(460.0);
                ui.set_max_width(460.0);
                ui.label(RichText::new("Appearance").strong());
                ui.horizontal(|ui| {
                    let dark = ui.selectable_label(self.theme == chrome::Theme::Dark, "Dark").clicked();
                    let light = ui.selectable_label(self.theme == chrome::Theme::Light, "Light").clicked();
                    if dark {
                        self.theme = chrome::Theme::Dark;
                    } else if light {
                        self.theme = chrome::Theme::Light;
                    }
                    if dark || light {
                        chrome::apply_theme(ui.ctx(), self.theme);
                        self.save_theme();
                    }
                });
                ui.add_space(10.0);

                ui.label(RichText::new("Output").strong());
                if ui
                    .checkbox(
                        &mut self.anchor_output,
                        "Keep the output window beside the RuneLite client",
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.add_space(4.0);
                let note = "The output window follows the RuneLite window it belongs to. Turn this off to leave the output window where you place it.";
                let galley = egui::WidgetText::from(RichText::new(note).weak().size(12.0)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    440.0,
                    egui::TextStyle::Body,
                );
                ui.add(Label::new(galley));
                ui.add_space(10.0);

                ui.label(RichText::new("Privacy").strong());
                if ui.checkbox(&mut self.anonymous, "Anonymous").changed() {
                    self.save_anonymous();
                }
                ui.add_space(4.0);
                let privacy = "RuneLite account names are shown as Hidden in the client list, running panel, output window, and logs.";
                let galley = egui::WidgetText::from(RichText::new(privacy).weak().size(12.0)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    440.0,
                    egui::TextStyle::Body,
                );
                ui.add(Label::new(galley));
                ui.add_space(10.0);

                ui.label(RichText::new("Remote input").strong());
                let remote_note = "Lets a script click and type inside the client without moving your real mouse.";
                let galley = egui::WidgetText::from(RichText::new(remote_note).weak().size(12.0)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    440.0,
                    egui::TextStyle::Body,
                );
                ui.add(Label::new(galley));
                ui.add_space(4.0);
                if ui.checkbox(&mut self.remote_input, "Enable remote input").changed() {
                    self.save_remote_input();
                }
                let block_on = self.remote_input;
                if ui
                    .add_enabled(
                        block_on,
                        egui::Checkbox::new(&mut self.block_real_input, "Block real input"),
                    )
                    .on_hover_text("Stops your real mouse and keyboard from reaching the game while a script runs.")
                    .changed()
                {
                    self.save_remote_input();
                }
                if self.remote_input && !self.block_real_input {
                    let warning = "Warning: touching the game while a script is running can cause issues.";
                    let galley = egui::WidgetText::from(
                        RichText::new(warning).color(Color32::from_rgb(230, 176, 72)).size(12.0),
                    )
                    .into_galley(ui, Some(egui::TextWrapMode::Wrap), 440.0, egui::TextStyle::Body);
                    ui.add(Label::new(galley));
                }
                ui.add_space(10.0);

                ui.label(RichText::new("Folders").strong());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Script screenshots").on_hover_text("Screenshots saved by scripts").clicked() {
                        action = Some(ExtraAction::Open(self.paths.screenshots()));
                    }
                    if ui.button("Simba folder").clicked() {
                        action = Some(ExtraAction::Open(self.paths.root.clone()));
                    }
                    if ui.button("Installed scripts").clicked() {
                        action = Some(ExtraAction::Open(self.paths.installed_scripts()));
                    }
                });
                ui.add_space(10.0);

                self.library_versions(ui, &mut action);
                ui.add_space(10.0);

                ui.label(RichText::new("Repair").strong());
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!self.script_running(), Button::new("Repair installation"))
                        .on_hover_text("Back up Simba and RuneLite, reinstall them, and restore the key and settings. Windows asks for administrator approval.")
                        .on_disabled_hover_text("Stop the running script first.")
                        .clicked()
                    {
                        action = Some(ExtraAction::Repair);
                    }
                    if ui
                        .add_enabled(!self.script_running(), Button::new("Reinstall libraries"))
                        .on_hover_text("Download SRL-B and BashLib again.")
                        .clicked()
                    {
                        action = Some(ExtraAction::Reinstall);
                    }
                    if ui
                        .add_enabled(!self.script_running(), Button::new("Clear cache"))
                        .on_hover_text("Delete extracted item images, map chunks, and other Simba cache files. Scripts rebuild them the next time they run.")
                        .on_disabled_hover_text("Stop the running script first.")
                        .clicked()
                    {
                        action = Some(ExtraAction::ClearCache);
                    }
                });
                ui.add_space(10.0);

                ui.label(RichText::new("Details").strong());
                let details = egui::WidgetText::from(RichText::new(self.debug_info()).size(12.0)).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    440.0,
                    egui::TextStyle::Body,
                );
                ui.add(Label::new(details));
                if ui.button("Copy details").clicked() {
                    ui.ctx().copy_text(self.debug_info());
                    action = Some(ExtraAction::Copied);
                }
                ui.add_space(10.0);

                let (version, version_color) = self.version_label();
                ui.label(RichText::new(version).color(version_color));
                let version_note = if self.app_status == "available" && !self.running_installed_copy() && !self.update_staged() {
                    Some("A newer launcher is published. This window is not the installed BASH.exe, so it was not replaced.")
                } else if self.app_status == "available" || self.update_staged() {
                    Some("A new launcher is downloaded. Close BASH and open it again, then choose Yes to restart into it.")
                } else if self.app_status == "restart" {
                    Some("The newer launcher is already in place. Close BASH and open it again.")
                } else if self.app_status == "offline" {
                    Some("Could not check the launcher for updates. Library status is unchanged.")
                } else {
                    None
                };
                if let Some(note) = version_note {
                    let galley = egui::WidgetText::from(note).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Wrap),
                        440.0,
                        egui::TextStyle::Body,
                    );
                    ui.add(Label::new(galley));
                }
            });
        self.settings_open = open;
        if changed {
            self.save_anchor_setting();
        }
        if let Some(action) = action {
            self.run_extra(action);
        }
    }

    fn save_remote_input(&mut self) {
        let settings = crate::wasplib::RemoteInput {
            enabled: self.remote_input,
            block_real_input: self.block_real_input,
        };
        if let Err(message) = crate::wasplib::save(&self.paths.wasplib_config(), settings) {
            let fresh = crate::wasplib::load(&self.paths.wasplib_config());
            self.remote_input = fresh.enabled;
            self.block_real_input = fresh.block_real_input;
            self.status = message;
        }
    }

    fn save_anchor_setting(&self) {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.anchor_output = self.anchor_output;
        let _ = state.save(&path);
    }

    fn save_theme(&self) {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.theme = self.theme.as_str().to_string();
        let _ = state.save(&path);
    }

    fn save_anonymous(&self) {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.anonymous = self.anonymous;
        let _ = state.save(&path);
    }

    fn run_extra(&mut self, action: ExtraAction) {
        match action {
            ExtraAction::Open(path) => {
                if chrome::open_path(&path) {
                    self.status = format!("Opened {}.", path.display());
                } else {
                    self.status = format!("Could not open {}.", path.display());
                }
            }
            ExtraAction::Reinstall => {
                self.reinstall_requested = true;
                self.settings_open = false;
            }
            ExtraAction::ClearCache => self.clear_cache(),
            ExtraAction::Repair => self.repair_confirm = true,
            ExtraAction::InstallLib(name) => self.install_chosen_library(&name),
            ExtraAction::Copied => self.status = "Copied launcher details.".into(),
        }
    }

    fn library_versions(&mut self, ui: &mut egui::Ui, action: &mut Option<ExtraAction>) {
        ui.label(RichText::new("Library versions").strong());
        let help = "Revert your installed libs to older versions. Only use this if there are issues with the current versions.";
        let galley = egui::WidgetText::from(RichText::new(help).weak().size(12.0)).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            440.0,
            egui::TextStyle::Body,
        );
        ui.add(Label::new(galley));
        ui.add_space(4.0);
        if self.releases_loading {
            ui.label("Loading releases...");
        }
        if !self.release_error.is_empty() {
            ui.label(RichText::new(&self.release_error).color(Color32::from_rgb(220, 90, 80)).size(12.0));
            if ui.button("Try again").clicked() {
                self.releases.clear();
                self.release_error.clear();
                self.releases_done = 0;
                self.releases_loading = false;
            }
        }
        let state = crate::state::BashState::load(&self.paths.state_file());
        for (name, component) in [("SRL-B", &state.srlb), ("BashLib", &state.bashlib)] {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).strong());
                let note = library_track_note(component);
                if !note.is_empty() {
                    ui.label(RichText::new(note).weak());
                }
            });
            let list = self.releases.get(name).cloned().unwrap_or_default();
            let mut choice = self
                .release_choice
                .get(name)
                .cloned()
                .unwrap_or_else(|| if component.track.is_empty() { "latest".into() } else { component.track.clone() });
            let selected = release_choice_label(&choice, &list);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!list.is_empty(), |ui| {
                    egui::ComboBox::from_id_salt(format!("lib-{name}"))
                        .selected_text(selected)
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut choice, "latest".to_string(), latest_choice_label(&list));
                            if list.iter().any(|release| release.prerelease) {
                                ui.selectable_value(&mut choice, "dev".to_string(), dev_choice_label(&list));
                            }
                            for release in &list {
                                ui.selectable_value(&mut choice, release.tag.clone(), release.label());
                            }
                        });
                });
                let busy = self.script_running() || !self.installing_lib.is_empty() || list.is_empty();
                let label = if self.installing_lib == name { "Installing..." } else { "Install" };
                let mut button = ui.add_enabled(!busy, Button::new(label));
                if self.script_running() {
                    button = button.on_disabled_hover_text("Stop the running script first.");
                }
                if button.clicked() {
                    *action = Some(ExtraAction::InstallLib(name.to_string()));
                }
            });
            self.release_choice.insert(name.to_string(), choice);
        }
    }

    fn load_releases(&mut self) {
        if self.releases_loading || self.releases_done > 0 {
            return;
        }
        self.releases_loading = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let client = match crate::download::client(versions::USER_AGENT) {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(NetMsg::Releases { name: "SRL-B".into(), result: Err(error.clone()) });
                    let _ = tx.send(NetMsg::Releases { name: "BashLib".into(), result: Err(error) });
                    return;
                }
            };
            for (name, repo) in [("SRL-B", versions::SRLB_REPO), ("BashLib", versions::BASHLIB_REPO)] {
                let result = versions::list_releases(&client, repo.0, repo.1);
                let _ = tx.send(NetMsg::Releases { name: name.into(), result });
            }
        });
    }

    fn install_chosen_library(&mut self, name: &str) {
        if self.script_running() {
            self.status = "Stop the running script before changing a library.".into();
            return;
        }
        if !self.installing_lib.is_empty() {
            return;
        }
        let choice = self.release_choice.get(name).cloned().unwrap_or_else(|| "latest".into());
        let list = self.releases.get(name).cloned().unwrap_or_default();
        let Some(spec) = resolve_library_install(name, &choice, &list) else {
            self.status = if choice == "dev" {
                format!("No dev release of {name} has been published.")
            } else {
                format!("Could not find that {name} release.")
            };
            return;
        };
        if !chrome::ask_yes_no(&format!(
            "Install {name} {}?\n\nEvery RuneLite client will be closed first — RemoteInput keeps the library DLLs open.\n\nUpdate libraries keeps this choice. Pick Latest here to go back to the normal release.",
            spec.label
        )) {
            return;
        }
        if crate::profile::runelite_running() {
            self.status = "Closing RuneLite...".into();
            if !crate::profile::close_runelite() || crate::profile::runelite_running() {
                self.status = "Could not close RuneLite. Close every client, then try again.".into();
                return;
            }
        }
        self.installing_lib = name.to_string();
        self.status = format!("Installing {name} {}...", spec.label);
        let paths = self.paths.clone();
        let tx = self.tx.clone();
        let lib_name = name.to_string();
        thread::spawn(move || {
            let result = install_library_release(&paths, &lib_name, &spec).map(|()| spec.label);
            let _ = tx.send(NetMsg::LibInstalled { name: lib_name, result });
        });
    }

    fn repair_window(&mut self, ctx: &egui::Context) {
        if !self.repair_confirm {
            return;
        }
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        egui::Window::new("Repair installation")
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    ui.set_min_width(480.0);
                    ui.label("Use this when BASH or RuneLite is broken. It runs the full repair.");
                    ui.add_space(6.0);
                    ui.label("BASH closes first. Windows asks for administrator approval, then the repair window shows the progress.");
                    ui.add_space(8.0);
                    for step in crate::repair::STEPS {
                        ui.label(format!("• {step}"));
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        if ui.button("Repair installation").clicked() {
                            start = true;
                        }
                    });
                });
            });
        if cancel {
            open = false;
        }
        self.repair_confirm = open;
        if start {
            self.repair_confirm = false;
            if self.script_running() {
                self.status = "Stop the running script before repairing.".into();
                return;
            }
            match crate::repair::launch_elevated() {
                Ok(()) => std::process::exit(0),
                Err(error) => self.status = error,
            }
        }
    }

    fn debug_info(&self) -> String {
        let state = crate::state::BashState::load(&self.paths.state_file());
        let show = |value: &str| {
            if value.is_empty() {
                "unknown".to_string()
            } else {
                value.to_string()
            }
        };
        let component = |name: &str, item: &crate::state::ComponentState| {
            format!("{name} {} (latest {})", show(&item.installed), show(&item.latest))
        };
        format!(
            "BASH {}\n{}\n{}\n{}\nInstall id {}\nSimba folder {}",
            versions::bootstrapper_version(),
            component("Simba", &state.simba),
            component("SRL-B", &state.srlb),
            component("BashLib", &state.bashlib),
            if self.install_id.is_empty() { "none".to_string() } else { self.install_id.clone() },
            self.paths.root.display(),
        )
    }

    fn clear_cache(&mut self) {
        if !chrome::ask_yes_no(
            "Clear the Simba cache?\n\nThis deletes extracted item images, map chunks, and other cached files. Scripts rebuild them the next time they run.",
        ) {
            return;
        }
        let dir = self.paths.cache();
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                self.status = format!("Could not clear the Simba cache: {e}");
                return;
            }
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("Could not recreate the Simba cache folder: {e}");
            return;
        }
        self.status = "Cleared the Simba cache.".into();
    }

    fn install_runelite_profile(&mut self) {
        if profile::runelite_running()
            && !chrome::ask_yes_no("RuneLite must be closed before the profile can be installed.\n\nClose RuneLite now?")
        {
            self.status = "RuneLite profile install cancelled.".into();
            return;
        }
        if profile::runelite_running() && !profile::close_runelite() {
            self.status = "Could not close RuneLite. Close it, then try the profile again.".into();
            return;
        }
        match profile::install(&self.install_id) {
            Ok(done) => {
                let verb = if done.updated { "Updated" } else { "Installed" };
                match profile::set_active(&self.install_id) {
                    Ok(()) => {
                        let message = format!(
                            "{verb} RuneLite profile \"{}\".\n\nPlease open your RuneLite client again.",
                            done.name
                        );
                        self.status = format!("{verb} RuneLite profile {}. Please open your RuneLite client again.", done.name);
                        chrome::show_info(&message);
                    }
                    Err(error) => {
                        let message = format!("{verb} RuneLite profile \"{}\", but it could not be selected.\n\n{error}", done.name);
                        self.status = format!("{verb} RuneLite profile {}, but it could not be selected.", done.name);
                        chrome::show_error(&message);
                    }
                }
            }
            Err(error) => {
                self.status = error.clone();
                chrome::show_error(&error);
            }
        }
    }

    /// The first time any script is run, offer Settings Searcher instead.
    fn first_run_choice(&mut self, script: &ScriptEntry) -> Option<ScriptEntry> {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        if state.prompted_setup || script.name.eq_ignore_ascii_case("Settings Searcher") {
            if !state.prompted_setup {
                state.prompted_setup = true;
                let _ = state.save(&path);
            }
            return Some(script.clone());
        }
        let run_searcher = chrome::ask_yes_no(
            "This is the first time a script is being run.\n\nSettings Searcher applies the game settings BASH scripts expect. Run Settings Searcher now instead?",
        );
        state.prompted_setup = true;
        let _ = state.save(&path);
        if !run_searcher {
            return Some(script.clone());
        }
        let Some(searcher) = self.scripts.iter().find(|entry| entry.name.eq_ignore_ascii_case("Settings Searcher")).cloned() else {
            self.status = "Settings Searcher is not in the script list yet.".into();
            return None;
        };
        if let Some(index) = self.scripts.iter().position(|entry| entry.id == searcher.id) {
            self.selected = Some(index);
        }
        if searcher.local_path.is_none() {
            self.status = "Install Settings Searcher, then run it before other scripts.".into();
            return None;
        }
        Some(searcher)
    }

    pub fn set_status(&mut self, note: impl Into<String>) {
        self.status = note.into();
    }

    pub fn refresh_library_status(&mut self) {
        let state = crate::state::BashState::load(&self.paths.state_file());
        self.library_status = state.library_status;
        self.app_status = state.bootstrapper_status;
        self.app_latest = state.bootstrapper.latest;
        let mut notes = Vec::new();
        for (name, component) in [("Simba", &state.simba), ("SRL-B", &state.srlb), ("BashLib", &state.bashlib)] {
            if component.track == "dev" {
                let installed = if component.installed.is_empty() { "not installed" } else { component.installed.as_str() };
                notes.push(format!("{name} {installed} · dev"));
            } else if !component.track.is_empty() {
                let version = component.track.trim().trim_start_matches(['v', 'V']);
                notes.push(format!("{name} {version} · pinned"));
            } else if component.is_behind() {
                let installed = if component.installed.is_empty() {
                    "not installed"
                } else {
                    component.installed.as_str()
                };
                notes.push(format!("{name} {installed} → {}", component.latest));
            } else if !component.installed.is_empty() {
                notes.push(format!("{name} {}", component.installed));
            }
        }
        self.library_detail = notes.join("\n");
        self.last_library_check = Some(Instant::now());
    }

    pub fn script_running(&self) -> bool {
        self.runs.iter().any(|run| run.active())
    }

    fn run_on_client(&self, hwnd: isize) -> Option<&Running> {
        let pid = self.clients.iter().find(|client| client.hwnd == hwnd).map(|client| client.pid);
        self.runs.iter().find(|run| run.active() && (run.hwnd == hwnd || pid == Some(run.pid)))
    }

    fn client_choice(&self, client: &ClientWindow) -> String {
        let account = shown_account(self.anonymous, &client.account());
        if let Some(run) = self.run_on_client(client.hwnd) {
            let state = if run.stopping() {
                "stopping"
            } else if run.paused.load(Ordering::Relaxed) {
                "paused"
            } else {
                "running"
            };
            format!("{account}  ·  {} ({state})", run.name)
        } else if self.anonymous && is_player_name(&client.account()) {
            "Hidden".to_string()
        } else {
            client.label()
        }
    }

    fn running_panel(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Running").strong());
        let alive: Vec<&Running> = self.runs.iter().filter(|run| run.active()).collect();
        if alive.is_empty() {
            ui.label(RichText::new("No scripts running.").weak());
            return;
        }
        for run in alive {
            let paused = run.paused.load(Ordering::Relaxed);
            Frame::new()
                .fill(chrome::panel_fill(ui.visuals()))
                .corner_radius(6.0)
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(shown_account(self.anonymous, &run.account)).strong().size(16.0));
                            ui.label(RichText::new(&run.name).size(13.0));
                            if !run.activity.is_empty() && !run.activity.eq_ignore_ascii_case("running") {
                                ui.label(RichText::new(&run.activity).size(13.0).color(Color32::from_rgb(232, 196, 96)));
                            }
                            let session = run.started.elapsed().as_secs();
                            let pending = session.saturating_sub(run.booked);
                            let saved = self.play_seconds.get(run.account.trim()).copied().unwrap_or(0);
                            ui.label(
                                RichText::new(format!("Session {}", format_clock(session)))
                                    .size(12.0)
                                    .weak(),
                            );
                            ui.label(
                                RichText::new(format!("All time {}", format_clock(saved + pending)))
                                    .size(12.0)
                                    .weak(),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let stopping = run.stopping();
                            if stopping_button(ui, [88.0, 30.0], stopping) {
                                run.stop.store(true, Ordering::Relaxed);
                            }
                            let pause_label = if paused { "Resume" } else { "Pause" };
                            if held_button(ui, [88.0, 30.0], pause_label, !stopping) {
                                let next = if paused { CTRL_RESUME } else { CTRL_PAUSE };
                                run.ctrl.store(next, Ordering::Relaxed);
                            }
                            let (state, color) = if stopping {
                                ("Stopping", Color32::from_rgb(168, 168, 176))
                            } else if paused {
                                ("Paused", Color32::from_rgb(230, 176, 72))
                            } else {
                                ("Running", Color32::from_rgb(126, 196, 132))
                            };
                            ui.label(RichText::new(state).color(color).strong());
                        });
                    });
                    if run.output_hidden.load(Ordering::Relaxed) {
                        if ui
                            .add_sized(
                                [ui.available_width(), 28.0],
                                Button::new("Show output"),
                            )
                            .on_hover_text("Show the output window beside the RuneLite client.")
                            .clicked()
                        {
                            run.output_hidden.store(false, Ordering::Relaxed);
                        }
                    }
                });
            ui.add_space(4.0);
        }
    }

    pub fn tick(&mut self) {
        if self.last_client_scan.map(|t| t.elapsed() > Duration::from_secs(2)).unwrap_or(true) {
            self.refresh_clients();
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                NetMsg::Catalog { gen, result } => {
                    if gen != self.fetch_gen {
                        continue;
                    }
                    self.loading = false;
                    match result {
                        Ok((remote, license_status)) => {
                            self.remote = remote;
                            self.license_status = license_status;
                            self.rebuild_scripts();
                        }
                        Err(e) => {
                            self.license_status = e.clone();
                            self.status = e;
                        }
                    }
                }
                NetMsg::Download { id, result } => {
                    self.loading = false;
                    let start_after = self.run_after_update.as_deref() == Some(id.as_str());
                    if start_after {
                        self.run_after_update = None;
                    }
                    match result {
                        Ok(path) => {
                            self.status = format!("Installed {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                            self.rebuild_scripts();
                            if let Some(index) = self.scripts.iter().position(|s| s.id == id) {
                                self.selected = Some(index);
                            }
                            if start_after {
                                if let Some(script) = self.scripts.iter().find(|s| s.id == id).cloned() {
                                    self.run(&script);
                                }
                            }
                        }
                        Err(e) => self.status = e,
                    }
                }
                NetMsg::Releases { name, result } => {
                    self.releases_done = self.releases_done.saturating_add(1);
                    match result {
                        Ok(list) => {
                            self.releases.insert(name, list);
                        }
                        Err(error) => self.release_error = error,
                    }
                    if self.releases_done >= 2 {
                        self.releases_loading = false;
                    }
                }
                NetMsg::LibInstalled { name, result } => {
                    if self.installing_lib == name {
                        self.installing_lib.clear();
                    }
                    self.refresh_library_status();
                    self.status = match result {
                        Ok(label) => format!("Installed {name} {label}."),
                        Err(error) => error,
                    };
                }
                NetMsg::Discord(result) => {
                    self.discord_pending = false;
                    match result {
                        catalog::DiscordLogin::Ready { key, username } => {
                            self.license_key = key;
                            if let Err(error) = catalog::save_config(
                                &self.paths.launcher_config(),
                                &self.license_key,
                                &self.install_id,
                            ) {
                                self.status = error;
                            } else {
                                self.reload_catalog();
                                self.remember_discord_user(&username);
                                self.status = if username.is_empty() {
                                    "Logged in with Discord. License saved.".into()
                                } else {
                                    format!("Logged in as {username}. License saved.")
                                };
                            }
                        }
                        catalog::DiscordLogin::NoLicense { username } => {
                            self.remember_discord_user(&username);
                            self.status = if username.is_empty() {
                                "That Discord account has no license key yet.".into()
                            } else {
                                format!("{username} is logged in, but that Discord account has no license key yet.")
                            };
                        }
                        catalog::DiscordLogin::Cancelled => {
                            self.status = "Discord login was cancelled.".into();
                        }
                        catalog::DiscordLogin::TimedOut => {
                            self.status = "Discord login timed out. Try again.".into();
                        }
                        catalog::DiscordLogin::Failed(error) => self.status = error,
                    }
                }
                NetMsg::Changelog { gen, result } => {
                    if gen != self.changelog_gen {
                        continue;
                    }
                    self.changelog_loading = false;
                    match result {
                        Ok(entries) => self.changelog_entries = entries,
                        Err(error) => self.changelog_error = error,
                    }
                }
            }
        }
        self.poll_runs();
        self.poll_library_check();
    }

    /// The startup check already ran. While the window stays open, look again each hour.
    fn poll_library_check(&mut self) {
        let finished = self.library_check_rx.as_ref().map(|rx| rx.try_recv());
        if matches!(finished, Some(Ok(())) | Some(Err(mpsc::TryRecvError::Disconnected))) {
            self.library_check_rx = None;
            self.refresh_library_status();
        }
        let due = self
            .last_library_check
            .map(|at| at.elapsed() >= LIBRARY_CHECK_INTERVAL)
            .unwrap_or(false);
        if !due || self.library_check_rx.is_some() {
            return;
        }
        self.last_library_check = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        self.library_check_rx = Some(rx);
        let paths = self.paths.clone();
        thread::spawn(move || {
            versions::refresh_library_record(&paths);
            let _ = tx.send(());
        });
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        chrome::apply_theme(ctx, self.theme);
        let filter = self.filter.to_lowercase();
        let mut visible: Vec<usize> = self
            .scripts
            .iter()
            .enumerate()
            .filter(|(_, script)| {
                script_in_category(script, &self.category, &self.skill)
                    && (filter.is_empty()
                        || script.name.to_lowercase().contains(&filter)
                        || script.kind.to_lowercase().contains(&filter))
            })
            .map(|(index, _)| index)
            .collect();
        visible.sort_by(|&a, &b| {
            let a_fav = self.is_favorite(&self.scripts[a].id);
            let b_fav = self.is_favorite(&self.scripts[b].id);
            b_fav.cmp(&a_fav)
        });

        chrome::install_smooth_drag();
        if self.logo.is_none() {
            self.logo = chrome::load_logo(ctx);
        }
        if !self.icons_loaded {
            self.icons = crate::icons::load(ctx);
            self.icons_loaded = true;
        }

        egui::TopBottomPanel::top("bash_header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                chrome::show_logo(ui, &self.logo, 52.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("BASH Launcher 2.0").strong().size(18.0));
                    let (version, version_color) = self.version_label();
                    ui.label(RichText::new(version).color(version_color).size(12.0));
                });
                ui.vertical(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.script_running(), Button::new("Update libraries"))
                            .on_hover_text("Check Simba, SRL-B and BashLib for updates")
                            .clicked()
                        {
                            self.recheck_requested = true;
                        }
                        if ui.button("Settings").on_hover_text("Output, remote input, folders, repair, and library versions").clicked() {
                            self.settings_open = true;
                        }
                        ui.separator();
                        if text_link(ui, "Discord").on_hover_text("Join the BASH Discord").clicked() {
                            let _ = chrome::open_url(DISCORD_URL);
                        }
                        if text_link(ui, "BASH Store").on_hover_text("Open the BASH store").clicked() {
                            let _ = chrome::open_url(STORE_URL);
                        }
                    });
                    if let Some((text, color)) = library_status_label(&self.library_status) {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = ui.label(RichText::new(text).color(color).size(12.0));
                            if !self.library_detail.is_empty() {
                                label.on_hover_text(&self.library_detail);
                            }
                        });
                    }
                });
            });
            if self.script_running() {
                ui.label(RichText::new("Stop the running script before updating libraries.").weak());
            }
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Client");
                let selected_text = self
                    .clients
                    .iter()
                    .find(|client| Some(client.hwnd) == self.selected_hwnd)
                    .map(|client| self.client_choice(client))
                    .unwrap_or_else(|| "Select your RuneLite client here".to_string());
                egui::ComboBox::from_id_salt("client")
                    .selected_text(selected_text)
                    .width(520.0)
                    .show_ui(ui, |ui| {
                        if self.clients.is_empty() {
                            ui.label("Open RuneLite, then press Refresh.");
                        }
                        for client in &self.clients {
                            let label = self.client_choice(client);
                            ui.selectable_value(&mut self.selected_hwnd, Some(client.hwnd), label);
                        }
                    });
                if ui.button("Refresh").clicked() {
                    self.refresh_clients();
                }
                if ui
                    .button("Install RuneLite profile")
                    .on_hover_text("Writes the BASH RuneLite profile and selects it. RuneLite has to be closed first.")
                    .clicked()
                {
                    self.install_runelite_profile();
                }
                ui.separator();
                if ui
                    .button("Accounts")
                    .on_hover_text("Account, antiban, webhooks, farm, and birdhouse. These apply to every script.")
                    .clicked()
                {
                    self.open_profile(scriptprofile::Page::Accounts);
                }
            });
            ui.add_space(2.0);
        });

        egui::TopBottomPanel::bottom("bash_footer").show(ctx, |ui| {
            ui.add_space(2.0);
            let accepted = license_accepted(&self.license_status);
            let dark = ui.visuals().dark_mode;
            let (fill, stroke, label_color) = if accepted {
                if dark {
                    (
                        Color32::from_rgb(32, 33, 36),
                        Stroke::new(1.0_f32, Color32::from_rgb(55, 58, 64)),
                        Color32::from_rgb(168, 172, 178),
                    )
                } else {
                    (
                        Color32::from_rgb(236, 237, 240),
                        Stroke::new(1.0_f32, Color32::from_rgb(190, 194, 200)),
                        Color32::from_rgb(55, 60, 68),
                    )
                }
            } else {
                (
                    Color32::from_rgb(38, 34, 24),
                    Stroke::new(1.0_f32, Color32::from_rgb(176, 138, 48)),
                    Color32::from_rgb(232, 196, 96),
                )
            };
            Frame::new()
                .fill(fill)
                .stroke(stroke)
                .corner_radius(6.0)
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("License key").strong().color(label_color));
                        let reveal = self.show_license;
                        let mut edit = TextEdit::singleline(&mut self.license_key)
                            .desired_width(340.0)
                            .hint_text("Paste your license key here");
                        if accepted {
                            edit = edit
                                .password(!reveal)
                                .margin(Margin { left: 4, right: 22, top: 2, bottom: 2 });
                        }
                        let edit = ui.add(edit);
                        if accepted {
                            let eye_rect = egui::Rect::from_center_size(
                                egui::pos2(edit.rect.right() + 14.0, edit.rect.center().y),
                                egui::vec2(14.0, edit.rect.height() + 4.0),
                            );
                            if eye_button(ui, eye_rect, reveal)
                                .on_hover_text(if reveal { "Hide the license key" } else { "Show the license key" })
                                .clicked()
                            {
                                self.show_license = !reveal;
                            }
                        }
                        let save = ui.button("Save").clicked();
                        if ui
                            .add_enabled(!self.discord_pending, Button::new(if self.discord_pending { "Waiting for Discord..." } else { "Log in with Discord" }))
                            .on_hover_text("Open Discord in your browser. If that account has a license, the key is filled in here.")
                            .clicked()
                        {
                            self.start_discord_login();
                        }
                        let submitted = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if save || submitted {
                            self.save_license();
                            self.reload_catalog();
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.license_status.starts_with("No license key saved") {
                                let color = license_color(&self.license_status);
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(RichText::new(".").color(color));
                                if text_link(ui, "Discord").on_hover_text("Join the BASH Discord").clicked() {
                                    let _ = chrome::open_url(DISCORD_URL);
                                }
                                ui.add_space(4.0);
                                ui.label(RichText::new("or").color(color));
                                ui.add_space(4.0);
                                if text_link(ui, "BASH Store").on_hover_text("Open the BASH store").clicked() {
                                    let _ = chrome::open_url(STORE_URL);
                                }
                                ui.add_space(4.0);
                                ui.label(RichText::new("No license key saved. Purchase from the").color(color));
                            } else {
                                ui.label(RichText::new(&self.license_status).color(license_color(&self.license_status)));
                            }
                        });
                    });
                });
            if self.status.is_empty() {
                return;
            }
            let color = if self.status.to_lowercase().contains("fail")
                || self.status.to_lowercase().contains("could not")
                || self.status.to_lowercase().contains("needs")
            {
                Color32::from_rgb(230, 120, 90)
            } else {
                Color32::from_rgb(180, 190, 170)
            };
            ui.label(RichText::new(&self.status).color(color));
        });

        egui::SidePanel::left("bash_scripts")
            .resizable(true)
            .default_width(500.0)
            .min_width(340.0)
            .max_width(760.0)
            .show(ctx, |ui| {
            ui.label(RichText::new("Category").weak());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (id, label) in CATEGORIES {
                    if ui.selectable_label(self.category == *id, *label).clicked() {
                        self.category = (*id).to_string();
                        self.skill.clear();
                    }
                }
            });
            if let Some(skills) = skills_in(&self.category) {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if ui.selectable_label(self.skill.is_empty(), "All").clicked() {
                        self.skill.clear();
                    }
                    for skill in skills {
                        let picked = if let Some(icon) = self.icons.get(skill.id) {
                            skill_icon_button(ui, icon, self.skill == skill.id, skill.label)
                        } else {
                            ui.selectable_label(self.skill == skill.id, skill.label).clicked()
                        };
                        if picked {
                            self.skill = skill.id.to_string();
                        }
                    }
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Scripts");
                ui.add(TextEdit::singleline(&mut self.filter).desired_width(180.0).hint_text("filter"));
                if self.loading {
                    ui.spinner();
                }
            });
            ui.add_space(4.0);
            ScrollArea::vertical().id_salt("script_list").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if visible.is_empty() {
                    ui.label("No scripts.");
                }
                let mut clicked = None;
                let mut starred = None;
                for index in &visible {
                    let script = &self.scripts[*index];
                    let activity = row_activity(script, &self.skill);
                    let icon = activity.and_then(|id| self.icons.get(id));
                    let favorite = self.is_favorite(&script.id);
                    let (response, star) = script_row(
                        ui,
                        script,
                        self.selected == Some(*index),
                        icon,
                        activity,
                        favorite,
                        &self.icons,
                    );
                    if star {
                        starred = Some(script.id.clone());
                    } else if response.clicked() {
                        clicked = Some(*index);
                    }
                }
                if let Some(id) = starred {
                    self.toggle_favorite(&id);
                }
                if let Some(index) = clicked {
                    self.selected = Some(index);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.running_panel(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            let selected = self.selected.and_then(|index| self.scripts.get(index).cloned());
            if let Some(script) = &selected {
                let activity = row_activity(script, &self.skill);
                let icon = activity.and_then(|id| self.icons.get(id)).cloned();
                ui.horizontal(|ui| {
                    if let Some(icon) = &icon {
                        ui.add(Image::new(icon).fit_to_exact_size(egui::vec2(28.0, 28.0)));
                    }
                    ui.label(RichText::new(&script.name).strong().size(20.0));
                    if script.for_sale() {
                        sale_badge(ui, script.discount_pct);
                    }
                    runs_icons(ui, script, &self.icons, 18.0);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    kind_badge(ui, &script.kind);
                    price_labels(ui, script, 15.0);
                    let (state, color) = state_style(script, ui.visuals().dark_mode);
                    ui.label(RichText::new(state).color(color).strong());
                    if script.kind == "premium" && !script.entitled && !script.installed() {
                        if text_link(ui, "Click here to purchase").clicked() {
                            self.open_website(script);
                        }
                    }
                });
                ui.add_space(8.0);
                ScrollArea::vertical().id_salt("description").max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    if let Some(runs) = script.included_runs() {
                        ui.label(RichText::new(runs).color(Color32::from_rgb(176, 196, 170)));
                        ui.add_space(6.0);
                    }
                    ui.label(&script.description);
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let installed = script.installed();
                    let active = self.selected_hwnd.and_then(|hwnd| {
                        self.run_on_client(hwnd).map(|run| (run.name.clone(), run.stopping()))
                    });
                    let this_running = active.as_ref().is_some_and(|(name, _)| name == &script.name);
                    let stopping = this_running && active.as_ref().is_some_and(|(_, stopping)| *stopping);
                    let other_running = active.is_some() && !this_running;
                    let label = if stopping {
                        "Stopping"
                    } else if this_running {
                        "Stop"
                    } else {
                        "Run"
                    };
                    let button = if stopping {
                        Button::new(label)
                    } else if this_running {
                        Button::new(label).fill(Color32::from_rgb(148, 56, 52))
                    } else {
                        Button::new(label).fill(Color32::from_rgb(46, 118, 68))
                    };
                    let enabled = if stopping {
                        false
                    } else if this_running {
                        true
                    } else {
                        installed && self.selected_hwnd.is_some() && !other_running
                    };
                    let mut run_btn = ui.add_enabled(enabled, button);
                    if stopping {
                        run_btn = run_btn.on_disabled_hover_text("Waiting for the script to stop.");
                    } else if other_running {
                        run_btn = run_btn.on_disabled_hover_text("This client already has a script running.");
                    }
                    if run_btn.clicked() {
                        if this_running {
                            if let Some(hwnd) = self.selected_hwnd {
                                self.stop_client(hwnd);
                            }
                        } else if self.run_after_update.as_deref() == Some(script.id.as_str()) {
                            self.status = format!("Updating {}...", script.name);
                        } else if script.update_available() {
                            self.prompt_script_update(script);
                        } else {
                            self.run(script);
                        }
                    }
                    let install_label = if script.update_available() {
                        "Update"
                    } else if installed {
                        "Reinstall"
                    } else {
                        "Install"
                    };
                    if ui
                        .add_enabled(!self.loading && !script.id.starts_with("local:"), Button::new(install_label))
                        .clicked()
                    {
                        self.install(script);
                    }
                    if ui.add_enabled(installed, Button::new("Open in editor")).clicked() {
                        if let Some(path) = &script.local_path {
                            self.open_in_editor(path);
                        }
                    }
                    if ui
                        .add_enabled(!script.id.starts_with("local:"), Button::new("View page"))
                        .on_hover_text("Open this script's page on bashscripts.shop")
                        .clicked()
                    {
                        self.open_website(script);
                    }
                    let has_changelog = !script.id.starts_with("local:");
                    if ui
                        .add_enabled(has_changelog, Button::new("Changelog"))
                        .on_hover_text("What changed in this script")
                        .on_disabled_hover_text("This copy is not in the catalog, so it has no changelog.")
                        .clicked()
                    {
                        self.open_changelog(script);
                    }
                });
            } else {
                ui.label("Select a script.");
            }
        });

        self.profile_window(ctx);
        self.profile_prompt_window(ctx);
        self.discord_prompt_window(ctx);
        self.changelog_window(ctx);
        self.settings_window(ctx);
        self.repair_window(ctx);
    }

    pub fn output_windows(&mut self, ctx: &egui::Context) {
        let pixels_per_point = ctx.pixels_per_point();
        let anonymous = self.anonymous;
        let names = account_names(&self.clients, &self.runs);
        for run in &mut self.runs {
            if run.output_hidden.load(Ordering::Relaxed) {
                // The window is gone, so the next show has to place it again.
                run.anchor.client = None;
                run.anchor.output = None;
                run.anchor.wait = None;
                continue;
            }
            let move_to = if self.anchor_output {
                let client = client_window_rect(run.hwnd);
                run.anchor.next_move(client.as_ref(), pixels_per_point)
            } else {
                run.anchor.client = None;
                run.anchor.wait = None;
                None
            };
            let title = format!("{} — {}", shown_account(anonymous, &run.account), run.name);
            let account = shown_account(anonymous, &run.account);
            let script_name = run.name.clone();
            let run_id = run.id;
            let copy_body = conceal_text(anonymous, &run.log_text, &names);
            let text = if run.log_text.is_empty() {
                "Please wait, script starting...".to_string()
            } else {
                copy_body.clone()
            };
            let stop = run.stop.clone();
            let ctrl = run.ctrl.clone();
            let paused = run.paused.clone();
            let dismiss = run.dismiss.clone();
            let hidden = run.output_hidden.clone();
            let finished = !run.active();
            let seen_x = Arc::new(AtomicI32::new(i32::MIN));
            let seen_y = Arc::new(AtomicI32::new(i32::MIN));
            let report_x = Arc::clone(&seen_x);
            let report_y = Arc::clone(&seen_y);
            let mut builder = egui::ViewportBuilder::default()
                .with_title(&title)
                .with_inner_size([760.0, 480.0]);
            if let Some((x, y)) = move_to {
                let scale = pixels_per_point.max(1.0);
                builder = builder.with_position(egui::pos2(x as f32 / scale, y as f32 / scale));
            }
            let theme = self.theme;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("bash-output", run.id)),
                builder,
                move |ctx, _class| {
                    chrome::apply_theme(ctx, theme);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        if finished {
                            dismiss.store(true, Ordering::Relaxed);
                        } else {
                            hidden.store(true, Ordering::Relaxed);
                            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        }
                    }
                    let scale = ctx.pixels_per_point().max(1.0);
                    if let Some((x, y)) = move_to {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                            x as f32 / scale,
                            y as f32 / scale,
                        )));
                    }
                    if let Some(rect) = ctx.input(|input| input.viewport().outer_rect) {
                        report_x.store((rect.min.x * scale).round() as i32, Ordering::Relaxed);
                        report_y.store((rect.min.y * scale).round() as i32, Ordering::Relaxed);
                    }
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading(&account);
                                ui.label(RichText::new(&script_name).size(16.0));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if finished {
                                    if ui.add_sized([120.0, 36.0], Button::new("Close")).clicked() {
                                        dismiss.store(true, Ordering::Relaxed);
                                    }
                                } else {
                                    let stopping = stop.load(Ordering::Relaxed);
                                    if stopping_button(ui, [110.0, 36.0], stopping) {
                                        stop.store(true, Ordering::Relaxed);
                                    }
                                    let pause_label = if paused.load(Ordering::Relaxed) { "Resume" } else { "Pause" };
                                    if held_button(ui, [110.0, 36.0], pause_label, !stopping) {
                                        let next = if paused.load(Ordering::Relaxed) { CTRL_RESUME } else { CTRL_PAUSE };
                                        ctrl.store(next, Ordering::Relaxed);
                                    }
                                    if ui
                                        .add_sized([90.0, 36.0], Button::new("Hide"))
                                        .on_hover_text("Hide this window. The script keeps running.")
                                        .clicked()
                                    {
                                        hidden.store(true, Ordering::Relaxed);
                                    }
                                }
                                let copied_id = egui::Id::new(("output-copied", run_id));
                                let now = ui.input(|input| input.time);
                                let copied = ui.ctx().data(|data| {
                                    data.get_temp::<f64>(copied_id).is_some_and(|at| now - at < 2.0)
                                });
                                let copy_label = if copied { "Copied" } else { "Copy logs" };
                                if ui
                                    .add_sized([120.0, 36.0], Button::new(copy_label))
                                    .on_hover_text("Copy this output to the clipboard.")
                                    .clicked()
                                {
                                    let body = if copy_body.is_empty() {
                                        format!("{account}\n{script_name}\n\n(no output yet)")
                                    } else {
                                        format!("{account}\n{script_name}\n\n{copy_body}")
                                    };
                                    ui.ctx().copy_text(body);
                                    ui.ctx().data_mut(|data| data.insert_temp(copied_id, now));
                                }
                            });
                        });
                        if stop.load(Ordering::Relaxed) && !finished {
                            ui.label(RichText::new("Stopping").color(Color32::from_rgb(168, 168, 176)).strong());
                        } else if paused.load(Ordering::Relaxed) && !finished {
                            ui.label(RichText::new("Paused").color(Color32::from_rgb(230, 176, 72)).strong());
                        }
                        ui.separator();
                        ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                            ui.label(RichText::new(&text).monospace().size(13.0));
                        });
                    });
                },
            );
            let reported_x = seen_x.load(Ordering::Relaxed);
            if reported_x != i32::MIN {
                run.anchor.output = Some((reported_x, seen_y.load(Ordering::Relaxed)));
            }
        }
    }

    fn refresh_clients(&mut self) {
        self.clients = clients::game_clients();
        self.last_client_scan = Some(Instant::now());
        if let Some(hwnd) = self.selected_hwnd {
            if !self.clients.iter().any(|c| c.hwnd == hwnd) {
                self.selected_hwnd = None;
            }
        }
        if self.selected_hwnd.is_none() && self.clients.len() == 1 {
            self.selected_hwnd = Some(self.clients[0].hwnd);
        }
        let mut switched = Vec::new();
        for (index, run) in self.runs.iter_mut().enumerate() {
            if run.child.is_none() {
                continue;
            }
            if let Some(client) = self.clients.iter().find(|client| client.hwnd == run.hwnd || client.pid == run.pid) {
                let account = client.account();
                if account != "Not logged in" && account != run.account {
                    switched.push((index, account));
                }
            }
        }
        let mut booked = false;
        for (index, account) in switched {
            booked |= self.book_run_time(index, false);
            self.runs[index].account = account;
        }
        if booked {
            self.save_play_time();
        }
    }

    /// Add this session's new seconds onto the account's all-time total.
    fn book_run_time(&mut self, index: usize, allow_unknown: bool) -> bool {
        let Some(run) = self.runs.get(index) else {
            return false;
        };
        let account = run.account.trim();
        if account.is_empty() || (!allow_unknown && account == "Not logged in") {
            return false;
        }
        let session = run.started.elapsed().as_secs();
        let extra = session.saturating_sub(run.booked);
        if extra == 0 {
            return false;
        }
        let account = account.to_string();
        *self.play_seconds.entry(account).or_default() += extra;
        self.runs[index].booked = session;
        true
    }

    fn save_play_time(&self) {
        let path = self.paths.state_file();
        let mut state = crate::state::BashState::load(&path);
        state.play_seconds = self.play_seconds.clone();
        let _ = state.save(&path);
    }

    fn reload_catalog(&mut self) {
        self.loading = true;
        self.fetch_gen += 1;
        let gen = self.fetch_gen;
        let key = self.license_key.clone();
        let install_id = self.install_id.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = catalog::fetch_catalog(&key, &install_id).map(|snap| (snap.scripts, snap.license_status));
            let _ = tx.send(NetMsg::Catalog { gen, result });
        });
    }

    fn save_license(&mut self) {
        match catalog::save_config(&self.paths.launcher_config(), &self.license_key, &self.install_id) {
            Ok(()) => self.status = "License saved.".into(),
            Err(e) => self.status = e,
        }
    }

    fn start_discord_login(&mut self) {
        if self.discord_pending {
            return;
        }
        self.discord_pending = true;
        self.status = "Opening Discord in your browser...".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = catalog::discord_login();
            let _ = tx.send(NetMsg::Discord(result));
        });
    }

    fn rebuild_scripts(&mut self) {
        let selected_id = self.selected.and_then(|i| self.scripts.get(i).map(|s| s.id.clone()));
        self.scripts = catalog::with_local(&self.paths, &self.remote);
        self.selected = selected_id.and_then(|id| self.scripts.iter().position(|s| s.id == id));
    }

    fn install(&mut self, script: &ScriptEntry) {
        self.save_license();
        let Some(current) = self.scripts.iter().find(|s| s.id == script.id).cloned() else {
            if self.run_after_update.as_deref() == Some(script.id.as_str()) {
                self.run_after_update = None;
            }
            return;
        };
        self.loading = true;
        self.status = format!("Installing {}...", current.name);
        let paths = self.paths.clone();
        let key = self.license_key.clone();
        let install_id = self.install_id.clone();
        let id = current.id.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = catalog::download_script(&current, &paths, &key, &install_id);
            let _ = tx.send(NetMsg::Download { id, result });
        });
    }

    fn open_profile(&mut self, page: scriptprofile::Page) {
        self.refresh_clients();
        let client = self.logged_in_client();
        self.profile_editor = Some(scriptprofile::Editor::for_page(&self.paths, client.as_ref(), page));
    }

    /// The client to read an account name from. The chosen client wins. Otherwise
    /// one logged-in window, or the only open client, is enough.
    fn logged_in_client(&self) -> Option<ClientWindow> {
        if let Some(hwnd) = self.selected_hwnd {
            if let Some(client) = self.clients.iter().find(|client| client.hwnd == hwnd) {
                return Some(client.clone());
            }
        }
        let mut logged_in = self
            .clients
            .iter()
            .filter(|client| agility::account_key(&client.account()) != "NoUserNameSelected")
            .cloned();
        let first = logged_in.next();
        let second = logged_in.next();
        if second.is_none() {
            if let Some(client) = first {
                return Some(client);
            }
        }
        if self.clients.len() == 1 {
            return Some(self.clients[0].clone());
        }
        None
    }

    fn profile_window(&mut self, ctx: &egui::Context) {
        let paths = self.paths.clone();
        if let Some(client) = self.logged_in_client() {
            let name = client.account();
            if let Some(editor) = self.profile_editor.as_mut() {
                editor.note_logged_in(&paths, &name);
            }
        }
        let event = {
            let Some(editor) = self.profile_editor.as_mut() else {
                return;
            };
            editor.client_hwnd = self.selected_hwnd;
            scriptprofile::show(ctx, &paths, editor)
        };
        match event {
            scriptprofile::EditorEvent::None => {}
            scriptprofile::EditorEvent::Close => {
                self.profile_editor = None;
            }
            scriptprofile::EditorEvent::CloseSaved => {
                self.status = "Settings saved.".into();
                self.profile_editor = None;
            }
            scriptprofile::EditorEvent::Start => {
                let running = self.profile_editor.as_ref().is_some_and(|editor| editor.run_script.is_some());
                if running {
                    if let Some(editor) = self.profile_editor.as_ref() {
                        if let Err(error) = editor.validate_course() {
                            self.status = error.clone();
                            if let Some(editor) = self.profile_editor.as_mut() {
                                editor.error = error;
                            }
                            return;
                        }
                        if let Err(error) = scriptprofile::worlds_ready(editor) {
                            self.status = error.clone();
                            if let Some(editor) = self.profile_editor.as_mut() {
                                editor.error = error;
                            }
                            return;
                        }
                    }
                }
                let Some(mut editor) = self.profile_editor.take() else {
                    return;
                };
                if let Err(error) = editor.save(&self.paths) {
                    self.status = error.clone();
                    editor.error = error;
                    self.profile_editor = Some(editor);
                    return;
                }
                if running {
                    self.agility_account = Some(editor.account_key());
                    if let Some(script) = editor.run_script.clone() {
                        self.run(&script);
                    }
                } else {
                    self.status = "Settings saved.".into();
                }
            }
        }
    }

    /// Yes updates the script, then runs it. No runs the copy already installed.
    fn prompt_script_update(&mut self, script: &ScriptEntry) {
        let update = chrome::ask_yes_no(&format!(
            "An update is available for {}.\n\nDo you want to update before running?\n\nYes downloads the update, then runs it. No runs the installed copy.",
            script.name
        ));
        if update {
            self.run_after_update = Some(script.id.clone());
            self.install(script);
        } else {
            self.run(script);
        }
    }

    fn run(&mut self, script: &ScriptEntry) {
        if script.local_path.is_none() {
            self.status = "Install the script first.".into();
            return;
        }
        let Some(hwnd) = self.selected_hwnd else {
            self.status = "Open a client and select it first.".into();
            return;
        };
        if self.run_on_client(hwnd).is_some() {
            self.status = "That client already has a script. Stop it before starting another.".into();
            return;
        }
        let Some(client) = self.clients.iter().find(|client| client.hwnd == hwnd).cloned() else {
            self.status = "That client is no longer open. Press Refresh.".into();
            return;
        };
        if !self.paths.simba_exe().is_file() {
            self.status = format!("Simba was not found at {}.", self.paths.simba_exe().display());
            return;
        }
        let Some(script) = self.first_run_choice(script) else {
            return;
        };
        let Some(path) = script.local_path.clone() else {
            self.status = "Install the script first.".into();
            return;
        };
        let opens_setup = agility::is_aio_agility(&script) || crate::scriptsetup::supports(&script.name);
        if opens_setup && self.agility_account.is_none() {
            if self.profile_editor.is_none() {
                self.profile_editor = Some(scriptprofile::Editor::for_run(&self.paths, &script, &client));
            }
            self.status = format!("Set up {}, then press Start.", script.name);
            return;
        }
        let agility_account = self.agility_account.take();

        let log_dir = self.paths.run_logs();
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            self.status = format!("Could not create the log folder: {e}");
            return;
        }
        let id = self.next_run;
        self.next_run += 1;
        let log_path = log_dir.join(format!("{id}.log"));
        let _ = std::fs::remove_file(&log_path);

        // Simba parses --target with a 32-bit StrToInt, which matches a Win32 window handle.
        let handle = hwnd as i32;
        // Simba's --log file is opened so nothing else can read it, and the whole
        // log only appears after the process exits. Read the pipes instead.
        // Script WriteLn already flushes, so lines show up as they are printed.
        let mut command = Command::new(self.paths.simba_exe());
        command
            .current_dir(&self.paths.root)
            .arg(format!("--target={handle}"))
            .arg("--run")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(account) = &agility_account {
            command.env("BASH_LAUNCHER", "1");
            if account != "NoUserNameSelected" {
                command.env("BASH_ACCOUNT", account);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        match command.spawn() {
            Ok(mut child) => {
                let stdin = child.stdin.take();
                let incoming = Arc::new(Mutex::new(Vec::new()));
                let output_left = Arc::new(AtomicU8::new(0));
                if let Some(out) = child.stdout.take() {
                    output_left.fetch_add(1, Ordering::Relaxed);
                    spawn_output_reader(out, Arc::clone(&incoming), Arc::clone(&output_left));
                }
                if let Some(err) = child.stderr.take() {
                    output_left.fetch_add(1, Ordering::Relaxed);
                    spawn_output_reader(err, Arc::clone(&incoming), Arc::clone(&output_left));
                }
                log_line(&format!(
                    "run {} pid {} target {handle} account {}",
                    script.name,
                    child.id(),
                    shown_account(self.anonymous, &client.account())
                ));
                self.status = format!("Running {} on {}.", script.name, shown_account(self.anonymous, &client.account()));
                self.runs.push(Running {
                    id,
                    name: script.name.clone(),
                    log_path,
                    log_text: String::new(),
                    incoming,
                    output_left,
                    child: Some(child),
                    stdin,
                    hwnd: client.hwnd,
                    pid: client.pid,
                    account: client.account(),
                    ctrl: Arc::new(AtomicU8::new(0)),
                    paused: Arc::new(AtomicBool::new(false)),
                    stop: Arc::new(AtomicBool::new(false)),
                    stop_sent: false,
                    stop_at: None,
                    dismiss: Arc::new(AtomicBool::new(false)),
                    dismiss_after: Arc::new(AtomicBool::new(false)),
                    exit_note: None,
                    note_shown: false,
                    started: Instant::now(),
                    booked: 0,
                    activity: String::new(),
                    line_buf: String::new(),
                    output_hidden: Arc::new(AtomicBool::new(false)),
                    anchor: OutputAnchor::default(),
                    release: None,
                    release_at: None,
                });
            }
            Err(e) => self.status = format!("Could not start Simba: {e}"),
        }
    }

    fn stop_client(&mut self, hwnd: isize) {
        let pid = self.clients.iter().find(|client| client.hwnd == hwnd).map(|client| client.pid);
        for run in &self.runs {
            if run.active() && (run.hwnd == hwnd || pid == Some(run.pid)) {
                run.stop.store(true, Ordering::Relaxed);
            }
        }
    }

    fn open_website(&mut self, script: &ScriptEntry) {
        let slug = script_slug(&script.name);
        if slug.is_empty() {
            self.status = "This script has no website page.".into();
            return;
        }
        let url = format!("https://bashscripts.shop/scripts/{slug}/");
        if chrome::open_url(&url) {
            self.status = format!("Opened {url}");
        } else {
            self.status = "Could not open the browser.".into();
        }
    }

    fn open_in_editor(&mut self, path: &Path) {
        match Command::new(self.paths.simba_exe()).current_dir(&self.paths.root).arg("--open").arg(path).spawn() {
            Ok(_) => self.status = "Opened in the Simba editor.".into(),
            Err(e) => self.status = format!("Could not open the editor: {e}"),
        }
    }

    fn poll_runs(&mut self) {
        let mut book = Vec::new();
        let mut release_failed = false;
        let simba = self.paths.simba_exe();
        let root = self.paths.root.clone();
        for (index, run) in self.runs.iter_mut().enumerate() {
            drain_output(run);
            if !run.stopping() {
                let ctrl = run.ctrl.swap(0, Ordering::Relaxed);
                if ctrl == CTRL_PAUSE {
                    if let Some(stdin) = run.stdin.as_mut() {
                        if send_simba_state(stdin, SIMBA_PAUSED) {
                            run.paused.store(true, Ordering::Relaxed);
                        }
                    }
                } else if ctrl == CTRL_RESUME {
                    if let Some(stdin) = run.stdin.as_mut() {
                        if send_simba_state(stdin, SIMBA_RUNNING) {
                            run.paused.store(false, Ordering::Relaxed);
                        }
                    }
                }
            } else {
                run.ctrl.store(0, Ordering::Relaxed);
            }

            let mut finished: Option<String> = None;
            let mut forced = false;
            if run.stopping() && run.child.is_some() {
                if !run.stop_sent {
                    if let Some(stdin) = run.stdin.as_mut() {
                        let _ = send_simba_state(stdin, SIMBA_STOP);
                    }
                    run.stop_sent = true;
                    run.stop_at = Some(Instant::now());
                    run.paused.store(false, Ordering::Relaxed);
                }
                let give_up = run.stop_at.map(|at| at.elapsed() > STOP_GRACE).unwrap_or(true);
                if let Some(child) = run.child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(_)) => finished = Some("Stopped.".into()),
                        Ok(None) if give_up => {
                            let _ = child.kill();
                            let _ = child.wait();
                            forced = true;
                            finished = Some("Stopped.".into());
                        }
                        Ok(None) => {}
                        Err(e) => {
                            forced = true;
                            finished = Some(format!("Script exited ({e})."));
                        }
                    }
                }
            } else if let Some(child) = run.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        finished = Some(format!("Script exited (code {}).", status.code().unwrap_or(-1)));
                    }
                    Ok(None) => {}
                    Err(e) => finished = Some(format!("Script exited ({e}).")),
                }
            }
            let ended = finished.is_some();
            if ended {
                let hwnd = run.hwnd;
                run.child = None;
                run.stdin = None;
                run.paused.store(false, Ordering::Relaxed);
                run.output_hidden.store(false, Ordering::Relaxed);
                if forced {
                    run.release = spawn_input_release(&simba, &root, hwnd);
                    run.release_at = run.release.is_some().then(Instant::now);
                    if run.release.is_none() {
                        run.stop.store(false, Ordering::Relaxed);
                    }
                } else {
                    run.stop.store(false, Ordering::Relaxed);
                }
                if run.exit_note.is_none() {
                    run.exit_note = finished;
                }
            }
            if run.release.is_some() {
                let give_up = run.release_at.map(|at| at.elapsed() > RELEASE_GRACE).unwrap_or(true);
                let mut done = false;
                let mut failed = false;
                if let Some(child) = run.release.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            done = true;
                            failed = !status.success();
                        }
                        Ok(None) if give_up => {
                            let _ = child.kill();
                            let _ = child.wait();
                            done = true;
                            failed = true;
                        }
                        Ok(None) => {}
                        Err(_) => {
                            done = true;
                            failed = true;
                        }
                    }
                }
                if done {
                    run.release = None;
                    run.release_at = None;
                    run.stop.store(false, Ordering::Relaxed);
                    if failed {
                        release_failed = true;
                    }
                }
            }
            drain_output(run);
            if run.exit_note.is_some() && !run.note_shown && run.output_left.load(Ordering::Relaxed) == 0 {
                if let Some(note) = run.exit_note.clone() {
                    if !run.log_text.ends_with('\n') && !run.log_text.is_empty() {
                        run.log_text.push('\n');
                    }
                    run.log_text.push_str(&note);
                    run.log_text.push('\n');
                }
                run.note_shown = true;
            }
            if !run.active() && run.dismiss_after.load(Ordering::Relaxed) {
                run.dismiss.store(true, Ordering::Relaxed);
            }
            let due = run.started.elapsed().as_secs().saturating_sub(run.booked) >= 15;
            if ended {
                book.push((index, true));
            } else if run.child.is_some() && due {
                book.push((index, false));
            }
        }
        let mut booked = false;
        for (index, allow_unknown) in book {
            booked |= self.book_run_time(index, allow_unknown);
        }
        if booked {
            self.save_play_time();
        }
        if release_failed {
            self.status = "Stopped. If the client still ignores the mouse, close it and open it again.".into();
        }
        self.runs.retain(|run| {
            if run.release.is_some() {
                return true;
            }
            if run.dismiss.load(Ordering::Relaxed) {
                if run.child.is_some() {
                    return true;
                }
                return false;
            }
            true
        });
    }
}

fn stopping_button(ui: &mut egui::Ui, size: [f32; 2], stopping: bool) -> bool {
    ui.add_enabled_ui(!stopping, |ui| {
        ui.add_sized(size, Button::new(if stopping { "Stopping" } else { "Stop" }))
            .on_disabled_hover_text("Waiting for the script to stop.")
            .clicked()
    })
    .inner
}

fn held_button(ui: &mut egui::Ui, size: [f32; 2], label: &str, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| ui.add_sized(size, Button::new(label)).clicked()).inner
}

/// Pair the client again and turn real mouse and keyboard input back on.
/// Used when Simba was closed before its own cleanup could run.
fn spawn_input_release(simba: &Path, root: &Path, hwnd: isize) -> Option<Child> {
    let script = std::env::temp_dir().join("bash-release-input.simba");
    let body = "{$I SRL-B/osr.simba}\r\nbegin\r\n  try\r\n    RSClient.RemoteInput.Setup();\r\n    RSClient.RemoteInput.EnableRealInput();\r\n  except\r\n  end;\r\n  try\r\n    if RSClient.RemoteInput.IsSetup() then\r\n      RSClient.RemoteInput.Free();\r\n  except\r\n  end;\r\nend.\r\n";
    std::fs::write(&script, body).ok()?;
    let mut command = Command::new(simba);
    command
        .current_dir(root)
        .arg(format!("--target={}", hwnd as i32))
        .arg("--run")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().ok()
}

struct Skill {
    id: &'static str,
    label: &'static str,
}

const CATEGORIES: &[(&str, &str)] = &[
    ("all", "All"),
    ("combat", "Combat"),
    ("gathering", "Gathering"),
    ("production", "Production"),
    ("utility", "Utility"),
    ("other", "Other"),
];

const COMBAT: &[Skill] = &[
    Skill { id: "combat", label: "Combat" },
    Skill { id: "magic", label: "Magic" },
    Skill { id: "prayer", label: "Prayer" },
    Skill { id: "ranged", label: "Ranged" },
];
const GATHERING: &[Skill] = &[
    Skill { id: "farming", label: "Farming" },
    Skill { id: "fishing", label: "Fishing" },
    Skill { id: "hunter", label: "Hunter" },
    Skill { id: "mining", label: "Mining" },
    Skill { id: "woodcutting", label: "Woodcutting" },
];
const PRODUCTION: &[Skill] = &[
    Skill { id: "cooking", label: "Cooking" },
    Skill { id: "crafting", label: "Crafting" },
    Skill { id: "fletching", label: "Fletching" },
    Skill { id: "herblore", label: "Herblore" },
    Skill { id: "runecraft", label: "Runecraft" },
    Skill { id: "smithing", label: "Smithing" },
];
const UTILITY: &[Skill] = &[
    Skill { id: "utility", label: "Utility" },
    Skill { id: "agility", label: "Agility" },
    Skill { id: "construction", label: "Construction" },
    Skill { id: "firemaking", label: "Firemaking" },
    Skill { id: "sailing", label: "Sailing" },
    Skill { id: "slayer", label: "Slayer" },
    Skill { id: "thieving", label: "Thieving" },
];
const OTHER: &[Skill] = &[
    Skill { id: "minigame", label: "MiniGames" },
    Skill { id: "bossing", label: "Bossing" },
];

fn library_status_label(status: &str) -> Option<(&'static str, Color32)> {
    match status {
        "up_to_date" => Some(("Up to date", Color32::from_rgb(126, 196, 132))),
        "available" => Some(("Updates available", Color32::from_rgb(230, 176, 72))),
        "offline" => Some(("Could not check for updates", Color32::from_rgb(230, 140, 90))),
        _ => None,
    }
}

fn text_link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.link(RichText::new(label).color(Color32::from_rgb(232, 196, 96)).underline())
}

fn is_player_name(account: &str) -> bool {
    let name = account.trim();
    !name.is_empty()
        && !name.eq_ignore_ascii_case("Not logged in")
        && !name.eq_ignore_ascii_case("RuneLite")
        && name != "NoUserNameSelected"
}

fn shown_account(anonymous: bool, account: &str) -> String {
    if anonymous && is_player_name(account) {
        "Hidden".to_string()
    } else {
        account.to_string()
    }
}

fn account_names(clients: &[ClientWindow], runs: &[Running]) -> Vec<String> {
    let mut names: Vec<String> = clients
        .iter()
        .map(|client| client.account())
        .chain(runs.iter().map(|run| run.account.clone()))
        .filter(|name| is_player_name(name))
        .collect();
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    names.dedup();
    names
}

fn conceal_text(anonymous: bool, text: &str, names: &[String]) -> String {
    if !anonymous {
        return text.to_string();
    }
    let mut hidden = text.to_string();
    for name in names {
        hidden = hidden.replace(name, "Hidden");
    }
    hidden
}

fn eye_button(ui: &mut egui::Ui, rect: egui::Rect, open: bool) -> egui::Response {
    let response = ui
        .interact(rect, egui::Id::new("bash-license-eye"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let color = if response.hovered() {
            Color32::from_rgb(232, 196, 96)
        } else {
            Color32::from_rgb(168, 174, 182)
        };
        let painter = ui.painter();
        let center = rect.center();
        painter.add(egui::Shape::Ellipse(egui::epaint::EllipseShape {
            center,
            radius: egui::vec2(6.0, 3.8),
            fill: Color32::TRANSPARENT,
            stroke: Stroke::new(1.2_f32, color),
        }));
        if open {
            painter.circle_filled(center, 1.5, color);
        } else {
            painter.line_segment(
                [center - egui::vec2(4.2, 3.4), center + egui::vec2(4.2, 3.4)],
                Stroke::new(1.2_f32, color),
            );
        }
    }
    response
}

fn license_accepted(status: &str) -> bool {
    let lower = status.to_lowercase();
    lower.contains("accepted") && !lower.contains("not")
}

fn license_color(status: &str) -> Color32 {
    let lower = status.to_lowercase();
    if license_accepted(status) {
        Color32::from_rgb(150, 170, 156)
    } else if lower.contains("not")
        || lower.contains("fail")
        || lower.contains("expir")
        || lower.contains("invalid")
        || lower.contains("could not")
    {
        Color32::from_rgb(230, 140, 90)
    } else {
        Color32::from_rgb(160, 160, 168)
    }
}

fn kind_style(kind: &str) -> (&'static str, Color32) {
    match kind {
        "free" => ("Free", Color32::from_rgb(126, 196, 132)),
        "premium" => ("Premium", Color32::from_rgb(212, 168, 64)),
        "full" => ("Full", Color32::from_rgb(120, 176, 224)),
        "testing" => ("Testing", Color32::from_rgb(224, 154, 90)),
        "local" => ("Local", Color32::from_rgb(176, 176, 184)),
        _ => ("Script", Color32::from_rgb(180, 180, 180)),
    }
}

fn state_style(script: &ScriptEntry, dark: bool) -> (&'static str, Color32) {
    if !script.installed() {
        let color = if dark {
            Color32::from_rgb(154, 154, 162)
        } else {
            Color32::from_rgb(90, 94, 102)
        };
        ("Not installed", color)
    } else if script.update_available() {
        ("Update available", Color32::from_rgb(230, 176, 72))
    } else {
        ("Installed", Color32::from_rgb(126, 196, 132))
    }
}

fn money(amount: f64) -> String {
    format!("${amount:.2}")
}

fn price_labels(ui: &mut egui::Ui, script: &ScriptEntry, size: f32) {
    if !script.shows_price() {
        return;
    }
    let Some(price) = script.price_usd else {
        return;
    };
    ui.spacing_mut().item_spacing.x = 4.0;
    if script.on_sale {
        if let Some(list) = script.list_price_usd.filter(|list| *list > price) {
            ui.label(
                RichText::new(money(list))
                    .size(size - 1.0)
                    .strikethrough()
                    .color(Color32::from_rgb(140, 140, 148)),
            );
        }
        ui.label(
            RichText::new(money(price))
                .size(size)
                .strong()
                .color(Color32::from_rgb(232, 93, 76)),
        );
    } else {
        ui.label(RichText::new(money(price)).size(size).color(ui.visuals().text_color()));
    }
}

fn sale_badge(ui: &mut egui::Ui, discount_pct: u32) {
    let shown = Frame::new()
        .fill(Color32::from_rgb(232, 93, 76))
        .corner_radius(4.0)
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Sale")
                    .size(11.0)
                    .strong()
                    .color(Color32::from_rgb(26, 10, 8)),
            );
        });
    if discount_pct > 0 {
        shown.response.on_hover_text(format!("{discount_pct}% off"));
    }
}

fn runs_icons(ui: &mut egui::Ui, script: &ScriptEntry, icons: &HashMap<String, egui::TextureHandle>, size: f32) {
    for (feature, icon_id, tip) in [
        ("farm_runs", "farming", "Farm runs included"),
        ("birdhouses", "hunter", "Birdhouse runs included"),
    ] {
        if script.features.iter().any(|saved| saved == feature) {
            if let Some(icon) = icons.get(icon_id) {
                ui.add(Image::new(icon).fit_to_exact_size(egui::vec2(size, size)))
                    .on_hover_text(tip);
            }
        }
    }
}

fn kind_badge(ui: &mut egui::Ui, kind: &str) {
    let (label, color) = kind_style(kind);
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 48))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(8.0)
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(11.0).strong().color(color));
        });
}

fn skill_icon_button(ui: &mut egui::Ui, texture: &egui::TextureHandle, selected: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::click());
    let bg = chrome::chip_fill(ui.visuals(), selected, response.hovered());
    ui.painter().rect_filled(rect, 5.0, bg);
    if selected || response.hovered() {
        let stroke = if selected {
            Color32::from_rgb(212, 160, 23)
        } else {
            Color32::from_rgb(90, 90, 96)
        };
        ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.5_f32, stroke), StrokeKind::Inside);
    }
    ui.painter().image(
        texture.id(),
        rect.shrink(4.0),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    response.on_hover_text(tip).clicked()
}

fn script_row(
    ui: &mut egui::Ui,
    script: &ScriptEntry,
    selected: bool,
    icon: Option<&egui::TextureHandle>,
    activity: Option<&str>,
    favorite: bool,
    icons: &HashMap<String, egui::TextureHandle>,
) -> (egui::Response, bool) {
    let width = ui.available_width();
    let dark = ui.visuals().dark_mode;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 58.0), Sense::hover());
    let pointer_in = ui.rect_contains_pointer(rect);
    let bg = chrome::row_fill(ui.visuals(), selected, pointer_in);
    ui.painter().rect_filled(rect, 6.0, bg);
    if selected {
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 3.0, rect.top() + 8.0),
            egui::pos2(rect.left() + 6.0, rect.bottom() - 8.0),
        );
        ui.painter().rect_filled(bar, 2.0, Color32::from_rgb(212, 160, 23));
    }

    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(Layout::left_to_right(Align::Center))
            .id_salt(("script-row", &script.id)),
    );
    let star = star_button(&mut row, favorite);
    row.add_space(6.0);
    if let Some(icon) = icon {
        let added = row.add(Image::new(icon).fit_to_exact_size(egui::vec2(28.0, 28.0)));
        if let Some(activity) = activity {
            added.on_hover_text(activity_label(activity));
        }
        row.add_space(8.0);
    }
    let (state, state_color) = state_style(script, dark);
    let text_width = (row.available_width() - 130.0).max(80.0);
    row.allocate_ui_with_layout(egui::vec2(text_width, 42.0), Layout::top_down(Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let run_count = ["farm_runs", "birdhouses"]
                .into_iter()
                .filter(|feature| script.features.iter().any(|saved| saved == feature))
                .count();
            let reserve = if script.for_sale() { 52.0 } else { 0.0 } + run_count as f32 * 22.0;
            let max_name = (ui.available_width() - reserve).max(24.0);
            let galley = egui::WidgetText::from(RichText::new(&script.name).strong().size(15.0)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                max_name,
                egui::TextStyle::Body,
            );
            ui.add(Label::new(galley));
            if script.for_sale() {
                sale_badge(ui, script.discount_pct);
            }
            runs_icons(ui, script, icons, 16.0);
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            kind_badge(ui, &script.kind);
        });
    });
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(state).color(state_color).strong().size(13.0));
    });
    let select_rect = egui::Rect::from_min_max(egui::pos2(rect.left() + 40.0, rect.top()), rect.max);
    let response = ui.interact(select_rect, ui.id().with(("script-select", &script.id)), Sense::click());
    (response, star)
}

fn star_button(ui: &mut egui::Ui, filled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), Sense::click());
    let color = if filled {
        Color32::from_rgb(232, 196, 96)
    } else if response.hovered() {
        Color32::from_rgb(210, 186, 120)
    } else {
        Color32::from_rgb(128, 128, 136)
    };
    let points = star_points(rect.center(), 8.5, 3.6);
    if filled {
        let inner: Vec<egui::Pos2> = points.iter().skip(1).step_by(2).copied().collect();
        ui.painter().add(egui::Shape::convex_polygon(inner, color, egui::Stroke::NONE));
        for i in 0..5 {
            let tip = points[i * 2];
            let left = points[(i * 2 + 9) % 10];
            let right = points[i * 2 + 1];
            ui.painter().add(egui::Shape::convex_polygon(
                vec![tip, left, right],
                color,
                egui::Stroke::NONE,
            ));
        }
    } else {
        ui.painter().add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(1.4_f32, color),
        ));
    }
    response
        .on_hover_text(if filled {
            "Remove from favorites"
        } else {
            "Add to favorites"
        })
        .clicked()
}

fn star_points(center: egui::Pos2, outer: f32, inner: f32) -> Vec<egui::Pos2> {
    let mut points = Vec::with_capacity(10);
    for i in 0..5 {
        let outer_angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
        let inner_angle = outer_angle + std::f32::consts::PI / 5.0;
        points.push(center + egui::vec2(outer_angle.cos(), outer_angle.sin()) * outer);
        points.push(center + egui::vec2(inner_angle.cos(), inner_angle.sin()) * inner);
    }
    points
}

fn skills_in(category: &str) -> Option<&'static [Skill]> {
    match category {
        "combat" => Some(COMBAT),
        "gathering" => Some(GATHERING),
        "production" => Some(PRODUCTION),
        "utility" => Some(UTILITY),
        "other" => Some(OTHER),
        _ => None,
    }
}

fn row_activity<'a>(script: &'a ScriptEntry, skill: &str) -> Option<&'a str> {
    if !skill.is_empty() {
        if let Some(found) = script.activities.iter().find(|activity| *activity == skill) {
            return Some(found.as_str());
        }
    }
    script.activities.first().map(|activity| activity.as_str())
}

fn activity_label(id: &str) -> &'static str {
    for group in [COMBAT, GATHERING, PRODUCTION, UTILITY, OTHER] {
        if let Some(skill) = group.iter().find(|skill| skill.id == id) {
            return skill.label;
        }
    }
    "Skill"
}

fn script_in_category(script: &ScriptEntry, category: &str, skill: &str) -> bool {
    if !skill.is_empty() {
        return script.activities.iter().any(|activity| activity == skill);
    }
    let Some(skills) = skills_in(category) else {
        return true;
    };
    if category == "other" && script.activities.is_empty() {
        return true;
    }
    script.activities.iter().any(|activity| skills.iter().any(|skill| skill.id == activity))
}

fn library_track_note(component: &crate::state::ComponentState) -> String {
    if component.track == "dev" {
        let installed = if component.installed.is_empty() { "not installed" } else { component.installed.as_str() };
        return format!("{installed} · dev");
    }
    if !component.track.is_empty() {
        let version = component.track.trim().trim_start_matches(['v', 'V']);
        return format!("{version} · pinned");
    }
    if component.installed.is_empty() {
        "not installed".into()
    } else {
        component.installed.clone()
    }
}

fn latest_choice_label(list: &[versions::ListedRelease]) -> String {
    match list.iter().find(|release| !release.prerelease) {
        Some(release) => format!("Latest ({})", release.version),
        None => "Latest".into(),
    }
}

fn dev_choice_label(list: &[versions::ListedRelease]) -> String {
    match list.iter().find(|release| release.prerelease) {
        Some(release) => format!("Dev ({})", release.label()),
        None => "Dev".into(),
    }
}

fn release_choice_label(choice: &str, list: &[versions::ListedRelease]) -> String {
    if choice == "latest" {
        latest_choice_label(list)
    } else if choice == "dev" {
        dev_choice_label(list)
    } else {
        list.iter()
            .find(|release| release.tag == choice)
            .map(|release| release.label())
            .unwrap_or_else(|| choice.to_string())
    }
}

struct LibInstall {
    track: String,
    tag: String,
    version: String,
    zip_url: String,
    label: String,
}

fn resolve_library_install(name: &str, choice: &str, list: &[versions::ListedRelease]) -> Option<LibInstall> {
    let repo = if name == "SRL-B" { versions::SRLB_REPO } else { versions::BASHLIB_REPO };
    let release = if choice == "latest" {
        list.iter().find(|item| !item.prerelease)?
    } else if choice == "dev" {
        list.iter().find(|item| item.prerelease)?
    } else {
        list.iter().find(|item| item.tag == choice)?
    };
    let track = if choice == "latest" {
        String::new()
    } else if choice == "dev" {
        "dev".to_string()
    } else {
        release.tag.clone()
    };
    Some(LibInstall {
        label: if choice == "latest" { release.version.clone() } else { release.label() },
        zip_url: versions::tag_zip_url(repo.0, repo.1, &release.tag),
        track,
        tag: release.tag.clone(),
        version: release.version.clone(),
    })
}

fn install_library_release(paths: &SimbaPaths, name: &str, spec: &LibInstall) -> Result<(), String> {
    let client = crate::download::client(versions::USER_AGENT)?;
    crate::install::install_package(&client, paths, name, &spec.zip_url, None, (0.0, 1.0))?;
    let mut state = crate::state::BashState::load(&paths.state_file());
    let component = if name == "SRL-B" { &mut state.srlb } else { &mut state.bashlib };
    component.track = spec.track.clone();
    component.installed_tag = spec.tag.clone();
    component.installed = if spec.tag.contains("-dev") {
        spec.version.clone()
    } else {
        versions::installed_lib_version(&paths.include(name)).unwrap_or_else(|| spec.version.clone())
    };
    if spec.track == "dev" {
        component.dev_tag = spec.tag.clone();
    } else {
        component.dev_tag.clear();
    }
    component.dev_checkout = false;
    crate::state::write_library_status(&mut state, false);
    state.save(&paths.state_file())?;
    Ok(())
}

fn script_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Simba's editor writes one 32-bit state to the script process: 0 pause, 1 stop, 2 run.
fn send_simba_state(stdin: &mut ChildStdin, state: u32) -> bool {
    stdin.write_all(&state.to_le_bytes()).is_ok() && stdin.flush().is_ok()
}

struct WindowRect {
    left: i32,
    top: i32,
    right: i32,
}

/// Follows the RuneLite window from outside it. The output window stays a
/// normal BASH window: it is not parented, owned, or drawn into the client.
struct OutputAnchor {
    offset: Option<(i32, i32)>,
    client: Option<(i32, i32)>,
    output: Option<(i32, i32)>,
    wait: Option<(i32, i32)>,
    wait_frames: u8,
}

impl Default for OutputAnchor {
    fn default() -> Self {
        Self {
            offset: None,
            client: None,
            output: None,
            wait: None,
            wait_frames: 0,
        }
    }
}

impl OutputAnchor {
    /// Physical top-left to move the output window to, when it needs to move.
    fn next_move(&mut self, client: Option<&WindowRect>, pixels_per_point: f32) -> Option<(i32, i32)> {
        let client = client?;
        let origin = (client.left, client.top);
        if self.offset.is_none() {
            let pos = dock_beside(client, pixels_per_point);
            self.offset = Some((pos.0 - origin.0, pos.1 - origin.1));
            self.client = Some(origin);
            self.wait = Some(pos);
            self.wait_frames = 0;
            return Some(pos);
        }
        let offset = self.offset.unwrap_or((0, 0));
        let expected = (origin.0 + offset.0, origin.1 + offset.1);
        if self.client != Some(origin) {
            self.client = Some(origin);
            self.wait = Some(expected);
            self.wait_frames = 0;
            return Some(expected);
        }
        if let Some(wait) = self.wait {
            self.wait_frames = self.wait_frames.saturating_add(1);
            if let Some(output) = self.output {
                if near_px(output, wait) || self.wait_frames > 8 {
                    self.wait = None;
                    if !near_px(output, wait) {
                        self.offset = Some((output.0 - origin.0, output.1 - origin.1));
                    }
                }
            }
            return None;
        }
        if let Some(output) = self.output {
            if !near_px(output, expected) {
                self.offset = Some((output.0 - origin.0, output.1 - origin.1));
            }
        }
        None
    }
}

fn near_px(a: (i32, i32), b: (i32, i32)) -> bool {
    (a.0 - b.0).abs() <= 12 && (a.1 - b.1).abs() <= 12
}

/// Read the RuneLite window rectangle. This does not send the client a message,
/// parent the output window, or install a hook.
fn client_window_rect(hwnd: isize) -> Option<WindowRect> {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindow},
    };
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        Some(WindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
        })
    }
}

fn virtual_screen() -> (i32, i32, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// Place the output window on the free side of the client, top aligned.
fn dock_beside(client: &WindowRect, pixels_per_point: f32) -> (i32, i32) {
    let scale = pixels_per_point.max(1.0);
    let gap = 8;
    let output_w = (760.0 * scale).round() as i32 + 24;
    let output_h = (480.0 * scale).round() as i32 + 64;
    let (vx, vy, vw, vh) = virtual_screen();
    let right_limit = vx + vw;
    let bottom_limit = vy + vh;
    let mut x = client.right + gap;
    if x + output_w > right_limit {
        let left_x = client.left - gap - output_w;
        if left_x >= vx {
            x = left_x;
        }
    }
    if x < vx {
        x = vx;
    }
    if x + output_w > right_limit {
        x = (right_limit - output_w).max(vx);
    }
    let mut y = client.top;
    if y + output_h > bottom_limit {
        y = (bottom_limit - output_h).max(vy);
    }
    if y < vy {
        y = vy;
    }
    (x, y)
}

fn spawn_output_reader(mut stream: impl Read + Send + 'static, incoming: Arc<Mutex<Vec<u8>>>, left: Arc<AtomicU8>) {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut extra) = incoming.lock() {
                        extra.extend_from_slice(&buf[..n]);
                    }
                }
            }
        }
        left.fetch_sub(1, Ordering::Relaxed);
    });
}

fn drain_output(run: &mut Running) {
    let extra = match run.incoming.lock() {
        Ok(mut buf) if !buf.is_empty() => std::mem::take(&mut *buf),
        _ => return,
    };
    run.line_buf.push_str(&String::from_utf8_lossy(&extra));
    let mut log = String::new();
    while let Some(pos) = run.line_buf.find('\n') {
        let line: String = run.line_buf.drain(..=pos).collect();
        let trimmed = line.trim_end_matches(['\r', '\n']).trim();
        if let Some(status) = trimmed.strip_prefix("#BASH status") {
            let text = status.trim();
            run.activity = text.chars().take(80).collect();
        } else {
            log.push_str(&line);
        }
    }
    if log.is_empty() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&run.log_path) {
        let _ = file.write_all(log.as_bytes());
    }
    run.log_text.push_str(&log);
    const MAX: usize = 120_000;
    if run.log_text.len() > MAX {
        let mut cut = run.log_text.len() - MAX;
        while !run.log_text.is_char_boundary(cut) && cut < run.log_text.len() {
            cut += 1;
        }
        run.log_text.drain(..cut);
    }
}

fn format_clock(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}
