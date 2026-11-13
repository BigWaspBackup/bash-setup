//! Per-account AI Chat setup. Writes `Configs/aichat-{account}.json`, the file `TAIChatbot` reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{self, ComboBox, RichText, ScrollArea};
use serde_json::{json, Map, Value};

use crate::paths::SimbaPaths;

const SERVICES: [&str; 7] = ["Gemini", "Groq", "DeepSeek", "Mistral", "OpenRouter", "ChatGPT", "Local"];
const STYLES: [&str; 4] = ["Normal", "Friendly", "Casual", "Minimal"];
const SYSTEM_PROMPT: &str = "You are a real human player in Old School RuneScape. Keep replies short and casual.";
const TEST_MESSAGE: &str = "Respond with exactly: OK";

pub struct Dialog {
    open: bool,
    wants_open: bool,
    account: String,
    enabled: bool,
    service: String,
    api_key: String,
    show_key: bool,
    model: String,
    style: usize,
    memory: bool,
    log_conversations: bool,
    nearby: String,
    interactions: String,
    cooldown_minutes: String,
    custom: String,
    ignore: String,
    message_cooldown: i64,
    max_log_count: i64,
    trust: bool,
    suspicious: i64,
    anti_detection: bool,
    min_response: i64,
    max_response: i64,
    typing_variation: bool,
    human_delays: bool,
    response_filtering: bool,
    extra: Map<String, Value>,
    note: String,
    error: String,
    test_rx: Option<Receiver<String>>,
}

struct Clicked {
    save: bool,
    cancel: bool,
    test: bool,
    defaults: bool,
    review: bool,
    delete: bool,
}

impl Dialog {
    pub fn closed() -> Self {
        Self::blank()
    }

    pub fn request(&mut self) {
        self.wants_open = true;
    }

