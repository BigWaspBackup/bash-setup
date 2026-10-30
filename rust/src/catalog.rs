//! Script catalog, license and install. Talks to the same premium API as the
//! Simba launcher and writes scripts to `Scripts\bigaussie`, so copies already
//! installed there show up here.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::blocking::Client;
use serde_json::Value;

use crate::{download, log::log_line, paths::SimbaPaths, versions::PREMIUM_API};

const USER_AGENT: &str = "B.A.S.H-Launcher";

#[derive(Debug, Clone)]
pub struct RemoteScript {
    pub id: String,
    pub name: String,
    pub description: String,
    pub file_name: String,
    pub revision: i64,
    pub kind: String,
    pub entitled: bool,
    /// Discounted on the store right now.
    pub on_sale: bool,
    pub discount_pct: u32,
    pub price_usd: Option<f64>,
    pub list_price_usd: Option<f64>,
    pub activities: Vec<String>,
    /// Extra behaviour bundled into the script, such as `farm_runs` and `birdhouses`.
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub file_name: String,
    pub revision: i64,
    pub kind: String,
    pub entitled: bool,
    /// Discounted on the store right now.
    pub on_sale: bool,
    pub discount_pct: u32,
    pub price_usd: Option<f64>,
    pub list_price_usd: Option<f64>,
    pub activities: Vec<String>,
    /// Extra behaviour bundled into the script, such as `farm_runs` and `birdhouses`.
    pub features: Vec<String>,
    pub local_path: Option<PathBuf>,
    pub local_revision: Option<i64>,
}

impl ScriptEntry {
    pub fn installed(&self) -> bool {
        self.local_path.is_some()
    }

    pub fn update_available(&self) -> bool {
        match (self.local_revision, self.revision) {
            (Some(local), remote) if remote > 0 && local < remote => true,
            _ => false,
        }
    }

    /// On sale, and this license does not already include it.
    pub fn for_sale(&self) -> bool {
        self.on_sale && !self.entitled
    }

    /// Farm runs, birdhouse runs, or both, when this script includes them alongside its main activity.
    pub fn included_runs(&self) -> Option<&'static str> {
        let farm = self.features.iter().any(|feature| feature == "farm_runs");
        let birdhouses = self.features.iter().any(|feature| feature == "birdhouses");
        match (farm, birdhouses) {
            (true, true) => Some("Includes farm and birdhouse runs."),
            (true, false) => Some("Includes farm runs."),
            (false, true) => Some("Includes birdhouse runs."),
            (false, false) => None,
        }
    }

    /// Store price. Free and Full scripts are not sold at the catalog amount, so they have no price here.
    pub fn shows_price(&self) -> bool {
        self.kind != "free"
            && self.kind != "full"
            && self.price_usd.is_some_and(|price| price > 0.0)
    }

    pub fn state_label(&self) -> &'static str {
        if !self.installed() {
            "not installed"
        } else if self.update_available() {
            "update available"
        } else {
            "installed"
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub scripts: Vec<RemoteScript>,
    pub license_status: String,
}

#[derive(Debug, Clone)]
pub struct ChangelogEntry {
    pub revision: Option<i64>,
    pub notes: String,
    pub at: String,
}

pub struct LauncherConfig {
    pub license_key: String,
    pub install_id: String,
}

pub fn load_config(path: &Path) -> LauncherConfig {
    let mut license_key = String::new();
    let mut install_id = String::new();
    if let Ok(text) = fs::read_to_string(path) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(s) = value.get("license_key").and_then(|v| v.as_str()) {
                license_key = s.trim().to_string();
            }
            if let Some(s) = value.get("install_id").and_then(|v| v.as_str()) {
                install_id = s.trim().to_string();
            }
        }
    }
    if install_id.is_empty() {
        install_id = new_install_id();
    }
    LauncherConfig { license_key, install_id }
}

/// Writes the key and install id without dropping other keys the Simba
/// launcher stores in the same file.
pub fn save_config(path: &Path, license_key: &str, install_id: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    let mut value = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let obj = value.as_object_mut().ok_or("Launcher config is not a JSON object.")?;
    let key = license_key.trim();
    if key.is_empty() {
        obj.remove("license_key");
    } else {
        obj.insert("license_key".into(), Value::String(key.to_string()));
    }
    obj.insert("install_id".into(), Value::String(install_id.to_string()));
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("Could not save the license: {e}"))
}

