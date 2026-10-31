//! Installed-vs-latest version detection for every component BASH.exe manages.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::blocking::Client;

use crate::{download, log::log_line, paths::SimbaPaths, state};

pub const USER_AGENT: &str = "B.A.S.H-Bootstrapper";

pub const SRLB_REPO: (&str, &str) = ("BigWaspBackup", "SRL-B");
pub const BASHLIB_REPO: (&str, &str) = ("BigWaspBackup", "BashLib");
pub const SETUP_REPO: (&str, &str) = ("BigWaspBackup", "bash-setup");
/// First repo that has a release wins. The B.A.S.H Simba fork is preferred;
/// Torwent's build is the fallback until the fork publishes a release.
/// Simba download sources, first one that has a release wins.
/// `bash-simba` is private until the B.A.S.H build is ready for release, so
/// anonymous lookups 404 and the Torwent 1400 release is used meanwhile.
pub const SIMBA_REPOS: &[(&str, &str)] = &[("BigWaspBackup", "bash-simba"), ("Torwent", "Simba")];
pub const SIMBA_ASSET: &str = "Simba-Win64.exe";
/// Published launcher binary on bash-setup releases. Installed as BASH.exe.
pub const BOOTSTRAPPER_ASSET: &str = "BASH.exe";

pub const LAUNCHER_RAW_URL: &str =
    "https://raw.githubusercontent.com/BigAussie/BASH/main/B.A.S.H%20Launcher.simba";
pub const PREMIUM_API: &str = "https://bash-premium-api.cdn-gate.workers.dev";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub owner: String,
    pub repo: String,
    pub tag: String,
}

impl Release {
    pub fn zip_url(&self) -> String {
        format!(
            "https://github.com/{}/{}/archive/refs/tags/{}.zip",
            self.owner, self.repo, self.tag
        )
    }

    pub fn asset_url(&self, asset: &str) -> String {
        format!(
            "https://github.com/{}/{}/releases/download/{}/{}",
            self.owner, self.repo, self.tag, asset
        )
    }

    pub fn version(&self) -> String {
        normalize_version(&self.tag)
    }
}

pub fn normalize_version(tag: &str) -> String {
    let t = tag.trim();
    t.strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .unwrap_or(t)
        .to_string()
}

/// Numeric dotted-version compare ("8.4.14" > "8.4.9"). Non-numeric parts
/// count as 0; missing parts count as 0.
pub fn version_parts(v: &str) -> Vec<u64> {
    normalize_version(v)
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let a = version_parts(candidate);
    let b = version_parts(current);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Resolve `github.com/{owner}/{repo}/releases/latest` without the API (no
/// rate limit). Returns `Ok(None)` when the repo has no releases.
pub fn latest_release(no_redirect: &Client, owner: &str, repo: &str) -> Result<Option<Release>, String> {
    let url = format!("https://github.com/{owner}/{repo}/releases/latest");
    let response = no_redirect
        .get(&url)
        .send()
        .map_err(|e| format!("Network error fetching {url}: {e}"))?;
    let status = response.status();
    if status.is_redirection() {
        if let Some(location) = response.headers().get(reqwest::header::LOCATION) {
            let location = location.to_str().unwrap_or("");
            if let Some(idx) = location.find("/releases/tag/") {
                let tag = &location[idx + "/releases/tag/".len()..];
                let tag = tag.split(['?', '#']).next().unwrap_or(tag);
                if !tag.is_empty() {
                    return Ok(Some(Release {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                        tag: tag.to_string(),
                    }));
                }
            }
            // Redirected to /releases: no releases yet.
            return Ok(None);
        }
        return Ok(None);
    }
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if status.is_success() {
        // GitHub served the page directly (no release -> /releases listing).
        return Ok(None);
    }
    Err(format!("HTTP {status} from {url}"))
}

/// Notes from the newest GitHub release, with markdown headings and backticks removed.
pub fn latest_release_notes(owner: &str, repo: &str) -> Result<String, String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to start the release client: {e}"))?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let text = download::get_text(&client, &url)?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Could not read the release notes: {e}"))?;
    let body = value.get("body").and_then(|item| item.as_str()).unwrap_or("").trim();
    if body.is_empty() {
        return Err("This release has no notes.".into());
    }
    Ok(plain_release_notes(body))
}