    pub fn load(paths: &SimbaPaths, account: &str) -> Self {
        let mut dialog = Self::blank();
        dialog.account = account.trim().to_string();
        let path = read_path(paths, account);
        let Ok(text) = fs::read_to_string(path) else {
            return dialog;
        };
        if text.trim().is_empty() {
            return dialog;
        }
        let mut map = match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            Ok(Value::Object(map)) => map,
            _ => {
                dialog.error = "AI Chat settings could not be read. Defaults are shown, and Done replaces the file.".into();
                return dialog;
            }
        };
        dialog.enabled = take_bool(&mut map, "enabled", false);
        dialog.service = canonical_service(&take_string(&mut map, "api_service", "Gemini"));
        dialog.api_key = take_string(&mut map, "api_key", "");
        dialog.model = take_string(&mut map, "api_model", "");
        if retired_model(&dialog.model) {
            dialog.model.clear();
        }
        dialog.style = take_i64(&mut map, "response_style", 0).clamp(0, 3) as usize;
        dialog.memory = take_bool(&mut map, "use_conversation_memory", true);
        dialog.log_conversations = take_bool(&mut map, "log_conversations", true);
        dialog.nearby = take_i64(&mut map, "max_nearby_players", 2).clamp(0, 500).to_string();
        dialog.interactions = take_i64(&mut map, "max_interaction_count", 2).clamp(0, 500).to_string();
        dialog.cooldown_minutes = minutes_from_ms(take_i64(&mut map, "interaction_cooldown", 600_000));
        let _ = take_string(&mut map, "activity", "");
        let system = take_string(&mut map, "system_instruction", "");
        let extra = take_string(&mut map, "extra_instructions", "");
        dialog.custom = combine_custom(system, extra);
        let ignore = take_string(&mut map, "ignore_list", "Buying,Selling,Discord");
        dialog.ignore = if ignore.trim().is_empty() { "Buying,Selling,Discord".into() } else { ignore };
        dialog.message_cooldown = take_i64(&mut map, "message_cooldown", 200);
        dialog.max_log_count = take_i64(&mut map, "max_log_count", 15);
        dialog.trust = take_bool(&mut map, "enable_player_trust_system", true);
        dialog.suspicious = take_i64(&mut map, "suspicious_activity_threshold", 2);
        dialog.anti_detection = take_bool(&mut map, "enable_anti_detection", true);
        dialog.min_response = take_i64(&mut map, "min_response_time", 3000);
        dialog.max_response = take_i64(&mut map, "max_response_time", 8000);
        dialog.typing_variation = take_bool(&mut map, "typing_variation", true);
        dialog.human_delays = take_bool(&mut map, "human_like_delays", true);
        dialog.response_filtering = take_bool(&mut map, "enable_response_filtering", true);
        dialog.extra = map;
        dialog
    }

    fn blank() -> Self {
        Self {
            open: false,
            wants_open: false,
            account: String::new(),
            enabled: false,
            service: "Gemini".into(),
            api_key: String::new(),
            show_key: false,
            model: String::new(),
            style: 0,
            memory: true,
            log_conversations: true,
            nearby: "2".into(),
            interactions: "2".into(),
            cooldown_minutes: "10".into(),
            custom: String::new(),
            ignore: "Buying,Selling,Discord".into(),
            message_cooldown: 200,
            max_log_count: 15,
            trust: true,
            suspicious: 2,
            anti_detection: true,
            min_response: 3000,
            max_response: 8000,
            typing_variation: true,
            human_delays: true,
            response_filtering: true,
            extra: Map::new(),
            note: String::new(),
            error: String::new(),
            test_rx: None,
        }
    }

    fn apply_defaults(&mut self) {
        self.style = 0;
        self.memory = true;
        self.nearby = "2".into();
        self.interactions = "4".into();
        self.cooldown_minutes = "10".into();
        self.message_cooldown = 200;
        self.log_conversations = true;
        self.ignore = "Buying,Selling,Discord".into();
    }

    pub fn has_account(&self) -> bool {
        let name = self.account.trim();
        !name.is_empty() && !name.eq_ignore_ascii_case("NoUserNameSelected")
    }

    pub fn status_line(&self) -> String {
        if !self.has_account() {
            return "Add an account to set up AI Chat.".into();
        }
        if self.enabled {
            format!("AI Chat on · {}", self.service)
        } else {
            "AI Chat off".into()
        }
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn save_for(&mut self, paths: &SimbaPaths, account: &str) -> Result<(), String> {
        self.account = account.trim().to_string();
        self.save(paths)
    }

    fn save(&self, paths: &SimbaPaths) -> Result<(), String> {
        if !self.has_account() {
            return Ok(());
        }
        let path = config_file(paths, &self.account);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("Could not create the configs folder: {error}"))?;
        }
        let mut map = self.extra.clone();
        let nearby = parse_count(&self.nearby, 2, 0, 500);
        let interactions = parse_count(&self.interactions, 2, 0, 500);
        let minutes = parse_count(&self.cooldown_minutes, 10, 1, 24 * 60);
        map.insert("enabled".into(), json!(self.enabled));
        map.insert("api_service".into(), json!(self.service));
        map.insert("api_key".into(), json!(self.api_key.trim()));
        map.insert("api_model".into(), json!(self.model.trim()));
        map.insert("response_style".into(), json!(self.style as i64));
        map.insert("ignore_list".into(), json!(normalize_ignore(&self.ignore)));
        map.insert("log_conversations".into(), json!(self.log_conversations));
        map.insert("use_conversation_memory".into(), json!(self.memory));
        map.insert("max_nearby_players".into(), json!(nearby));
        map.insert("max_interaction_count".into(), json!(interactions));
        map.insert("interaction_cooldown".into(), json!(minutes * 60_000));
        map.insert("message_cooldown".into(), json!(self.message_cooldown));
        map.insert("max_log_count".into(), json!(self.max_log_count));
        map.insert("enable_player_trust_system".into(), json!(self.trust));
        map.insert("suspicious_activity_threshold".into(), json!(self.suspicious));
        map.insert("enable_anti_detection".into(), json!(self.anti_detection));
        map.insert("min_response_time".into(), json!(self.min_response));
        map.insert("max_response_time".into(), json!(self.max_response));
        map.insert("typing_variation".into(), json!(self.typing_variation));
        map.insert("human_like_delays".into(), json!(self.human_delays));
        map.insert("enable_response_filtering".into(), json!(self.response_filtering));
        let custom = self.custom.trim();
        map.insert("extra_instructions".into(), json!(custom));
        map.insert("system_instruction".into(), json!(custom));
        let text = serde_json::to_string_pretty(&Value::Object(map))
            .map_err(|error| format!("Could not write AI Chat settings: {error}"))?;
        fs::write(path, format!("{text}\n")).map_err(|error| format!("Could not save AI Chat settings: {error}"))
    }

    fn start_test(&mut self) {
        if self.test_rx.is_some() {
            return;
        }
        let service = self.service.clone();
        let key = self.api_key.clone();
        let model = self.model.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("aichat-test".into())
            .spawn(move || {
                let _ = tx.send(run_test(&service, &key, &model));
            })
            .ok();
        self.test_rx = Some(rx);
        self.note = "Testing the API key...".into();
        self.error.clear();
    }
}