pub fn fetch_catalog(license_key: &str, install_id: &str) -> Result<CatalogSnapshot, String> {
    let client = download::client(USER_AGENT)?;
    let body = get_json(&client, &format!("{PREMIUM_API}/v1/catalog"), None, install_id)?;
    let mut scripts = parse_scripts(&body, false)?;
    scripts.retain(|s| !s.file_name.is_empty());

    let license_status = if license_key.trim().is_empty() {
        "No license key saved. Purchase from the BASH store or Discord.".to_string()
    } else {
        match get_json(&client, &format!("{PREMIUM_API}/v1/me"), Some(license_key.trim()), install_id) {
            Ok(me) => apply_entitlements(&mut scripts, &me),
            Err(e) => e,
        }
    };

    scripts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(CatalogSnapshot { scripts, license_status })
}

/// Public changelog for one script. Newest entry first.
pub fn fetch_changelog(script_id: &str) -> Result<Vec<ChangelogEntry>, String> {
    let client = download::client(USER_AGENT)?;
    let url = format!("{PREMIUM_API}/v1/scripts/{script_id}/changelog");
    let response = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("Could not load the changelog: {e}"))?;
    let status = response.status();
    let text = response.text().map_err(|e| format!("Bad changelog response: {e}"))?;
    if status.as_u16() == 404 {
        return Err("No changelog for this script.".into());
    }
    if !status.is_success() {
        return Err(format!("Changelog returned HTTP {status}."));
    }
    let body: Value = serde_json::from_str(&text).map_err(|_| "Changelog could not be read.".to_string())?;
    let entries = body.get("entries").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for entry in entries {
        let notes = entry.get("notes").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if notes.is_empty() {
            continue;
        }
        out.push(ChangelogEntry {
            revision: entry.get("revision").and_then(|v| v.as_i64()),
            notes,
            at: entry.get("at").and_then(|v| v.as_str()).unwrap_or("").trim().to_string(),
        });
    }
    Ok(out)
}

/// Result of opening Discord in the browser and waiting for the license key.
#[derive(Debug, Clone)]
pub enum DiscordLogin {
    Ready { key: String, username: String },
    NoLicense { username: String },
    Cancelled,
    TimedOut,
    Failed(String),
}

/// Opens Discord login in the browser and waits until the API has the key, the
/// user cancels, or three minutes pass. Call this off the UI thread.
pub fn discord_login() -> DiscordLogin {
    let client = match download::client(USER_AGENT) {
        Ok(client) => client,
        Err(error) => return DiscordLogin::Failed(error),
    };
    let started = match post_json(&client, &format!("{PREMIUM_API}/v1/auth/launcher/start")) {
        Ok(body) => body,
        Err(error) => return DiscordLogin::Failed(error),
    };
    let login_id = started.get("login_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let url = started.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if login_id.is_empty() || url.is_empty() {
        return DiscordLogin::Failed("Discord login did not start.".into());
    }
    if !crate::ui::open_url(&url) {
        return DiscordLogin::Failed("Could not open the browser for Discord login.".into());
    }
    for _ in 0..180 {
        thread::sleep(Duration::from_secs(1));
        let body = match get_json(
            &client,
            &format!("{PREMIUM_API}/v1/auth/launcher/{login_id}"),
            None,
            "",
        ) {
            Ok(body) => body,
            Err(_) => continue,
        };
        let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        match status {
            "pending" => {}
            "ready" => {
                let key = body.get("license_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                if key.is_empty() {
                    return DiscordLogin::NoLicense { username };
                }
                return DiscordLogin::Ready { key, username };
            }
            "none" => return DiscordLogin::NoLicense { username },
            "denied" => return DiscordLogin::Cancelled,
            "expired" => return DiscordLogin::TimedOut,
            _ => {}
        }
    }
    DiscordLogin::TimedOut
}

