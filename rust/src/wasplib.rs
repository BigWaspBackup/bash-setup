//! The shared script settings file at `Configs/wasplib.json`.
//! Remote input is read by BashLib when a script starts.

use std::{fs, path::Path};

use serde_json::{Map, Value};

const ENABLED: &str = "enabled";
const BLOCK: &str = "block_real_input";

#[derive(Clone, Copy)]
pub struct RemoteInput {
    pub enabled: bool,
    pub block_real_input: bool,
}

impl Default for RemoteInput {
    fn default() -> Self {
        Self {
            enabled: true,
            block_real_input: true,
        }
    }
}

pub fn load(path: &Path) -> RemoteInput {
    let Ok(text) = fs::read_to_string(path) else {
        return RemoteInput::default();
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return RemoteInput::default();
    };
    let Some(remote) = root.get("remote_input").and_then(Value::as_object) else {
        return RemoteInput::default();
    };
    RemoteInput {
        enabled: bool_or(remote.get(ENABLED), true),
        block_real_input: bool_or(remote.get(BLOCK), true),
    }
}

pub fn save(path: &Path, settings: RemoteInput) -> Result<(), String> {
    let mut root = if path.exists() {
        let text = fs::read_to_string(path).map_err(|_| "Could not read remote input settings.".to_string())?;
        if text.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(&text).map_err(|_| "Could not read remote input settings.".to_string())?
        }
    } else {
        Value::Object(Map::new())
    };
    if !root.is_object() {
        return Err("Could not read remote input settings.".into());
    }
    let remote = root
        .as_object_mut()
        .expect("checked object")
        .entry("remote_input")
        .or_insert_with(|| Value::Object(Map::new()));
    if !remote.is_object() {
        *remote = Value::Object(Map::new());
    }
    let remote = remote.as_object_mut().expect("just set object");
    remote.insert(ENABLED.into(), Value::Bool(settings.enabled));
    remote.insert(BLOCK.into(), Value::Bool(settings.block_real_input));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "Could not save remote input settings.".to_string())?;
    }
    let body = serde_json::to_string_pretty(&root).map_err(|_| "Could not save remote input settings.".to_string())?;
    fs::write(path, body + "\n").map_err(|_| "Could not save remote input settings.".to_string())
}

fn bool_or(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}