pub fn show(ctx: &egui::Context, paths: &SimbaPaths, dialog: &mut Dialog, account: &str) {
    if std::mem::take(&mut dialog.wants_open) && !dialog.open {
        *dialog = Dialog::load(paths, account);
        dialog.open = true;
    } else if dialog.account.trim() != account.trim() && !dialog.open {
        *dialog = Dialog::load(paths, account);
    }
    if !dialog.open {
        dialog.test_rx = None;
        return;
    }
    poll_test(ctx, dialog);

    let mut clicked = Clicked {
        save: false,
        cancel: false,
        test: false,
        defaults: false,
        review: false,
        delete: false,
    };
    egui::Modal::new(egui::Id::new("bash-aichat")).show(ctx, |ui| {
        ui.set_min_width(720.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("AI Chat").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    clicked.cancel = true;
                }
            });
        });
        ui.add_space(6.0);
        draw(ui, dialog, &mut clicked);
    });

    if clicked.cancel || (!dialog.open && !clicked.save) {
        let account = dialog.account.clone();
        *dialog = Dialog::load(paths, &account);
        dialog.open = false;
    }
    if clicked.defaults {
        dialog.apply_defaults();
    }
    if clicked.test {
        dialog.start_test();
        ctx.request_repaint();
    }
    if clicked.review {
        match review_logs(paths) {
            Ok(message) => {
                dialog.note = message;
                dialog.error.clear();
            }
            Err(message) => {
                dialog.error = message;
                dialog.note.clear();
            }
        }
    }
    if clicked.delete {
        match delete_logs(paths) {
            Ok(message) => {
                if !message.is_empty() {
                    dialog.note = message;
                    dialog.error.clear();
                }
            }
            Err(message) => {
                dialog.error = message;
                dialog.note.clear();
            }
        }
    }
    if clicked.save {
        match dialog.save(paths) {
            Ok(()) => dialog.open = false,
            Err(message) => dialog.error = message,
        }
    }
    if !dialog.open {
        dialog.test_rx = None;
    }
}