pub fn with_local(paths: &SimbaPaths, remote: &[RemoteScript]) -> Vec<ScriptEntry> {
    let dir = paths.installed_scripts();
    let mut entries: Vec<ScriptEntry> = remote
        .iter()
        .map(|script| {
            let path = dir.join(&script.file_name);
            let (local_path, local_revision) = if path.is_file() {
                (Some(path.clone()), read_revision(&path))
            } else {
                (None, None)
            };
            ScriptEntry {
                id: script.id.clone(),
                name: script.name.clone(),
                description: script.description.clone(),
                file_name: script.file_name.clone(),
                revision: script.revision,
                kind: script.kind.clone(),
                entitled: script.entitled,
                on_sale: script.on_sale,
                discount_pct: script.discount_pct,
                price_usd: script.price_usd,
                list_price_usd: script.list_price_usd,
                activities: script.activities.clone(),
                features: script.features.clone(),
                local_path,
                local_revision,
            }
        })
        .collect();

    let known: Vec<String> = entries.iter().map(|e| e.file_name.to_lowercase()).collect();
    if let Ok(read) = fs::read_dir(&dir) {
        for item in read.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("simba")) != Some(true) {
                continue;
            }
            let file_name = item.file_name().to_string_lossy().to_string();
            if known.iter().any(|k| k == &file_name.to_lowercase()) {
                continue;
            }
            if file_name.to_lowercase().contains("launcher") {
                continue;
            }
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(file_name.clone());
            entries.push(ScriptEntry {
                id: format!("local:{file_name}"),
                name,
                description: "Installed on this computer. It is not in the current catalog.".to_string(),
                file_name,
                revision: 0,
                kind: "local".into(),
                entitled: true,
                on_sale: false,
                discount_pct: 0,
                price_usd: None,
                list_price_usd: None,
                activities: Vec::new(),
                features: Vec::new(),
                local_revision: read_revision(&path),
                local_path: Some(path),
            });
        }
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

pub fn download_script(script: &ScriptEntry, paths: &SimbaPaths, license_key: &str, install_id: &str) -> Result<PathBuf, String> {
    if script.id.starts_with("local:") {
        return Err("This script is already on disk.".into());
    }
    if script.kind != "free" && !script.entitled {
        return Err(format!("\"{}\" needs an active license key. Enter it above and press Save.", script.name));
    }
    let dir = paths.installed_scripts();
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let dest = dir.join(&script.file_name);
    let temp = dir.join(format!("{}.download", script.file_name));
    let url = format!("{PREMIUM_API}/v1/scripts/{}/download", script.id);
    let key = if script.kind == "free" { None } else { Some(license_key.trim()) };
    if key == Some("") {
        return Err("Enter a license key and press Save before installing this script.".into());
    }

    log_line(&format!("download script {} -> {}", script.name, dest.display()));
    let client = download::client(USER_AGENT)?;
    let mut request = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("X-Install-Id", install_id);
    if let Some(key) = key {
        request = request.header("X-License-Key", key);
    }
    let mut response = request.send().map_err(|e| format!("Download failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let _ = fs::remove_file(&temp);
        if body.contains("license_expired") {
            return Err(format!("License expired. \"{}\" was not changed.", script.name));
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(format!("License rejected for \"{}\". The local copy was kept.", script.name));
        }
        return Err(format!("Download of \"{}\" failed (HTTP {}).", script.name, status));
    }
    {
        let mut file = fs::File::create(&temp).map_err(|e| format!("Could not write {}: {e}", temp.display()))?;
        std::io::copy(&mut response, &mut file).map_err(|e| format!("Download of \"{}\" failed: {e}", script.name))?;
        file.flush().ok();
    }
    if dest.exists() {
        fs::remove_file(&dest).map_err(|e| {
            let _ = fs::remove_file(&temp);
            format!("Could not replace {}: {e}", dest.display())
        })?;
    }
    fs::rename(&temp, &dest).map_err(|e| format!("Could not save {}: {e}", dest.display()))?;
    Ok(dest)
}

fn get_json(client: &Client, url: &str, license_key: Option<&str>, install_id: &str) -> Result<Value, String> {
    let mut request = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .header("X-Install-Id", install_id);
    if let Some(key) = license_key {
        request = request.header("X-License-Key", key);
    }
    let response = request.send().map_err(|e| format!("Could not reach the script catalog: {e}"))?;
    let status = response.status();
    let text = response.text().map_err(|e| format!("Bad response from the catalog: {e}"))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        if text.contains("license_expired") {
            return Err("License expired. Premium installs are unavailable until it is renewed.".into());
        }
        return Err("License key was rejected. Check it and press Save.".into());
    }
    if !status.is_success() {
        return Err(format!("Script catalog returned HTTP {status}."));
    }
    serde_json::from_str(&text).map_err(|_| "Script catalog returned data that could not be read.".into())
}

