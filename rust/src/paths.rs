use std::{env, path::PathBuf};

pub const LAUNCHER_FILE: &str = "bash-launcher.simba";
pub const SIMBA_EXE: &str = "Simba64.exe";
pub const BOOTSTRAPPER_EXE: &str = "BASH.exe";

pub const SRLB_DIR: &str = "SRL-B";
pub const BASHLIB_DIR: &str = "BashLib";

pub fn simba_install_dir() -> PathBuf {
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

#[derive(Clone)]
pub struct SimbaPaths {
    pub root: PathBuf,
}

impl SimbaPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_install() -> Self {
        Self::new(simba_install_dir())
    }

    pub fn simba_exe(&self) -> PathBuf {
        self.root.join(SIMBA_EXE)
    }

    pub fn bootstrapper_exe(&self) -> PathBuf {
        self.root.join(BOOTSTRAPPER_EXE)
    }

    pub fn data(&self) -> PathBuf {
        self.root.join("Data")
    }

    pub fn bash_data(&self) -> PathBuf {
        self.data().join("bash")
    }

    pub fn downloads(&self) -> PathBuf {
        self.bash_data().join("downloads")
    }

    /// Extracted item images, map chunks, and other files Simba rebuilds as scripts run.
    pub fn cache(&self) -> PathBuf {
        self.data().join("cache")
    }

    pub fn state_file(&self) -> PathBuf {
        self.bash_data().join("state.json")
    }

    pub fn includes(&self) -> PathBuf {
        self.root.join("Includes")
    }

    pub fn include(&self, name: &str) -> PathBuf {
        self.includes().join(name)
    }

    pub fn scripts(&self) -> PathBuf {
        self.root.join("Scripts")
    }

    pub fn launcher(&self) -> PathBuf {
        self.scripts().join(LAUNCHER_FILE)
    }

    /// Where the launcher installs scripts. Same folder the Simba launcher used.
    pub fn installed_scripts(&self) -> PathBuf {
        self.scripts().join("bigaussie")
    }

    /// Shared with the Simba launcher so a key saved in either place works.
    pub fn launcher_config(&self) -> PathBuf {
        self.configs().join("bash-launcher.json")
    }

    pub fn run_logs(&self) -> PathBuf {
        self.bash_data().join("logs")
    }

    /// Script screenshots. BashLib writes these under Simba's Data folder.
    pub fn screenshots(&self) -> PathBuf {
        self.data().join("screenshots")
    }

    /// Simba's own log folder (`Data\logs`), separate from script output logs.
    pub fn simba_logs(&self) -> PathBuf {
        self.data().join("logs")
    }

    pub fn configs(&self) -> PathBuf {
        self.root.join("Configs")
    }

    /// Shared BashLib settings, including remote input.
    pub fn wasplib_config(&self) -> PathBuf {
        self.configs().join("wasplib.json")
    }

    pub fn fonts(&self) -> PathBuf {
        self.root.join("Fonts")
    }

    pub fn default_script(&self) -> PathBuf {
        self.data().join("default.simba")
    }
}

/// Body written to `Data\default.simba`. Users should open BASH Launcher 2.0
/// via the desktop shortcut (`BASH.exe`), not this Simba tab.
pub fn default_simba_body() -> String {
    format!(
        "(* Thank you for choosing B.A.S.H - BigAussie Script House *)\n\
         (* Discord: {} *)\n\n\
         (* Use the B.A.S.H desktop shortcut to open BASH Launcher 2.0 (BASH.exe). *)\n\
         (* That is how you update libs, install scripts, and run them. *)\n\n\
         begin\nend.\n",
        crate::DISCORD_URL
    )
}

pub fn packages_ini_body(install_dir: &str) -> String {
    format!(
        "[BigWaspBackup/SRL-B]\nName=SRL-B\nTemplates={}\\Includes\\SRL-B\\templates\n\n\
         [BigWaspBackup/BashLib]\nName=BashLib\nTemplates={}\\Includes\\BashLib\\templates\n",
        install_dir, install_dir,
    )
}