fn draw(ui: &mut egui::Ui, dialog: &mut Dialog, clicked: &mut Clicked) {
    ui.set_min_width(720.0);
    ui.label(RichText::new("Replies to public chat using an AI service. Leave this off if you do not want the bot to talk.").weak());
    ui.label(RichText::new(format!("Saved for {}.", dialog.account.trim())).weak());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut dialog.enabled, "Enable AI Chatbot");
        crate::ui::help_tip(ui, "When on, the script watches public chat and can type short replies.");
    });
    if dialog.enabled {
    ui.add_space(8.0);
    ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
        ui.label(RichText::new("API").strong());
        ui.horizontal(|ui| {
            ui.label("AI Service");
            crate::ui::help_tip(ui, "Gemini, Groq, and OpenRouter have free options. Mistral and DeepSeek are cheap. ChatGPT is paid.");
            ComboBox::from_id_salt("aichat-service")
                .width(220.0)
                .selected_text(&dialog.service)
                .show_ui(ui, |ui| {
                    for service in SERVICES {
                        ui.selectable_value(&mut dialog.service, service.to_string(), service);
                    }
                });
        });
        let (help, url) = service_help(&dialog.service);
        if let Some(url) = url {
            if ui.link(help).on_hover_text(help).clicked() {
                crate::ui::open_url(url);
            }
        } else {
            ui.label(RichText::new(help).weak());
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Model (optional)");
            crate::ui::help_tip(
                ui,
                "Leave blank to use the built-in model for this service. Fill this in when a provider retires that model.",
            );
            ui.add(egui::TextEdit::singleline(&mut dialog.model).desired_width(280.0).hint_text(default_model(&dialog.service)));
        });
        ui.horizontal(|ui| {
            ui.label("API Key");
            crate::ui::help_tip(ui, "Your key from the AI provider. Not needed for Local/Ollama.");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.api_key)
                    .password(!dialog.show_key)
                    .desired_width(280.0),
            );
            ui.checkbox(&mut dialog.show_key, "Show");
            let testing = dialog.test_rx.is_some();
            if ui
                .add_enabled(!testing, egui::Button::new(if testing { "Testing..." } else { "Test API" }))
                .on_hover_text("Checks that the selected service and API key work.")
                .clicked()
            {
                clicked.test = true;
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Chat settings").strong());
        ui.horizontal(|ui| {
            ui.label("Custom instructions");
            crate::ui::help_tip(
                ui,
                "Added after the default chat rules. It does not replace them. Example: keep answers vague about exact location.",
            );
        });
        ui.add(egui::TextEdit::multiline(&mut dialog.custom).desired_width(640.0).desired_rows(4));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Reply style");
            crate::ui::help_tip(ui, "Normal is balanced. Friendly is warmer. Casual is slangy. Minimal is very short.");
            ComboBox::from_id_salt("aichat-style")
                .width(220.0)
                .selected_text(STYLES.get(dialog.style).copied().unwrap_or("Normal"))
                .show_ui(ui, |ui| {
                    for (index, style) in STYLES.iter().enumerate() {
                        ui.selectable_value(&mut dialog.style, index, *style);
                    }
                });
            if ui
                .button("Default")
                .on_hover_text("Reset reply limits and style. Leaves the API key and custom instructions as they are.")
                .clicked()
            {
                clicked.defaults = true;
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut dialog.memory, "Remember recent chat with each player");
            crate::ui::help_tip(ui, "Keeps short context so replies make more sense in an ongoing conversation.");
        });
        ui.add_space(8.0);
        number_field(
            ui,
            "Pause if this many players nearby",
            "If more players than this are nearby, the bot stops replying until it is quieter.",
            &mut dialog.nearby,
            0,
            500,
            2,
        );
        number_field(
            ui,
            "Max interactions per player",
            "How many replies one player can get before the cooldown starts.",
            &mut dialog.interactions,
            0,
            500,
            2,
        );
        number_field(
            ui,
            "Cooldown after max (minutes)",
            "How long to wait before chatting with that player again after hitting the max.",
            &mut dialog.cooldown_minutes,
            1,
            24 * 60,
            10,
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Ignore messages containing");
            crate::ui::help_tip(ui, "Skip chat that looks like spam trading ads. Separate words with commas.");
        });
        ui.add(egui::TextEdit::singleline(&mut dialog.ignore).desired_width(520.0));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut dialog.log_conversations, "Save conversations to a log file");
            crate::ui::help_tip(ui, "Writes chats to files you can open with Review Logs.");
            if ui.button("Review Logs").on_hover_text("Open the folder of saved conversation logs.").clicked() {
                clicked.review = true;
            }
            if ui.button("Delete Logs").on_hover_text("Delete all saved AI Chat conversation log files.").clicked() {
                clicked.delete = true;
            }
        });
    });
    }
    if !dialog.error.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(&dialog.error).color(egui::Color32::from_rgb(220, 120, 110)));
    } else if !dialog.note.is_empty() {
        ui.add_space(6.0);
        ui.label(&dialog.note);
    }
    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                clicked.cancel = true;
            }
            if ui.button("Done").on_hover_text("Saves this account's AI Chat settings.").clicked() {
                clicked.save = true;
            }
        });
    });
}

fn number_field(ui: &mut egui::Ui, label: &str, tip: &str, text: &mut String, min: i64, max: i64, default: i64) {
    ui.horizontal(|ui| {
        ui.label(label);
        crate::ui::help_tip(ui, tip);
        let response = ui.add(egui::TextEdit::singleline(text).desired_width(64.0));
        if response.lost_focus() {
            *text = parse_count(text, default, min, max).to_string();
        }
    });
}