fn post_json(client: &Client, url: &str) -> Result<Value, String> {
    let response = client
        .post(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("Could not reach Discord login: {e}"))?;
    let status = response.status();
    let text = response.text().map_err(|e| format!("Bad response from Discord login: {e}"))?;
    if !status.is_success() {
        return Err(format!("Discord login returned HTTP {status}."));
    }
    serde_json::from_str(&text).map_err(|_| "Discord login returned data that could not be read.".into())
}

fn parse_scripts(body: &Value, entitled: bool) -> Result<Vec<RemoteScript>, String> {
    let items = body
        .get("scripts")
        .and_then(|v| v.as_array())
        .ok_or("Script catalog has no script list.")?;
    let mut out = Vec::new();
    for item in items {
        if item.get("is_launcher").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if id.is_empty() {
            continue;
        }
        let path = item
            .get("path")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("url").and_then(|v| v.as_str()))
            .unwrap_or("");
        let file_name = file_name_from_path(path);
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let name = if name.is_empty() {
            file_name.trim_end_matches(".simba").to_string()
        } else {
            name
        };
        if name.to_lowercase().contains("launcher") || file_name.to_lowercase().contains("launcher") {
            continue;
        }
        let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("premium").to_lowercase();
        let (on_sale, discount_pct) = parse_sale(item);
        let (price_usd, list_price_usd) = parse_prices(item);
        out.push(RemoteScript {
            id,
            name,
            description: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            file_name,
            revision: item.get("revision").and_then(|v| v.as_i64()).unwrap_or(1),
            entitled: entitled || kind == "free",
            kind,
            on_sale,
            discount_pct,
            price_usd,
            list_price_usd,
            activities: parse_activities(item),
            features: parse_features(item),
        });
    }
    Ok(out)
}

fn apply_entitlements(scripts: &mut [RemoteScript], me: &Value) -> String {
    if me.get("valid").and_then(|v| v.as_bool()) == Some(false) {
        let err = me.get("error").and_then(|v| v.as_str()).unwrap_or("invalid");
        return format!("License was not accepted ({err}).");
    }
    let entitled_ids: Vec<String> = me
        .get("scripts")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    for script in scripts.iter_mut() {
        if entitled_ids.iter().any(|id| id == &script.id) {
            script.entitled = true;
        }
    }
    let expires = me.get("expires_at").and_then(|v| v.as_str()).unwrap_or("").trim();
    if expires.is_empty() || expires == "null" {
        "License accepted.".to_string()
    } else {
        format!("License accepted. Expires {}.", &expires[..expires.len().min(10)])
    }
}

fn parse_prices(item: &Value) -> (Option<f64>, Option<f64>) {
    let price = positive_amount(item.get("price_usd"));
    let list = positive_amount(item.get("list_price_usd"));
    (price, list)
}

fn positive_amount(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| v.as_f64()).filter(|amount| amount.is_finite() && *amount > 0.0)
}

/// A script is on sale when the catalog says so and the discounted price is below the list price.
fn parse_sale(item: &Value) -> (bool, u32) {
    let flagged = item.get("on_sale").and_then(|v| v.as_bool()).unwrap_or(false);
    let pct = item.get("discount_pct").and_then(|v| v.as_u64()).unwrap_or(0).min(100) as u32;
    let list = item.get("list_price_usd").and_then(|v| v.as_f64());
    let price = item.get("price_usd").and_then(|v| v.as_f64());
    let discounted = match (list, price) {
        (Some(list), Some(price)) => list > price,
        _ => flagged,
    };
    (flagged && discounted, pct)
}

fn parse_features(item: &Value) -> Vec<String> {
    let mut features = Vec::new();
    let raw: Vec<String> = match item.get("features") {
        Some(Value::Array(items)) => items.iter().filter_map(|value| value.as_str()).map(|value| value.to_string()).collect(),
        Some(Value::String(value)) => value.split(|c: char| c == ',' || c == '|' || c == ';').map(|value| value.to_string()).collect(),
        _ => Vec::new(),
    };
    for value in raw {
        let lower = value.trim().to_lowercase();
        let id = match lower.as_str() {
            "farm" | "farming" | "farm_run" | "farmruns" | "farming_run" | "farming_runs" | "farm_runs" => "farm_runs",
            "bird" | "birds" | "birdhouse" | "birdhouse_run" | "birdhouse_runs" | "bh" | "bh_runs" | "birdhouses" => {
                "birdhouses"
            }
            _ => continue,
        };
        if !features.iter().any(|saved: &String| saved == id) {
            features.push(id.to_string());
        }
    }
    features
}

