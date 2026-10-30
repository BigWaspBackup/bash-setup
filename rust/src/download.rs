use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use reqwest::blocking::Client;

use crate::{log::log_line, send_status, ProgressSender};

pub fn client(user_agent: &str) -> Result<Client, String> {
    Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to start the download client: {e}"))
}

/// Client that does not follow redirects (used to resolve GitHub `releases/latest`).
pub fn client_no_redirect(user_agent: &str) -> Result<Client, String> {
    Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to start the download client: {e}"))
}

pub fn get_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Network error fetching {url}: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|e| format!("Failed to read response from {url}: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {} from {}: {}",
            status,
            url,
            text.lines().next().unwrap_or("").trim()
        ));
    }
    Ok(text)
}

/// Download `url` to `dest`, streaming to a `.part` file first. Reports
/// progress through `tx` when the server sends a content length.
pub fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    tx: Option<&ProgressSender>,
    progress_range: (f32, f32),
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| url.to_string());
    if let Some(tx) = tx {
        send_status(tx, format!("Downloading {name}..."));
    }
    log_line(&format!("download {url} -> {}", dest.display()));

    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to download {name}: network error: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Failed to download {name}: HTTP {} - {}",
            status,
            body.lines().next().unwrap_or("").trim()
        ));
    }

    let total = response.content_length();
    let part = dest.with_extension(format!(
        "{}part",
        dest.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    let mut file =
        File::create(&part).map_err(|e| format!("Failed to create {}: {e}", part.display()))?;

    let mut buffer = [0u8; 64 * 1024];
    let mut received: u64 = 0;
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|e| format!("Failed while downloading {name}: {e}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("Failed to write {}: {e}", part.display()))?;
        received += read as u64;
        if let (Some(tx), Some(total)) = (tx, total) {
            if total > 0 {
                let frac = received as f32 / total as f32;
                let p = progress_range.0 + (progress_range.1 - progress_range.0) * frac;
                let _ = tx.send(crate::ProgressMessage::Progress(p));
            }
        }
    }
    drop(file);

    if received == 0 {
        let _ = fs::remove_file(&part);
        return Err(format!("Failed to download {name}: empty response"));
    }

    if dest.exists() {
        fs::remove_file(dest).map_err(|e| format!("Failed to replace {}: {e}", dest.display()))?;
    }
    fs::rename(&part, dest).map_err(|e| format!("Failed to move {} into place: {e}", part.display()))?;
    Ok(())
}

/// Download a small text file (the launcher script) into memory.
pub fn download_text(client: &Client, url: &str) -> Result<String, String> {
    get_text(client, url)
}
