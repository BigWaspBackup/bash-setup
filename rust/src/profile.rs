//! The BASH RuneLite profile. The settings file is the shared launcher profile
//! with per-account lines removed. The profile name comes from the install id,
//! the same way the old launcher named it.

use std::{fs, path::PathBuf, process::Command};

use serde_json::{json, Value};

const PROFILE_TEMPLATE: &str = include_str!("../assets/runelite-profile.properties");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct ProfileInstall {
    pub name: String,
    pub updated: bool,
}

pub fn profiles_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(home).join(".runelite").join("profiles2"))
}

/// `(name, id)` matching `TWaspUpdater.InstallRLProfile`.
pub fn profile_identity(install_id: &str) -> (String, i32) {
    let mut hex: String = install_id.chars().filter(|c| *c != '-').flat_map(|c| c.to_lowercase()).collect();
    while hex.len() < 12 {
        hex.push('0');
    }
    let hex: String = hex.chars().take(32).collect();
    let name: String = hex.chars().take(10).collect();
    let id_src: String = hex.chars().take(12).collect();
    (name, profile_id(&id_src))
}

fn profile_id(hex: &str) -> i32 {
    let mut result: i32 = 0;
    for byte in hex.bytes() {
        result = result.wrapping_mul(31).wrapping_add(i32::from(byte));
    }
    if result < 0 {
        result = result.wrapping_neg();
    }
    if result == 0 {
        1
    } else {
        result
    }
}

pub fn is_installed(install_id: &str) -> bool {
    let Some(dir) = profiles_dir() else {
        return false;
    };
    let (name, id) = profile_identity(install_id);
    dir.join(format!("{name}-{id}.properties")).is_file()
}

pub fn install(install_id: &str) -> Result<ProfileInstall, String> {
    let dir = profiles_dir().ok_or("Could not find the user profile folder.")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the RuneLite profiles folder: {e}"))?;
    let (name, id) = profile_identity(install_id);
    let file = dir.join(format!("{name}-{id}.properties"));
    let updated = file.is_file();
    fs::write(&file, PROFILE_TEMPLATE).map_err(|e| format!("Could not write the RuneLite profile: {e}"))?;
    register(&dir.join("profiles.json"), &name, id)?;
    Ok(ProfileInstall { name, updated })
}

fn register(path: &std::path::Path, name: &str, id: i32) -> Result<(), String> {
    let existing = fs::read_to_string(path).unwrap_or_else(|_| "{\"profiles\":[]}".to_string());
    let mut value: Value = serde_json::from_str(&existing).map_err(|_| {
        "profiles.json could not be read, so it was left unchanged.".to_string()
    })?;
    let Some(list) = value.get_mut("profiles").and_then(|entry| entry.as_array_mut()) else {
        return Err("profiles.json has no profiles list, so it was left unchanged.".into());
    };
    if list.iter().any(|entry| entry.get("name").and_then(|n| n.as_str()) == Some(name)) {
        return Ok(());
    }
    list.push(json!({
        "id": id,
        "name": name,
        "sync": false,
        "active": false,
        "rev": -1
    }));
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("Could not update profiles.json: {e}"))
}

/// Make the BASH profile the one RuneLite opens, and mark every other profile inactive.
pub fn set_active(install_id: &str) -> Result<(), String> {
    let dir = profiles_dir().ok_or("Could not find the user profile folder.")?;
    let (name, id) = profile_identity(install_id);
    let path = dir.join("profiles.json");
    let existing = fs::read_to_string(&path).unwrap_or_else(|_| "{\"profiles\":[]}".to_string());
    let mut value: Value = serde_json::from_str(&existing).map_err(|_| {
        "profiles.json could not be read, so the active profile was left unchanged.".to_string()
    })?;
    mark_active(&mut value, &name, id)?;
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("Could not update profiles.json: {e}"))
}

fn mark_active(value: &mut Value, name: &str, id: i32) -> Result<(), String> {
    let Some(list) = value.get_mut("profiles").and_then(|entry| entry.as_array_mut()) else {
        return Err("profiles.json has no profiles list, so the active profile was left unchanged.".into());
    };
        let mut found = false;
        let mut bindings = Vec::new();
        for entry in list.iter() {
            if let Some(items) = entry.get("defaultForRsProfiles").and_then(|item| item.as_array()) {
                for item in items {
                    if !bindings.iter().any(|saved| saved == item) {
                        bindings.push(item.clone());
                    }
                }
            }
        }
        for entry in list.iter_mut() {
            let Some(obj) = entry.as_object_mut() else {
                continue;
            };
            let is_ours = obj.get("name").and_then(|item| item.as_str()) == Some(name);
            obj.insert("active".to_string(), json!(is_ours));
            if is_ours {
                found = true;
                obj.insert("sync".to_string(), json!(false));
                if !bindings.is_empty() {
                    obj.insert("defaultForRsProfiles".to_string(), json!(bindings.clone()));
                }
            } else if obj.contains_key("defaultForRsProfiles") && !bindings.is_empty() {
                obj.insert("defaultForRsProfiles".to_string(), json!([]));
            }
        }
    if !found {
        list.push(json!({
            "id": id,
            "name": name,
            "sync": false,
            "active": true,
            "rev": -1
        }));
    }
    Ok(())
}

