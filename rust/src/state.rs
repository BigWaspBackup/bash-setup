//! `Data\bash\state.json` - what BASH.exe knows about the install. The launcher
//! script reads this to show versions without hitting GitHub itself.

use std::{
    collections::HashMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentState {
    #[serde(default)]
    pub installed: String,
    #[serde(default)]
    pub latest: String,
    /// Set when the include is a junction/symlink or git checkout.
    #[serde(default)]
    pub dev_checkout: bool,
    /// Empty follows the newest normal release. "dev" follows the newest
    /// pre-release. Any other value is a release tag that stays put.
    #[serde(default)]
    pub track: String,
    /// Tag last installed by BASH. Empty until a version is chosen in Settings.
    #[serde(default)]
    pub installed_tag: String,
    /// Newest pre-release tag, when this library is tracking dev.
    #[serde(default)]
    pub dev_tag: String,
}

impl ComponentState {
    /// True when Update libraries should change this component.
    pub fn is_behind(&self) -> bool {
        if self.track.is_empty() {
            return !self.latest.is_empty() && self.installed != self.latest;
        }
        if self.track == "dev" {
            return !self.dev_tag.is_empty() && self.installed_tag != self.dev_tag;
        }
        if !self.installed_tag.is_empty() {
            return self.installed_tag != self.track;
        }
        let wanted = self.track.trim().trim_start_matches(['v', 'V']);
        !wanted.is_empty() && self.installed != wanted
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BashState {
    #[serde(default)]
    pub simba_tag: String,
    #[serde(default)]
    pub bootstrapper_version: String,
    #[serde(default)]
    pub last_check: u64,
    #[serde(default)]
    pub simba: ComponentState,
    #[serde(default)]
    pub srlb: ComponentState,
    #[serde(default)]
    pub bashlib: ComponentState,
    #[serde(default)]
    pub launcher: ComponentState,
    #[serde(default)]
    pub bootstrapper: ComponentState,
    /// "up_to_date", "available", "restart", or "offline" after the BASH.exe check.
    #[serde(default)]
    pub bootstrapper_status: String,
    /// "up_to_date", "available", or "offline" after the startup library check.
    #[serde(default)]
    pub library_status: String,
    /// Account names seen in a RuneLite title bar.
    #[serde(default)]
    pub accounts: Vec<String>,
    /// The first-launch RuneLite profile question has been answered.
    #[serde(default)]
    pub prompted_profile: bool,
    /// The first script run has already suggested Settings Searcher.
    #[serde(default)]
    pub prompted_setup: bool,
    /// Discord username after a successful launcher login. Empty until they log in.
    #[serde(default)]
    pub discord_user: String,
    /// Seconds each account has been running scripts, including finished sessions.
    #[serde(default)]
    pub play_seconds: HashMap<String, u64>,
    /// Script ids the user starred. These stay at the top of the list.
    #[serde(default)]
    pub favorites: Vec<String>,
    /// The script output window follows the RuneLite window it belongs to.
    #[serde(default = "default_true")]
    pub anchor_output: bool,
    /// "dark" or "light". Empty means dark.
    #[serde(default)]
    pub theme: String,
    /// RuneLite account names are shown as Hidden outside account settings.
    #[serde(default)]
    pub anonymous: bool,
}

fn default_true() -> bool {
    true
}

impl BashState {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))
    }

    pub fn touch(&mut self) {
        self.last_check = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
}

pub fn libraries_behind(state: &BashState) -> bool {
    [&state.simba, &state.srlb, &state.bashlib].into_iter().any(ComponentState::is_behind)
}

pub fn write_library_status(state: &mut BashState, offline: bool) {
    state.library_status = if libraries_behind(state) {
        "available".to_string()
    } else if offline {
        "offline".to_string()
    } else {
        "up_to_date".to_string()
    };
}

#[cfg(test)]
mod tests {
    use super::ComponentState;

    #[test]
    fn a_pinned_release_is_not_treated_as_out_of_date() {
        let pinned = ComponentState {
            installed: "8.4.10".into(),
            latest: "8.4.14".into(),
            installed_tag: "v8.4.10".into(),
            track: "v8.4.10".into(),
            ..ComponentState::default()
        };
        assert!(!pinned.is_behind());

        let mut dev = pinned.clone();
        dev.track = "dev".into();
        dev.installed_tag = "v8.4.15-dev".into();
        dev.dev_tag = "v8.4.15-dev".into();
        assert!(!dev.is_behind());
        dev.dev_tag = "v8.4.16-dev".into();
        assert!(dev.is_behind());
    }

    #[test]
    fn a_newer_release_is_behind_even_for_a_local_folder() {
        let local = ComponentState {
            installed: "8.4.10".into(),
            latest: "8.4.14".into(),
            dev_checkout: true,
            ..ComponentState::default()
        };
        assert!(local.is_behind());
    }
}