fn poll_test(ctx: &egui::Context, dialog: &mut Dialog) {
    let Some(rx) = dialog.test_rx.as_ref() else {
        return;
    };
    let message = match rx.try_recv() {
        Ok(message) => Some(message),
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        Err(TryRecvError::Disconnected) => Some("Could not finish the API test.".into()),
    };
    dialog.test_rx = None;
    if let Some(message) = message {
        if message.starts_with("API Key Test PASSED") {
            dialog.note = message;
            dialog.error.clear();
        } else {
            dialog.error = message;
            dialog.note.clear();
        }
    }
}

fn review_logs(paths: &SimbaPaths) -> Result<String, String> {
    let dir = logs_dir(paths);
    fs::create_dir_all(&dir).map_err(|_| "Could not create the logs folder.".to_string())?;
    if log_files(&dir).is_empty() {
        return Err(format!(
            "No conversation logs found yet.\n\nLogs appear here after chats are saved:\n{}",
            dir.display()
        ));
    }
    if crate::ui::open_url(&dir.display().to_string()) {
        Ok("Opened the conversation logs folder.".into())
    } else {
        Err(format!("Could not open the logs folder.\n\n{}", dir.display()))
    }
}

fn delete_logs(paths: &SimbaPaths) -> Result<String, String> {
    let dir = logs_dir(paths);
    if !dir.is_dir() {
        return Err("No conversation logs folder found yet.".into());
    }
    let files = log_files(&dir);
    if files.is_empty() {
        return Err("There are no conversation logs to delete.".into());
    }
    let count = files.len();
    if !crate::ui::ask_yes_no(&format!("Delete all {count} AI Chat conversation log(s)?\n\nThis cannot be undone.")) {
        return Ok(String::new());
    }
    let deleted = files.iter().filter(|file| fs::remove_file(file).is_ok()).count();
    Ok(format!("Deleted {deleted} conversation log(s)."))
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect()
}

fn config_file(paths: &SimbaPaths, account: &str) -> PathBuf {
    paths.configs().join(config_name(account))
}

fn config_name(account: &str) -> String {
    let account = sanitize_account(account);
    if account.is_empty() || account.eq_ignore_ascii_case("NoUserNameSelected") {
        "aichat.json".into()
    } else {
        format!("aichat-{account}.json")
    }
}

fn read_path(paths: &SimbaPaths, account: &str) -> PathBuf {
    let path = config_file(paths, account);
    let shared = paths.configs().join("aichat.json");
    if path != shared && !path.exists() && shared.exists() {
        shared
    } else {
        path
    }
}

fn sanitize_account(account: &str) -> String {
    account
        .trim()
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

fn combine_custom(system: String, extra: String) -> String {
    let system = system.trim();
    let extra = extra.trim();
    if system.is_empty() {
        extra.to_string()
    } else if extra.is_empty() || extra == system {
        system.to_string()
    } else {
        format!("{system}\n{extra}")
    }
}

fn logs_dir(paths: &SimbaPaths) -> PathBuf {
    paths.data().join("BashLib").join("cache").join("AIChat").join("Logs")
}

fn canonical_service(name: &str) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "groq" => "Groq".into(),
        "deepseek" => "DeepSeek".into(),
        "mistral" => "Mistral".into(),
        "openrouter" | "open router" => "OpenRouter".into(),
        "chatgpt" | "openai" => "ChatGPT".into(),
        "local" | "ollama" => "Local".into(),
        _ => "Gemini".into(),
    }
}

fn service_help(service: &str) -> (&'static str, Option<&'static str>) {
    match service {
        "Groq" => (
            "Click here to learn how to get a Groq API Key",
            Some("https://www.apideck.com/blog/how-to-get-your-groq-api-key"),
        ),
        "DeepSeek" => (
            "Click here to open the DeepSeek API keys page",
            Some("https://platform.deepseek.com/api_keys"),
        ),
        "Mistral" => (
            "Click here to open the Mistral API keys page",
            Some("https://console.mistral.ai/api-keys"),
        ),
        "OpenRouter" => (
            "Click here to open the OpenRouter API keys page",
            Some("https://openrouter.ai/keys"),
        ),
        "ChatGPT" => (
            "Click here to open the OpenAI API keys page",
            Some("https://platform.openai.com/api-keys"),
        ),
        "Local" => ("Local/Ollama does not need an API key", None),
        _ => (
            "Click here to learn how to get a Gemini API Key",
            Some("https://geminiforwork.gwaddons.com/setup-api-keys/create-geminiai-api-key/"),
        ),
    }
}