fn parse_activities(item: &Value) -> Vec<String> {
    match item.get("activities") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|value| value.as_str())
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty())
            .collect(),
        Some(Value::String(value)) => value
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn file_name_from_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/").replace("%20", " ");
    let name = path.rsplit('/').next().unwrap_or("").trim().to_string();
    if name.is_empty() || name.ends_with('/') {
        return String::new();
    }
    if name.to_lowercase().ends_with(".simba") {
        name
    } else {
        format!("{name}.simba")
    }
}

pub fn read_revision(path: &Path) -> Option<i64> {
    let text = fs::read_to_string(path).ok()?;
    let head = &text[..text.len().min(8192)];
    let marker = "SCRIPT_REVISION";
    let idx = head.find(marker)?;
    let rest = &head[idx + marker.len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn new_install_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mixed = nanos ^ ((std::process::id() as u128) << 64) ^ 0x6a4e_9c1d_33b7_0e55;
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (mixed >> 96) as u32,
        (mixed >> 80) as u16,
        (mixed >> 64) as u16 & 0x0fff,
        ((mixed >> 48) as u16 & 0x3fff) | 0x8000,
        mixed as u64 & 0x0000_ffff_ffff_ffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_and_skips_the_launcher() {
        let body = serde_json::json!({
            "scripts": [
                {"id": "1", "name": "AIO Agility", "description": "runs", "path": "AIO Agility.simba", "revision": 20, "type": "premium", "is_launcher": false, "activities": ["agility"]},
                {"id": "2", "name": "Launcher", "path": "bash-launcher.simba", "type": "free", "is_launcher": true}
            ]
        });
        let scripts = parse_scripts(&body, false).unwrap();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].file_name, "AIO Agility.simba");
        assert!(!scripts[0].entitled);
        assert_eq!(scripts[0].kind, "premium");
        assert_eq!(scripts[0].activities, vec!["agility".to_string()]);
        assert!(!scripts[0].on_sale);
    }

    #[test]
    fn sale_needs_a_lower_price() {
        let body = serde_json::json!({
            "scripts": [
                {"id": "1", "name": "On sale", "path": "a.simba", "type": "premium", "on_sale": true, "discount_pct": 20, "list_price_usd": 10.0, "price_usd": 8.0},
                {"id": "2", "name": "Full price", "path": "b.simba", "type": "premium", "on_sale": true, "list_price_usd": 10.0, "price_usd": 10.0}
            ]
        });
        let scripts = parse_scripts(&body, false).unwrap();
        assert!(scripts[0].on_sale);
        assert_eq!(scripts[0].discount_pct, 20);
        assert_eq!(scripts[0].price_usd, Some(8.0));
        assert_eq!(scripts[0].list_price_usd, Some(10.0));
        assert!(!scripts[1].on_sale);
    }

    #[test]
    fn included_runs_come_from_features() {
        let body = serde_json::json!({
            "scripts": [
                {"id": "1", "name": "AIO Agility", "path": "a.simba", "type": "premium", "features": ["farm_runs", "birdhouses"]},
                {"id": "2", "name": "Runs", "path": "b.simba", "type": "free", "features": ["farming"]}
            ]
        });
        let scripts = parse_scripts(&body, false).unwrap();
        assert_eq!(scripts[0].features, vec!["farm_runs".to_string(), "birdhouses".to_string()]);
        assert_eq!(scripts[1].features, vec!["farm_runs".to_string()]);
    }

    #[test]
    fn free_scripts_are_entitled_without_a_key() {
        let body = serde_json::json!({
            "scripts": [{"id": "9", "name": "Free Agility", "path": "Free Agility.simba", "type": "free", "revision": 3}]
        });
        let scripts = parse_scripts(&body, false).unwrap();
        assert!(scripts[0].entitled);
    }
}