pub fn runelite_running() -> bool {
    let mut command = Command::new("tasklist");
    command.args(["/FI", "IMAGENAME eq RuneLite.exe", "/NH"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains("runelite.exe"))
        .unwrap_or(false)
}

pub fn close_runelite() -> bool {
    let mut command = Command::new("taskkill");
    command.args(["/F", "/IM", "RuneLite.exe"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.status().map(|status| status.success()).unwrap_or(false)
}

/// Launcher settings files. Scaling lives here, beside the JVM and client arguments.
fn launcher_settings_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE") {
        files.push(PathBuf::from(home).join(".runelite").join("settings.json"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        files.push(PathBuf::from(local).join("RuneLite").join("settings.json"));
    }
    for install in runelite_install_dirs() {
        files.push(install.join("settings.json"));
    }
    files.retain(|path| path.is_file());
    files.sort();
    files.dedup();
    files
}

fn runelite_install_dirs() -> Vec<PathBuf> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-Command",
        "$roots = @('HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*','HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'); Get-ItemProperty $roots -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq 'RuneLite' -and $_.InstallLocation } | ForEach-Object { $_.InstallLocation }",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Set RuneLite launcher Scaling to 1.0 when a saved value is something else.
/// `Some(true)` means a file changed while RuneLite is open.
pub fn fix_launcher_scale() -> Option<bool> {
    let changed = launcher_settings_files().into_iter().any(|path| set_scale_to_one(&path));
    if changed {
        Some(runelite_running())
    } else {
        None
    }
}

fn set_scale_to_one(path: &std::path::Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let Some(scale) = value.get("scale").and_then(Value::as_f64) else {
        return false;
    };
    if (scale - 1.0).abs() < 0.001 {
        return false;
    }
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    object.insert("scale".to_string(), json!(1.0));
    let Ok(pretty) = serde_json::to_string_pretty(&value) else {
        return false;
    };
    fs::write(path, pretty + "\n").is_ok()
}

#[cfg(test)]
mod tests {
    use super::{mark_active, profile_identity};
    use serde_json::json;

    #[test]
    fn name_is_ten_hex_chars() {
        let (name, id) = profile_identity("A1B2C3D4-E5F6-7890-ABCD-EF1234567890");
        assert_eq!(name, "a1b2c3d4e5");
        assert!(id > 0);
    }

    #[test]
    fn mark_active_selects_only_the_bash_profile() {
        let mut value = json!({
            "profiles": [
                {"id": 1, "name": "other", "sync": true, "active": true},
                {"id": 2, "name": "bash", "sync": true, "active": false}
            ]
        });
        mark_active(&mut value, "bash", 2).unwrap();
        let profiles = value["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["active"], false);
        assert_eq!(profiles[0]["sync"], true);
        assert_eq!(profiles[1]["active"], true);
        assert_eq!(profiles[1]["sync"], false);
    }

    #[test]
    fn mark_active_moves_character_bindings_onto_the_bash_profile() {
        let mut value = json!({
            "profiles": [
                {"id": 1, "name": "other", "sync": false, "active": true, "defaultForRsProfiles": ["character-a"]},
                {"id": 2, "name": "bash", "sync": false, "active": false}
            ]
        });
        mark_active(&mut value, "bash", 2).unwrap();
        let profiles = value["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["active"], false);
        assert_eq!(profiles[0]["defaultForRsProfiles"], json!([]));
        assert_eq!(profiles[1]["active"], true);
        assert_eq!(profiles[1]["defaultForRsProfiles"], json!(["character-a"]));
    }

    #[test]
    fn scaling_above_one_is_set_to_one_and_other_keys_stay() {
        let path = std::env::temp_dir().join(format!("bash-scale-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"scale":1.5,"clientArguments":"keep-me","jvmArguments":""}"#).unwrap();
        assert!(super::set_scale_to_one(&path));
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["scale"], 1.0);
        assert_eq!(value["clientArguments"], "keep-me");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_scaling_is_left_alone() {
        let path = std::env::temp_dir().join(format!("bash-scale-missing-{}.json", std::process::id()));
        let original = r#"{"clientArguments":"keep-me"}"#;
        std::fs::write(&path, original).unwrap();
        assert!(!super::set_scale_to_one(&path));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let _ = std::fs::remove_file(&path);
    }
}