fn plain_release_notes(body: &str) -> String {
    let mut lines = Vec::new();
    let mut blank = false;
    for line in body.lines() {
        let mut line = line.trim().to_string();
        if line.starts_with('#') {
            line = line.trim_start_matches('#').trim().to_string();
        }
        line = line.replace('`', "");
        if line.is_empty() {
            if !blank && !lines.is_empty() {
                lines.push(String::new());
                blank = true;
            }
            continue;
        }
        blank = false;
        lines.push(line);
    }
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRelease {
    pub tag: String,
    pub version: String,
    pub prerelease: bool,
}

impl ListedRelease {
    pub fn label(&self) -> String {
        if self.prerelease {
            format!("{} dev", self.version)
        } else {
            self.version.clone()
        }
    }
}

/// The release a library should be on. `track` is empty for the newest normal
/// release, "dev" for the newest pre-release, or a tag such as "v8.4.10".
#[derive(Debug, Clone)]
pub struct LibraryTarget {
    pub tag: String,
    pub version: String,
    pub zip_url: String,
    pub stable_version: String,
    pub dev_tag: String,
}

pub fn tag_zip_url(owner: &str, repo: &str, tag: &str) -> String {
    format!("https://github.com/{owner}/{repo}/archive/refs/tags/{tag}.zip")
}

pub fn list_releases(client: &Client, owner: &str, repo: &str) -> Result<Vec<ListedRelease>, String> {
    let mut all = Vec::new();
    for page in 1..=3 {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=100&page={page}");
        let text = download::get_text(client, &url)?;
        let batch = parse_release_list(&text)?;
        let done = batch.len() < 100;
        all.extend(batch);
        if done {
            break;
        }
    }
    Ok(all)
}

pub fn parse_release_list(text: &str) -> Result<Vec<ListedRelease>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Could not read the release list: {e}"))?;
    let Some(list) = value.as_array() else {
        let message = value.get("message").and_then(|item| item.as_str()).unwrap_or("GitHub did not return a release list.");
        return Err(message.to_string());
    };
    let mut releases = Vec::new();
    for item in list {
        if item.get("draft").and_then(|draft| draft.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(tag) = item.get("tag_name").and_then(|tag| tag.as_str()) else {
            continue;
        };
        if tag.is_empty() {
            continue;
        }
        releases.push(ListedRelease {
            tag: tag.to_string(),
            version: normalize_version(tag),
            prerelease: item.get("prerelease").and_then(|flag| flag.as_bool()).unwrap_or(false),
        });
    }
    Ok(releases)
}

pub fn library_target(
    client: &Client,
    no_redirect: &Client,
    owner: &str,
    repo: &str,
    track: &str,
) -> Result<Option<LibraryTarget>, String> {
    let stable = latest_release(no_redirect, owner, repo)?;
    let stable_version = stable.as_ref().map(|release| release.version()).unwrap_or_default();
    if track.is_empty() {
        let Some(release) = stable else {
            return Ok(None);
        };
        return Ok(Some(LibraryTarget {
            version: release.version(),
            zip_url: release.zip_url(),
            stable_version: release.version(),
            tag: release.tag,
            dev_tag: String::new(),
        }));
    }
    if track == "dev" {
        let dev = list_releases(client, owner, repo)?.into_iter().find(|release| release.prerelease);
        let Some(dev) = dev else {
            return Ok(None);
        };
        return Ok(Some(LibraryTarget {
            zip_url: tag_zip_url(owner, repo, &dev.tag),
            stable_version,
            tag: dev.tag.clone(),
            version: dev.version,
            dev_tag: dev.tag,
        }));
    }
    Ok(Some(LibraryTarget {
        version: normalize_version(track),
        zip_url: tag_zip_url(owner, repo, track),
        stable_version,
        tag: track.to_string(),
        dev_tag: String::new(),
    }))
}

pub fn latest_simba_release(no_redirect: &Client) -> Result<Option<Release>, String> {
    let mut last_err = None;
    for (owner, repo) in SIMBA_REPOS {
        match latest_release(no_redirect, owner, repo) {
            Ok(Some(release)) => return Ok(Some(release)),
            Ok(None) => continue,
            Err(e) => {
                log_line(&format!("latest_simba_release {owner}/{repo}: {e}"));
                last_err = Some(e);
            }
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

/// Version from the first `## [x.y.z](...)` line of a library CHANGELOG.md,
/// which is how the launcher script detects installed versions as well.
pub fn installed_lib_version(include_dir: &Path) -> Option<String> {
    let text = fs::read_to_string(include_dir.join("CHANGELOG.md")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }
        let start = line.find('[')? + 1;
        let end = line[start..].find(']')? + start;
        let version = line[start..end].trim();
        if !version.is_empty() {
            return Some(normalize_version(version));
        }
    }
    None
}

pub fn launcher_revision_from_text(text: &str) -> Option<u32> {
    let head: String = text.chars().take(8192).collect();
    for quote in ["'", "\""] {
        let marker = format!("SCRIPT_REVISION := {quote}");
        if let Some(idx) = head.find(&marker) {
            let rest = &head[idx + marker.len()..];
            let end = rest.find(quote)?;
            if let Ok(rev) = rest[..end].trim().parse::<u32>() {
                return Some(rev);
            }
        }
    }
    None
}

pub fn installed_launcher_revision(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    launcher_revision_from_text(&text)
}

pub struct RemoteLauncher {
    pub revision: u32,
    pub body: String,
    pub source: &'static str,
}

/// Newest launcher from GitHub raw and the premium API (same two sources the
/// launcher script itself checks).
pub fn remote_launcher(client: &Client) -> Result<RemoteLauncher, String> {
    let mut best: Option<RemoteLauncher> = None;

    let github_url = format!(
        "{LAUNCHER_RAW_URL}?t={}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    match download::download_text(client, &github_url) {
        Ok(body) if body.len() >= 500 => {
            if let Some(revision) = launcher_revision_from_text(&body) {
                best = Some(RemoteLauncher {
                    revision,
                    body,
                    source: "GitHub",
                });
            }
        }
        Ok(_) => log_line("launcher GitHub raw body too small"),
        Err(e) => log_line(&format!("launcher GitHub raw: {e}")),
    }

    let api_rev = download::get_text(client, &format!("{PREMIUM_API}/v1/launcher"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json.get("revision").and_then(|r| r.as_u64()))
        .map(|r| r as u32)
        .unwrap_or(0);

    if api_rev > best.as_ref().map(|b| b.revision).unwrap_or(0) {
        match download::download_text(client, &format!("{PREMIUM_API}/v1/launcher/download")) {
            Ok(body) if body.len() >= 500 => {
                let revision = launcher_revision_from_text(&body).unwrap_or(api_rev);
                best = Some(RemoteLauncher {
                    revision: revision.max(api_rev),
                    body,
                    source: "API",
                });
            }
            Ok(_) => log_line("launcher API body too small"),
            Err(e) => log_line(&format!("launcher API download: {e}")),
        }
    }

    best.ok_or_else(|| "Could not download the launcher from GitHub or the B.A.S.H API.".to_string())
}

pub fn bootstrapper_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Record this BASH.exe against the latest bash-setup release.
///
/// A newer build is downloaded as `BASH.exe.new` only when this process is the
/// installed copy. `just_swapped` means that file was already replaced on disk
/// for the next start, so this process does not download it again.
pub fn refresh_app_version(paths: &SimbaPaths, state: &mut state::BashState, just_swapped: bool) {
    state.bootstrapper.installed = bootstrapper_version().to_string();
    state.bootstrapper_version = bootstrapper_version().to_string();
    let fetched = match download::client_no_redirect(USER_AGENT) {
        Ok(no_redirect) => latest_release(&no_redirect, SETUP_REPO.0, SETUP_REPO.1),
        Err(e) => Err(e),
    };
    match fetched {
        Ok(Some(release)) => {
            let latest = release.version();
            state.bootstrapper.latest = latest.clone();
            if just_swapped {
                state.bootstrapper_status = "restart".to_string();
                log_line("launcher update swapped, restart to finish");
                return;
            }
            if !is_newer(&latest, bootstrapper_version()) {
                state.bootstrapper_status = "up_to_date".to_string();
                return;
            }
            state.bootstrapper_status = "available".to_string();
            stage_launcher_update(paths, &release);
        }
        Ok(None) => {
            log_line("launcher: no release found");
            state.bootstrapper_status = if just_swapped {
                "restart".to_string()
            } else {
                "offline".to_string()
            };
        }
        Err(e) => {
            log_line(&format!("launcher check: {e}"));
            state.bootstrapper_status = if just_swapped {
                "restart".to_string()
            } else {
                "offline".to_string()
            };
        }
    }
}

fn stage_launcher_update(paths: &SimbaPaths, release: &Release) {
    let Ok(exe) = env::current_exe() else {
        return;
    };
    if exe != paths.bootstrapper_exe() {
        log_line("launcher update available; this is not the installed BASH.exe, so it was not downloaded");
        return;
    }
    let new = PathBuf::from(format!("{}.new", exe.display()));
    if new.exists() {
        return;
    }
    let Ok(client) = download::client(USER_AGENT) else {
        return;
    };
    if let Err(e) = download::download_to_file(
        &client,
        &release.asset_url(BOOTSTRAPPER_ASSET),
        &new,
        None,
        (0.0, 0.0),
    ) {
        log_line(&format!("self-update download: {e}"));
        let _ = fs::remove_file(&new);
    } else {
        log_line(&format!("self-update staged: {}", release.version()));
    }
}

/// Quiet Simba / SRL-B / BashLib check. Updates the status line and does not install anything.
pub fn refresh_library_record(paths: &SimbaPaths) {
    let mut state = state::BashState::load(&paths.state_file());
    let mut offline = false;
    let Ok(no_redirect) = download::client_no_redirect(USER_AGENT) else {
        state.touch();
        state.play_seconds = state::BashState::load(&paths.state_file()).play_seconds;
        state::write_library_status(&mut state, true);
        let _ = state.save(&paths.state_file());
        return;
    };
    let Ok(client) = download::client(USER_AGENT) else {
        state.touch();
        state.play_seconds = state::BashState::load(&paths.state_file()).play_seconds;
        state::write_library_status(&mut state, true);
        let _ = state.save(&paths.state_file());
        return;
    };

    match latest_simba_release(&no_redirect) {
        Ok(Some(release)) => {
            state.simba.latest = release.version();
            if !paths.simba_exe().exists() {
                state.simba.installed.clear();
            } else if state.simba_tag.is_empty() {
                state.simba_tag = release.tag.clone();
                state.simba.installed = release.version();
            } else if state.simba_tag != release.tag {
                state.simba.installed = normalize_version(&state.simba_tag);
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

    for (name, repo) in [(crate::paths::SRLB_DIR, SRLB_REPO), (crate::paths::BASHLIB_DIR, BASHLIB_REPO)] {
        let include = paths.include(name);
        let installed = installed_lib_version(&include).unwrap_or_default();
        let component = if name == crate::paths::SRLB_DIR {
            &mut state.srlb
        } else {
            &mut state.bashlib
        };
        component.installed = installed.clone();
        component.dev_checkout = false;
        match library_target(&client, &no_redirect, repo.0, repo.1, &component.track) {
            Ok(Some(target)) => {
                component.latest = target.stable_version;
                if component.track == "dev" {
                    component.dev_tag = target.dev_tag;
                }
            }
            Ok(None) => log_line(&format!("{name}: no release found for the selected version")),
            Err(e) => {
                log_line(&format!("{name} check: {e}"));
                offline = true;
            }
        }
    }

    refresh_app_version(paths, &mut state, false);
    state.touch();
    let fresh = state::BashState::load(&paths.state_file());
    state.accounts = fresh.accounts;
    state.prompted_profile = fresh.prompted_profile;
    state.prompted_setup = fresh.prompted_setup;
    state.discord_user = fresh.discord_user;
    state.play_seconds = fresh.play_seconds;
    state.favorites = fresh.favorites;
    state.anchor_output = fresh.anchor_output;
    keep_library_choice(&mut state.srlb, &fresh.srlb);
    keep_library_choice(&mut state.bashlib, &fresh.bashlib);
    state::write_library_status(&mut state, offline);
    if let Err(e) = state.save(&paths.state_file()) {
        log_line(&format!("library check save: {e}"));
    } else {
        log_line(&format!("library check: {}", state.library_status));
    }
}

fn keep_library_choice(component: &mut state::ComponentState, fresh: &state::ComponentState) {
    if fresh.track == component.track && fresh.installed_tag == component.installed_tag {
        return;
    }
    component.track = fresh.track.clone();
    component.installed_tag = fresh.installed_tag.clone();
    if !fresh.installed.is_empty() {
        component.installed = fresh.installed.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::parse_release_list;

    #[test]
    fn release_list_keeps_old_versions_and_marks_dev() {
        let text = r#"[
            {"tag_name":"v8.4.15-dev","prerelease":true,"draft":false},
            {"tag_name":"v8.4.14","prerelease":false,"draft":false},
            {"tag_name":"v8.4.13","prerelease":false,"draft":true}
        ]"#;
        let releases = parse_release_list(text).unwrap();
        assert_eq!(releases.len(), 2);
        assert!(releases[0].prerelease);
        assert_eq!(releases[0].version, "8.4.15-dev");
        assert_eq!(releases[1].label(), "8.4.14");
    }
}
