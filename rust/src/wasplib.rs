//! The shared script settings file at `Configs/wasplib.json`.
//! Remote input and crash recording are read by BashLib when a script starts.

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

const VIDEO_SECONDS: &str = "time";

#[derive(Clone, Copy)]
pub struct CrashRecording {
    pub enabled: bool,
    pub seconds: i32,
}

impl Default for CrashRecording {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 30,
        }
    }
}

pub fn load_recording(path: &Path) -> CrashRecording {
    let Ok(text) = fs::read_to_string(path) else {
        return CrashRecording::default();
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return CrashRecording::default();
    };
    let Some(video) = root.get("video").and_then(Value::as_object) else {
        return CrashRecording::default();
    };
    let seconds = int_or(video.get(VIDEO_SECONDS), 30).clamp(5, 99) as i32;
    CrashRecording {
        enabled: bool_or(video.get(ENABLED), false),
        seconds,
    }
}

pub fn save_recording(path: &Path, settings: CrashRecording) -> Result<(), String> {
    let mut root = read_root(path, "Could not read crash recording settings.")?;
    let video = root
        .as_object_mut()
        .expect("checked object")
        .entry("video")
        .or_insert_with(|| Value::Object(Map::new()));
    if !video.is_object() {
        *video = Value::Object(Map::new());
    }
    let video = video.as_object_mut().expect("just set object");
    video.insert(ENABLED.into(), Value::Bool(settings.enabled));
    video.insert(
        VIDEO_SECONDS.into(),
        Value::from(settings.seconds.clamp(5, 99)),
    );
    write_root(path, &root, "Could not save crash recording settings.")
}

fn read_root(path: &Path, error: &str) -> Result<Value, String> {
    let root = if path.exists() {
        let text = fs::read_to_string(path).map_err(|_| error.to_string())?;
        if text.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(&text).map_err(|_| error.to_string())?
        }
    } else {
        Value::Object(Map::new())
    };
    if !root.is_object() {
        return Err(error.into());
    }
    Ok(root)
}

fn write_root(path: &Path, root: &Value, error: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| error.to_string())?;
    }
    let body = serde_json::to_string_pretty(root).map_err(|_| error.to_string())?;
    fs::write(path, body + "\n").map_err(|_| error.to_string())
}

fn bool_or(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn int_or(value: Option<&Value>, fallback: i64) -> i64 {
    value
        .and_then(|item| item.as_i64().or_else(|| item.as_f64().map(|number| number as i64)))
        .unwrap_or(fallback)
}
