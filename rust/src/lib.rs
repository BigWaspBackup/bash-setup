//! Shared code for `bash-setup.exe` (installer) and `BASH.exe` (bootstrapper).

pub mod agility;
pub mod bootstrap;
pub mod catalog;
pub mod clients;
pub mod download;
pub mod gear;
pub mod icons;
pub mod install;
pub mod launcher;
pub mod locks;
pub mod log;
pub mod paths;
pub mod profile;
pub mod repair;
pub mod scriptprofile;
mod salvage_data;
pub mod salvage;
pub mod scriptsetup;
pub mod shops;
pub mod skillcatalog;
pub mod shortcut;
pub mod state;
pub mod ui;
pub mod unzip;
pub mod versions;
pub mod wasplib;
pub mod worlds;

pub const DISCORD_URL: &str = "https://discord.gg/qsmKs5uKfR";
pub const ICON_BYTES: &[u8] = include_bytes!("../assets/bash.png");

/// Messages sent from a background worker thread to a UI.
#[derive(Debug, Clone)]
pub enum ProgressMessage {
    Progress(f32),
    Status(String),
    Completed,
    Error(String),
}

pub type ProgressSender = std::sync::mpsc::Sender<ProgressMessage>;

pub fn send_status(tx: &ProgressSender, status: impl Into<String>) {
    let status = status.into();
    log::log_line(&status);
    let _ = tx.send(ProgressMessage::Status(status));
}

pub fn send_progress(tx: &ProgressSender, progress: f32) {
    let _ = tx.send(ProgressMessage::Progress(progress.clamp(0.0, 1.0)));
}