fn retired_model(model: &str) -> bool {
    matches!(
        model.trim(),
        "llama-3.3-70b-versatile" | "deepseek-chat" | "deepseek-reasoner"
    )
}

fn default_model(service: &str) -> &'static str {
    match service {
        "Groq" => "openai/gpt-oss-20b",
        "DeepSeek" => "deepseek-flash",
        "Mistral" => "mistral-small-latest",
        "OpenRouter" => "openrouter/free",
        "ChatGPT" => "gpt-4o-mini",
        "Local" => "granite4.1:3b",
        _ => "gemini-3.5-flash-lite",
    }
}

fn minutes_from_ms(ms: i64) -> String {
    let minutes = if ms <= 0 { 10 } else { (ms / 60_000).max(1) };
    minutes.to_string()
}

fn parse_count(text: &str, default: i64, min: i64, max: i64) -> i64 {
    text.trim().parse::<i64>().unwrap_or(default).clamp(min, max)
}

fn normalize_ignore(text: &str) -> String {
    let parts: Vec<&str> = text.split(',').map(str::trim).filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        "Buying,Selling,Discord".into()
    } else {
        parts.join(",")
    }
}

fn take_string(map: &mut Map<String, Value>, key: &str, default: &str) -> String {
    match map.remove(key) {
        Some(Value::String(text)) => text,
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => default.to_string(),
    }
}

fn take_bool(map: &mut Map<String, Value>, key: &str, default: bool) -> bool {
    match map.remove(key) {
        Some(value) => value_bool(&value, default),
        None => default,
    }
}

fn value_bool(value: &Value, default: bool) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_i64().unwrap_or(0) != 0,
        Value::String(text) => {
            let text = text.trim();
            if text.eq_ignore_ascii_case("true") || text == "1" {
                true
            } else if text.eq_ignore_ascii_case("false") || text == "0" {
                false
            } else {
                default
            }
        }
        _ => default,
    }
}

fn take_i64(map: &mut Map<String, Value>, key: &str, default: i64) -> i64 {
    match map.remove(key) {
        Some(Value::Number(number)) => number.as_i64().or_else(|| number.as_f64().map(|value| value as i64)).unwrap_or(default),
        Some(Value::String(text)) => text.trim().parse().unwrap_or(default),
        Some(Value::Bool(value)) => i64::from(value),
        _ => default,
    }
}

fn run_test(service: &str, api_key: &str, model: &str) -> String {
    let service = canonical_service(service);
    let key = api_key.trim();
    if service != "Local" && key.is_empty() {
        return fail(&service, 0, "API key is empty");
    }
    let model = {
        let trimmed = model.trim();
        if trimmed.is_empty() { default_model(&service) } else { trimmed }
    };
    let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(25)).build() {
        Ok(client) => client,
        Err(_) => return fail(&service, 0, "Could not start the request"),
    };
    let mut last = fail(&service, 0, "No response");
    for attempt in 0..2 {
        last = match post_test(&client, &service, key, model) {
            Ok((200, body)) => match extract_reply(&body) {
                Some(reply) => {
                    return format!(
                        "API Key Test PASSED\n\nService: {service}\nHTTP Code: 200\nReply: {}",
                        clip(&reply, 80)
                    );
                }
                None => fail(&service, 200, "The service replied, but the message could not be read."),
            },
            Ok((code, body)) => fail(&service, code, &clip(&body, 180)),
            Err(message) => fail(&service, 0, &message),
        };
        let retry = last.contains("HTTP Code: 0") || last.contains("HTTP Code: 429") || last.contains("HTTP Code: 5");
        if attempt == 0 && retry {
            continue;
        }
        break;
    }
    last
}

fn post_test(client: &reqwest::blocking::Client, service: &str, key: &str, model: &str) -> Result<(u16, String), String> {
    let local = service == "Local";
    let gemini = service == "Gemini";
    let url = match service {
        "Gemini" => format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            path_escape(model)
        ),
        "Local" => "http://127.0.0.1:11434/api/chat".into(),
        "DeepSeek" => "https://api.deepseek.com/chat/completions".into(),
        "ChatGPT" => "https://api.openai.com/v1/chat/completions".into(),
        "Mistral" => "https://api.mistral.ai/v1/chat/completions".into(),
        "OpenRouter" => "https://openrouter.ai/api/v1/chat/completions".into(),
        _ => "https://api.groq.com/openai/v1/chat/completions".into(),
    };
    let payload = if gemini {
        json!({
            "system_instruction": { "parts": [{ "text": SYSTEM_PROMPT }] },
            "contents": [{ "role": "user", "parts": [{ "text": TEST_MESSAGE }] }]
        })
    } else {
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": TEST_MESSAGE }
            ],
            "stream": false
        })
    };
    let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
    let mut request = client.post(url).header("Content-Type", "application/json").body(body);
    if gemini && !key.is_empty() {
        request = request.header("x-goog-api-key", key);
    } else if !local && !key.is_empty() {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    if service == "OpenRouter" {
        request = request.header("HTTP-Referer", "https://bashscripts.shop").header("X-Title", "BASH");
    }
    let response = request.send().map_err(|_| "Could not reach the AI service.".to_string())?;
    let code = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    Ok((code, body))
}

fn extract_reply(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    for pointer in [
        "/choices/0/message/content",
        "/message/content",
        "/candidates/0/content/parts/0/text",
    ] {
        if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn fail(service: &str, code: u16, detail: &str) -> String {
    format!(
        "API Key Test FAILED\n\nService: {service}\nHTTP Code: {code}\n\n{detail}\n\nCheck your API key and service settings."
    )
}

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut end = flat.char_indices().nth(max).map(|(index, _)| index).unwrap_or(flat.len());
    if !flat.is_char_boundary(end) {
        end = 0;
    }
    format!("{}…", &flat[..end])
}

fn path_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
            out.push(ch);
        } else {
            let mut bytes = [0; 4];
            for byte in ch.encode_utf8(&mut bytes).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_script_fields_and_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("bash-aichat-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = SimbaPaths::new(dir.clone());
        fs::create_dir_all(paths.configs()).unwrap();
        fs::write(
            paths.configs().join("aichat.json"),
            r#"{
                "enabled": "true",
                "api_service": "openai",
                "api_key": "sk-test",
                "api_model": "gpt-custom",
                "response_style": 2,
                "ignore_list": "Buying, Scam",
                "interaction_cooldown": 1200000,
                "activity": "agility",
                "extra_instructions": "keep it vague",
                "custom_flag": 1
            }"#,
        )
        .unwrap();

        let dialog = Dialog::load(&paths, "Hero");
        assert!(dialog.enabled);
        assert_eq!(dialog.service, "ChatGPT");
        assert_eq!(dialog.api_key, "sk-test");
        assert_eq!(dialog.model, "gpt-custom");
        assert_eq!(dialog.style, 2);
        assert_eq!(dialog.cooldown_minutes, "20");
        assert_eq!(dialog.custom, "keep it vague");
        assert_eq!(dialog.status_line(), "AI Chat on · ChatGPT");
        assert_eq!(dialog.extra.get("custom_flag").and_then(Value::as_i64), Some(1));

        dialog.save(&paths).unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(config_file(&paths, "Hero")).unwrap()).unwrap();
        assert_eq!(saved["enabled"], true);
        assert_eq!(saved["api_service"], "ChatGPT");
        assert_eq!(saved["api_key"], "sk-test");
        assert_eq!(saved["api_model"], "gpt-custom");
        assert_eq!(saved["response_style"], 2);
        assert_eq!(saved["ignore_list"], "Buying,Scam");
        assert_eq!(saved["interaction_cooldown"], 1_200_000);
        assert_eq!(saved["extra_instructions"], "keep it vague");
        assert_eq!(saved["system_instruction"], "keep it vague");
        assert!(saved.get("activity").is_none());
        assert_eq!(saved["custom_flag"], 1);
        assert_eq!(saved["max_nearby_players"], 2);
        assert!(paths.configs().join("aichat.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
