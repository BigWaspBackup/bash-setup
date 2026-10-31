//! Shared gear sets for Tormented Demons.
//!
//! Sets live in `Configs/gearhandler.json`, the same file the Simba equipment
//! manager writes. Tormented Demons remembers the two chosen sets by name in
//! `tormenteddemons.ini`.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Cursor, Read},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Button, Color32, ComboBox, Frame, Margin, RichText, Stroke, StrokeKind, TextEdit, Ui};
use image::ImageReader;

use crate::paths::SimbaPaths;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const READ_TIMEOUT: Duration = Duration::from_secs(90);
const STYLES: [&str; 3] = ["Melee", "Ranged", "Magic"];
const SLOT_KEYS: [&str; 12] = [
    "head",
    "secondary_ammo",
    "cape",
    "neck",
    "ammo",
    "weapon",
    "body",
    "shield",
    "legs",
    "hands",
    "feet",
    "ring",
];

struct DollSlot {
    key: &'static str,
    x: f32,
    y: f32,
}

/// Slot squares on `equipment.png` (202×273), matching the worn screen.
const DOLL_SLOTS: [DollSlot; 12] = [
    DollSlot { key: "head", x: 84.0, y: 41.0 },
    DollSlot { key: "secondary_ammo", x: 125.0, y: 41.0 },
    DollSlot { key: "cape", x: 43.0, y: 80.0 },
    DollSlot { key: "neck", x: 84.0, y: 80.0 },
    DollSlot { key: "ammo", x: 125.0, y: 80.0 },
    DollSlot { key: "weapon", x: 28.0, y: 119.0 },
    DollSlot { key: "body", x: 84.0, y: 119.0 },
    DollSlot { key: "shield", x: 140.0, y: 119.0 },
    DollSlot { key: "legs", x: 84.0, y: 159.0 },
    DollSlot { key: "hands", x: 28.0, y: 199.0 },
    DollSlot { key: "feet", x: 84.0, y: 199.0 },
    DollSlot { key: "ring", x: 140.0, y: 199.0 },
];
const DOLL_SCALE: f32 = 1.75;
const DOLL_SLOT: f32 = 34.0;

pub struct Editor {
    sets: Vec<Set>,
    loadout: [String; 2],
    style: [String; 2],
    punish: bool,
    editing: usize,
    draft: String,
    draft_for: usize,
    note: String,
    broken: bool,
    catalog: Catalog,
    art: ItemArt,
    selected_slot: String,
    slot_query: String,
    read: Option<ReadJob>,
}

#[derive(Clone)]
struct Set {
    name: String,
    slots: HashMap<String, String>,
}

struct Catalog {
    lists: HashMap<String, Vec<String>>,
    two_handed: HashSet<String>,
}

struct ReadJob {
    child: Child,
    started: Instant,
    account: String,
    killed: bool,
    rx: mpsc::Receiver<(String, String)>,
}

impl Drop for ReadJob {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Editor {
    pub fn load(paths: &SimbaPaths) -> Self {
        let catalog = Catalog::load(&paths.include("BashLib").join("utils").join("data").join("gear.json"));
        let gear_path = paths.configs().join("gearhandler.json");
        let (sets, broken) = match fs::read_to_string(&gear_path) {
            Ok(text) if text.trim().is_empty() => (Vec::new(), false),
            Ok(text) => match parse_sets(&text) {
                Some(sets) => (sets, false),
                None => (Vec::new(), true),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Vec::new(), false),
            Err(_) => (Vec::new(), true),
        };
        let ini = fs::read_to_string(paths.configs().join("tormenteddemons.ini")).unwrap_or_default();
        let gear = crate::agility::ini_section(&ini, "Gear");
        let mut editor = Self {
            loadout: [
                resolve_loadout(&sets, &gear, "Loadout1Name", "Loadout1Index"),
                resolve_loadout(&sets, &gear, "Loadout2Name", "Loadout2Index"),
            ],
            style: [
                style_or(crate::agility::ini_get(&gear, "Loadout1Style").as_deref().unwrap_or(""), "Melee"),
                style_or(crate::agility::ini_get(&gear, "Loadout2Style").as_deref().unwrap_or(""), "Ranged"),
            ],
            punish: crate::agility::ini_bool(&gear, "Punish", false),
            editing: 0,
            draft: String::new(),
            draft_for: usize::MAX,
            note: if broken {
                "Gear sets could not be read, so they were left unchanged.".into()
            } else {
                String::new()
            },
            broken,
            catalog,
            art: ItemArt::load(paths),
            selected_slot: "weapon".into(),
            slot_query: String::new(),
            read: None,
            sets,
        };
        editor.clear_two_handed_shields();
        editor
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        let finished = {
            let Some(job) = self.read.as_mut() else {
                return;
            };
            if job.started.elapsed() > READ_TIMEOUT && !job.killed {
                let _ = job.child.kill();
                job.killed = true;
            }
            match job.child.try_wait() {
                Ok(Some(_)) => match job.rx.try_recv() {
                    Ok(output) => Some((output.0, output.1, job.killed, job.account.clone())),
                    Err(mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(200));
                        None
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some((String::new(), String::new(), job.killed, job.account.clone()))
                    }
                },
                Ok(None) => {
                    ctx.request_repaint_after(Duration::from_millis(200));
                    None
                }
                Err(_) => Some((String::new(), String::new(), true, job.account.clone())),
            }
        };
        if let Some((stdout, stderr, killed, account)) = finished {
            self.read = None;
            self.finish_read(&stdout, &stderr, killed, &account);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, paths: &SimbaPaths, hwnd: Option<isize>, account: &str) {
        if self.draft_for != self.editing {
            self.draft = self.current_name();
            self.draft_for = self.editing;
        }
        self.clear_two_handed_shields();
        let busy = self.read.is_some();
        ui.horizontal_top(|ui| {
            self.equipment_screen(ui);
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        RichText::new("Please setup two loadouts, Two different combat styles are required to fight Tormented Demons").weak(),
                    )
                    .wrap_mode(egui::TextWrapMode::Wrap),
                );
                ui.add_space(6.0);
                ui.columns(2, |cols| {
                    let (left, right) = cols.split_at_mut(1);
                    self.card(&mut left[0], 0, busy);
                    self.card(&mut right[0], 1, busy);
                });
                if self.style[0].eq_ignore_ascii_case(&self.style[1]) {
                    ui.add_space(4.0);
                    ui.label(RichText::new("The two loadouts need different styles.").color(Color32::from_rgb(232, 196, 96)));
                }
                ui.add_space(8.0);
                self.set_name(ui, busy);
                ui.add_space(8.0);
                self.slot_picker(ui, busy);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.punish, "Punish");
                    crate::ui::help_tip(
                        ui,
                        "After a fire bomb, use a crush or ranged weapon from the backpack for one hit.",
                    );
                });
                ui.add_space(8.0);
                let has_set = self.current_index().is_some();
                let mut read = false;
                let mut create = false;
                let mut duplicate = false;
                let mut delete = false;
                let mut save = false;
                ui.horizontal_wrapped(|ui| {
                    read = ui
                        .add_enabled(!busy, Button::new(if busy { "Reading worn gear..." } else { "Read from client" }))
                        .on_hover_text("Logs in if needed, then fills the slots from what is currently worn.")
                        .clicked();
                    create = ui
                        .add_enabled(!busy, Button::new("New loadout"))
                        .on_hover_text("Create an empty gear set and select it for this loadout.")
                        .clicked();
                    duplicate = ui
                        .add_enabled(!busy && has_set, Button::new("Duplicate"))
                        .on_hover_text("Copy the current gear set into a new one.")
                        .clicked();
                    delete = ui
                        .add_enabled(!busy && has_set, Button::new("Delete"))
                        .on_hover_text("Delete the current gear set.")
                        .clicked();
                    save = ui
                        .add_enabled(!busy, Button::new("Save gear"))
                        .on_hover_text("Write the gear sets and Tormented Demons loadout choices to disk.")
                        .clicked();
                    crate::ui::help_tip(ui, "Click a slot on the equipment screen, then pick an item on the right.");
                });
                if !self.note.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.note).color(note_color(&self.note)));
                }
                if read {
                    self.start_read(paths, hwnd, account);
                }
                if create {
                    self.create_set();
                }
                if duplicate {
                    self.duplicate_current();
                }
                if delete && self.current_index().is_some() && crate::ui::ask_yes_no("Delete this gear set?") {
                    self.delete_current();
                }
                if save {
                    self.note = match self.persist(paths) {
                        Ok(()) => "Gear saved.".into(),
                        Err(err) => err,
                    };
                }
            });
        });
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.broken {
            return Err("Gear sets could not be read, so they were left unchanged.".into());
        }
        if let Some(error) = self.draft_problem() {
            return Err(error);
        }
        if self.style[0].eq_ignore_ascii_case(&self.style[1]) {
            return Err("Pick two different combat styles.".into());
        }
        if self.find_set(&self.loadout[0]).is_none() {
            return Err("Pick a gear set for loadout 1.".into());
        }
        if self.find_set(&self.loadout[1]).is_none() {
            return Err("Pick a gear set for loadout 2.".into());
        }
        if self.loadout[0].eq_ignore_ascii_case(&self.loadout[1]) {
            return Err("Pick a different gear set for each loadout.".into());
        }
        if !self.set_has_item(&self.loadout[0]) {
            return Err("Loadout 1 has no items.".into());
        }
        if !self.set_has_item(&self.loadout[1]) {
            return Err("Loadout 2 has no items.".into());
        }
        Ok(())
    }

    pub fn persist(&mut self, paths: &SimbaPaths) -> Result<(), String> {
        if self.broken {
            return Err("Gear sets could not be read, so they were left unchanged.".into());
        }
        if let Some(error) = self.draft_problem() {
            return Err(error);
        }
        if let Some(index) = self.current_index() {
            let draft = self.draft.trim().to_string();
            if !draft.is_empty() {
                self.apply_name(index, &draft);
                self.draft = draft;
            }
        }
        self.clear_two_handed_shields();
        let configs = paths.configs();
        fs::create_dir_all(&configs).map_err(|_| "Could not save gear settings.".to_string())?;
        fs::write(configs.join("gearhandler.json"), sets_json(&self.sets)).map_err(|_| "Could not save gear settings.".to_string())?;
        let ini_path = configs.join("tormenteddemons.ini");
        let existing = fs::read_to_string(&ini_path).unwrap_or_default();
        let pairs = [
            ("Loadout1Name", self.loadout[0].clone()),
            ("Loadout1Index", self.index_of(&self.loadout[0]).to_string()),
            ("Loadout1Style", self.style[0].clone()),
            ("Loadout2Name", self.loadout[1].clone()),
            ("Loadout2Index", self.index_of(&self.loadout[1]).to_string()),
            ("Loadout2Style", self.style[1].clone()),
            ("Punish", if self.punish { "True".into() } else { "False".into() }),
        ];
        let text = crate::agility::write_section(&existing, "Gear", &pairs);
        fs::write(ini_path, text).map_err(|_| "Could not save gear settings.".to_string())?;
        Ok(())
    }

    fn card(&mut self, ui: &mut Ui, index: usize, busy: bool) {
        let selected = self.editing == index;
        let hit_id = ui.id().with(("loadout-hit", index));
        if !busy {
            if let Some(previous) = ui.ctx().data(|data| data.get_temp::<egui::Rect>(hit_id)) {
                if ui.interact(previous, hit_id, egui::Sense::click()).clicked() {
                    self.select_card(index);
                }
            }
        }
        let border = if selected {
            Color32::from_rgb(120, 170, 130)
        } else if ui.visuals().dark_mode {
            Color32::from_rgb(55, 60, 68)
        } else {
            Color32::from_rgb(176, 180, 186)
        };
        let frame = Frame::new()
            .fill(crate::ui::panel_fill(ui.visuals()))
            .stroke(Stroke::new(if selected { 1.5_f32 } else { 1.0_f32 }, border))
            .corner_radius(6.0)
            .inner_margin(Margin::symmetric(10, 8));
        let mut prepared = frame.begin(ui);
        prepared.content_ui.set_min_width(prepared.content_ui.available_width());
        prepared.content_ui.add_enabled_ui(!busy, |ui| {
            ui.label(RichText::new(if index == 0 { "Loadout 1" } else { "Loadout 2" }).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Style").weak());
            let mut style = self.style[index].clone();
            let style_response = ComboBox::from_id_salt(("td-style", index))
                .selected_text(&style)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for choice in STYLES {
                        ui.selectable_value(&mut style, choice.to_string(), choice);
                    }
                });
            if style_response.response.clicked() {
                self.select_card(index);
            }
            if style != self.style[index] {
                self.style[index] = style;
                self.select_card(index);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Gear set").weak());
            let current = if self.find_set(&self.loadout[index]).is_some() {
                self.loadout[index].clone()
            } else {
                "Choose a gear set".into()
            };
            let mut picked = None;
            let set_response = ComboBox::from_id_salt(("td-set", index))
                .selected_text(&current)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for set in &self.sets {
                        if ui.selectable_label(set.name.eq_ignore_ascii_case(&self.loadout[index]), &set.name).clicked() {
                            picked = Some(set.name.clone());
                        }
                    }
                });
            if set_response.response.clicked() {
                self.select_card(index);
            }
            if let Some(name) = picked {
                self.loadout[index] = name;
                self.select_card(index);
            }
        });
        let response = prepared.end(ui);
        ui.ctx().data_mut(|data| data.insert_temp(hit_id, response.rect));
    }

    fn equipment_screen(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(202.0 * DOLL_SCALE, 273.0 * DOLL_SCALE), egui::Sense::hover());
        let doll = self.art.doll(ui.ctx()).map(|tex| tex.id());
        let blank = self.art.blank(ui.ctx()).map(|tex| tex.id());
        let two_handed = self.weapon_is_two_handed();
        let mut icons = Vec::new();
        let mut names = Vec::new();
        for slot in DOLL_SLOTS {
            let name = self.slot_value(slot.key);
            let icon = if name.is_empty() {
                None
            } else {
                self.art.icon(ui.ctx(), &name).map(|tex| (tex.id(), tex.size()))
            };
            icons.push(icon);
            names.push(name);
        }
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if let Some(id) = doll {
            ui.painter().image(id, rect, uv, Color32::WHITE);
        }
        let mut pick = None;
        for (index, slot) in DOLL_SLOTS.iter().enumerate() {
            let slot_rect = scaled_slot(rect, slot);
            let response = ui.interact(slot_rect, ui.id().with(("doll-slot", slot.key)), egui::Sense::click());
            let blocked = slot.key == "shield" && two_handed;
            if response.clicked() && !blocked {
                pick = Some(slot.key);
            }
            if let Some((id, size)) = icons[index] {
                if let Some(blank) = blank {
                    // Cover the empty-slot picture with the same stone, then the item.
                    let inset = 2.0 / 36.0;
                    let stone = egui::Rect::from_min_max(egui::pos2(inset, inset), egui::pos2(1.0 - inset, 1.0 - inset));
                    ui.painter().image(blank, slot_rect, stone, Color32::WHITE);
                }
                let image = egui::vec2(size[0] as f32, size[1] as f32);
                ui.painter().image(id, fitted_icon(slot_rect, image), uv, Color32::WHITE);
            }
            if self.selected_slot == slot.key {
                ui.painter().rect_stroke(slot_rect.shrink(1.0), 0.0, Stroke::new(2.0_f32, Color32::from_rgb(214, 176, 92)), StrokeKind::Inside);
            } else if response.hovered() && !blocked {
                ui.painter().rect_stroke(slot_rect.shrink(1.0), 0.0, Stroke::new(1.0_f32, Color32::from_rgb(160, 140, 90)), StrokeKind::Inside);
            }
            if blocked {
                ui.painter().rect_filled(slot_rect, 0.0, Color32::from_black_alpha(120));
                let _ = response.on_hover_text("Two-handed");
            } else if names[index].is_empty() {
                let _ = response.on_hover_text(slot_label(slot.key));
            } else {
                let _ = response.on_hover_text(&names[index]);
            }
        }
        if let Some(key) = pick {
            if self.selected_slot != key {
                self.slot_query.clear();
            }
            self.selected_slot = key.to_string();
        }
    }

    fn set_name(&mut self, ui: &mut Ui, busy: bool) {
        let Some(index) = self.current_index() else {
            ui.label("Choose a gear set, or create one.");
            return;
        };
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Loadout Name");
                let mut name = self.draft.clone();
                let response = ui.add(TextEdit::singleline(&mut name).hint_text("Loadout Name").desired_width(280.0));
                if response.changed() {
                    self.draft = name;
                    let trimmed = self.draft.trim().to_string();
                    if !trimmed.is_empty() && !self.name_taken(index, &trimmed) {
                        self.apply_name(index, &trimmed);
                    }
                }
                if response.lost_focus() {
                    self.draft = self.draft.trim().to_string();
                    if !self.draft.is_empty() && !self.name_taken(index, &self.draft) {
                        let draft = self.draft.clone();
                        self.apply_name(index, &draft);
                    }
                }
            });
        });
    }

    fn slot_picker(&mut self, ui: &mut Ui, busy: bool) {
        let key = self.selected_slot.clone();
        let two_handed = key == "shield" && self.weapon_is_two_handed();
        ui.label(RichText::new(slot_label(&key)).strong().size(16.0));
        let current = self.slot_value(&key);
        ui.horizontal(|ui| {
            let icon = self.art.icon(ui.ctx(), &current).map(|tex| tex.id());
            if let Some(id) = icon {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 32.0), egui::Sense::hover());
                ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            ui.label(if current.is_empty() { "None".to_string() } else { current.clone() });
        });
        if two_handed {
            ui.label(RichText::new("Two-handed weapons leave the shield empty.").weak());
            return;
        }
        if self.current_index().is_none() {
            return;
        }
        let mut chosen: Option<String> = None;
        ui.add_enabled_ui(!busy, |ui| {
            ui.add(TextEdit::singleline(&mut self.slot_query).hint_text("Search items").desired_width(280.0));
            let query = self.slot_query.trim().to_lowercase();
            let items = self.catalog.lists.get(&key).cloned().unwrap_or_default();
            egui::ScrollArea::vertical().max_height(180.0).id_salt(("td-slot-search", key.clone())).show(ui, |ui| {
                if ui.selectable_label(current.is_empty(), "None").clicked() {
                    chosen = Some(String::new());
                }
                if query.is_empty() {
                    ui.label(RichText::new("Type to search the full list.").weak());
                    return;
                }
                let mut shown = 0usize;
                let custom = !current.is_empty() && !items.iter().any(|item| item.eq_ignore_ascii_case(&current));
                for item in &items {
                    if !item.to_lowercase().contains(&query) {
                        continue;
                    }
                    if shown >= 40 {
                        ui.label(RichText::new("Keep typing to narrow the list.").weak());
                        break;
                    }
                    shown += 1;
                    let icon = self.art.icon(ui.ctx(), item).map(|tex| tex.id());
                    let picked = ui
                        .horizontal(|ui| {
                            if let Some(id) = icon {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 24.0), egui::Sense::hover());
                                ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            ui.selectable_label(item.eq_ignore_ascii_case(&current), item).clicked()
                        })
                        .inner;
                    if picked {
                        chosen = Some(item.clone());
                    }
                }
                if custom && current.to_lowercase().contains(&query) {
                    if ui.selectable_label(true, &current).clicked() {
                        chosen = Some(current.clone());
                    }
                } else if shown == 0 {
                    ui.label(RichText::new("No matching items.").weak());
                }
            });
        });
        if let Some(item) = chosen {
            self.slot_query.clear();
            self.set_slot(&key, &item);
        }
    }

    fn start_read(&mut self, paths: &SimbaPaths, hwnd: Option<isize>, account: &str) {
        if self.read.is_some() {
            return;
        }
        if self.broken {
            self.note = "Gear sets could not be read, so they were left unchanged.".into();
            return;
        }
        if self.current_index().is_none() {
            self.note = "Choose a gear set before reading worn gear.".into();
            return;
        }
        let Some(hwnd) = hwnd.filter(|hwnd| *hwnd != 0) else {
            self.note = "Choose a RuneLite client first.".into();
            return;
        };
        let account = account.trim();
        if account.is_empty() || account.eq_ignore_ascii_case("NoUserNameSelected") {
            self.note = "Choose an account before reading worn gear.".into();
            return;
        }
        let simba = paths.simba_exe();
        if !simba.is_file() {
            self.note = "Simba was not found.".into();
            return;
        }
        let script = std::env::temp_dir().join("bash-read-gear.simba");
        if fs::write(&script, READ_SCRIPT).is_err() {
            self.note = "Could not read worn gear.".into();
            return;
        }
        let mut command = Command::new(&simba);
        command
            .current_dir(&paths.root)
            .env("BASH_LAUNCHER", "1")
            .env("BASH_ACCOUNT", account)
            .arg(format!("--target={}", hwnd as i32))
            .arg("--run")
            .arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                self.note = "Could not read worn gear.".into();
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let out = thread::spawn(move || read_pipe(stdout));
            let err = thread::spawn(move || read_pipe(stderr));
            let out = out.join().unwrap_or_default();
            let err = err.join().unwrap_or_default();
            let _ = tx.send((out, err));
        });
        self.note = "Reading worn gear...".into();
        self.read = Some(ReadJob {
            child,
            started: Instant::now(),
            account: account.to_string(),
            killed: false,
            rx,
        });
    }

    fn finish_read(&mut self, stdout: &str, stderr: &str, killed: bool, account: &str) {
        let Some(index) = self.current_index() else {
            self.note = "Choose a gear set before reading worn gear.".into();
            return;
        };
        let mut worn = HashMap::new();
        for line in stdout.lines() {
            let Some(rest) = line.trim().strip_prefix("BASHGEAR ") else {
                continue;
            };
            let Some((raw, item)) = rest.split_once('=') else {
                continue;
            };
            let key = raw.rsplit('.').next().unwrap_or(raw).trim().to_lowercase();
            if SLOT_KEYS.contains(&key.as_str()) {
                worn.insert(key, item.trim().trim_matches('"').to_string());
            }
        }
        if worn.is_empty() {
            self.note = if killed {
                "Reading worn gear took too long. Log in, then try again.".into()
            } else {
                read_failure(stderr, account)
            };
            return;
        }
        if worn.values().all(|item| is_blank(item)) {
            self.note = "No worn gear was found.".into();
            return;
        }
        for key in SLOT_KEYS {
            let item = worn.get(key).map(String::as_str).unwrap_or("");
            let stored = self.catalog.canonical(key, item);
            if stored.is_empty() {
                self.sets[index].slots.remove(key);
            } else {
                self.sets[index].slots.insert(key.to_string(), stored);
            }
        }
        self.clear_two_handed_shields();
        self.note = "Worn gear filled in. Use Save gear to keep it.".into();
    }

    fn select_card(&mut self, index: usize) {
        if self.editing == index {
            return;
        }
        self.editing = index;
        self.draft = self.current_name();
        self.draft_for = index;
    }

    fn create_set(&mut self) {
        let name = unique_name(&self.sets, "New loadout");
        self.sets.push(Set { name: name.clone(), slots: HashMap::new() });
        let names = self.loadout.clone();
        let card = names.iter().position(|entry| self.find_set(entry).is_none()).unwrap_or(self.editing);
        self.loadout[card] = name;
        self.select_card(card);
        self.draft = self.current_name();
        self.draft_for = self.editing;
    }

    fn duplicate_current(&mut self) {
        let Some(index) = self.current_index() else {
            return;
        };
        let mut copy = self.sets[index].clone();
        copy.name = unique_name(&self.sets, &format!("{} copy", copy.name));
        let name = copy.name.clone();
        self.sets.push(copy);
        self.loadout[self.editing] = name;
        self.draft = self.current_name();
        self.draft_for = self.editing;
    }

    fn delete_current(&mut self) {
        let Some(index) = self.current_index() else {
            return;
        };
        let name = self.sets[index].name.clone();
        self.sets.remove(index);
        for slot in &mut self.loadout {
            if slot.eq_ignore_ascii_case(&name) {
                slot.clear();
            }
        }
        self.draft = self.current_name();
        self.draft_for = self.editing;
    }

    fn current_index(&self) -> Option<usize> {
        self.find_set(&self.loadout[self.editing])
    }

    fn current_name(&self) -> String {
        self.current_index().map(|index| self.sets[index].name.clone()).unwrap_or_default()
    }

    fn find_set(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.sets.iter().position(|set| set.name.eq_ignore_ascii_case(name))
    }

    fn index_of(&self, name: &str) -> i32 {
        self.find_set(name).map(|index| index as i32).unwrap_or(-1)
    }

    fn set_has_item(&self, name: &str) -> bool {
        self.find_set(name).is_some_and(|index| self.sets[index].slots.values().any(|item| !is_blank(item)))
    }

    fn name_taken(&self, index: usize, name: &str) -> bool {
        self.sets.iter().enumerate().any(|(other, set)| other != index && set.name.eq_ignore_ascii_case(name))
    }

    fn draft_problem(&self) -> Option<String> {
        let index = self.current_index()?;
        if self.draft_for != self.editing {
            return None;
        }
        let draft = self.draft.trim();
        if draft.is_empty() {
            return Some("Give the gear set a name.".into());
        }
        if self.name_taken(index, draft) {
            return Some("A gear set already uses that name.".into());
        }
        None
    }

    fn apply_name(&mut self, index: usize, name: &str) {
        let old = self.sets[index].name.clone();
        if old == name {
            return;
        }
        self.sets[index].name = name.to_string();
        for slot in &mut self.loadout {
            if slot.eq_ignore_ascii_case(&old) {
                *slot = name.to_string();
            }
        }
    }

    fn slot_value(&self, key: &str) -> String {
        self.current_index()
            .and_then(|index| self.sets[index].slots.get(key).cloned())
            .filter(|item| !is_blank(item))
            .unwrap_or_default()
    }

    fn set_slot(&mut self, key: &str, item: &str) {
        let Some(index) = self.current_index() else {
            return;
        };
        if is_blank(item) {
            self.sets[index].slots.remove(key);
        } else {
            self.sets[index].slots.insert(key.to_string(), item.to_string());
        }
        if key == "weapon" {
            let weapon = item.to_string();
            if self.catalog.is_two_handed(&weapon) {
                self.sets[index].slots.remove("shield");
            }
        }
    }

    fn weapon_is_two_handed(&self) -> bool {
        self.catalog.is_two_handed(&self.slot_value("weapon"))
    }

    fn clear_two_handed_shields(&mut self) {
        let two_handed = self.catalog.two_handed.clone();
        for set in &mut self.sets {
            let weapon = set.slots.get("weapon").cloned().unwrap_or_default();
            if !is_blank(&weapon) && two_handed.contains(&weapon.to_lowercase()) {
                set.slots.remove("shield");
            }
        }
    }
}

struct ItemArt {
    ids: HashMap<String, u32>,
    zip_path: std::path::PathBuf,
    doll_path: std::path::PathBuf,
    blank_path: std::path::PathBuf,
    zip: Option<zip::ZipArchive<File>>,
    doll: Option<egui::TextureHandle>,
    blank: Option<egui::TextureHandle>,
    icons: HashMap<u32, egui::TextureHandle>,
    missing: HashSet<u32>,
}

impl ItemArt {
    fn load(paths: &SimbaPaths) -> Self {
        let finder = paths.include("SRL-B").join("osr").join("finders").join("itemfinder");
        Self {
            ids: read_item_ids(&finder.join("item.txt"), &finder.join("id.txt")),
            zip_path: finder.join("items-imgs.zip"),
            doll_path: paths.include("BashLib").join("utils").join("assets").join("equipment.png"),
            blank_path: paths.include("BashLib").join("utils").join("assets").join("slot.png"),
            zip: None,
            doll: None,
            blank: None,
            icons: HashMap::new(),
            missing: HashSet::new(),
        }
    }

    fn doll(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if self.doll.is_none() {
            let bytes = fs::read(&self.doll_path).ok()?;
            let image = decode_sprite(&bytes, false)?;
            self.doll = Some(ctx.load_texture("bash-equipment", image, crisp_texture()));
        }
        self.doll.as_ref()
    }

    /// Plain stone slot, without the empty-slot picture (cape, helm, and so on).
    fn blank(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if self.blank.is_none() {
            let bytes = fs::read(&self.blank_path).ok()?;
            let image = decode_sprite(&bytes, false)?;
            self.blank = Some(ctx.load_texture("bash-slot", image, crisp_texture()));
        }
        self.blank.as_ref()
    }

    fn icon(&mut self, ctx: &egui::Context, name: &str) -> Option<&egui::TextureHandle> {
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return None;
        }
        let id = *self.ids.get(&name.to_lowercase())?;
        if self.missing.contains(&id) {
            return None;
        }
        if !self.icons.contains_key(&id) {
            let Some(bytes) = self.read_sprite(id) else {
                self.missing.insert(id);
                return None;
            };
            let Some(image) = decode_sprite(&bytes, true) else {
                self.missing.insert(id);
                return None;
            };
            self.icons.insert(id, ctx.load_texture(format!("bash-item-{id}"), image, crisp_texture()));
        }
        self.icons.get(&id)
    }

    fn read_sprite(&mut self, id: u32) -> Option<Vec<u8>> {
        if self.zip.is_none() {
            let file = File::open(&self.zip_path).ok()?;
            self.zip = zip::ZipArchive::new(file).ok();
        }
        let archive = self.zip.as_mut()?;
        let mut entry = archive.by_name(&format!("{id}.png")).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }
}

fn read_item_ids(items_path: &std::path::Path, ids_path: &std::path::Path) -> HashMap<String, u32> {
    let items = fs::read_to_string(items_path).unwrap_or_default();
    let ids = fs::read_to_string(ids_path).unwrap_or_default();
    let mut map = HashMap::new();
    for (name, id) in items.lines().zip(ids.lines()) {
        let name = name.trim().to_lowercase();
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        if name.is_empty() {
            continue;
        }
        map.entry(name).or_insert(id);
    }
    map
}

fn decode_sprite(bytes: &[u8], clear_shadow: bool) -> Option<egui::ColorImage> {
    let image = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.decode().ok()?;
    let mut rgba = image.into_rgba8();
    if clear_shadow {
        for pixel in rgba.pixels_mut() {
            if pixel[0] == 48 && pixel[1] == 32 && pixel[2] == 32 {
                *pixel = image::Rgba([0, 0, 0, 0]);
            }
        }
    }
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

fn crisp_texture() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: None,
    }
}

fn scaled_slot(doll: egui::Rect, slot: &DollSlot) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(doll.min.x + slot.x * DOLL_SCALE, doll.min.y + slot.y * DOLL_SCALE),
        egui::vec2(DOLL_SLOT * DOLL_SCALE, DOLL_SLOT * DOLL_SCALE),
    )
}

fn fitted_icon(slot: egui::Rect, image: egui::Vec2) -> egui::Rect {
    let max = slot.shrink(4.0).size();
    if image.x <= 0.0 || image.y <= 0.0 {
        return slot;
    }
    let scale = (max.x / image.x).min(max.y / image.y);
    egui::Rect::from_center_size(slot.center(), image * scale)
}

impl Catalog {
    fn load(path: &std::path::Path) -> Self {
        let text = fs::read_to_string(path).unwrap_or_default();
        let root: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(serde_json::Value::Null);
        let mut lists = HashMap::new();
        for key in SLOT_KEYS {
            let source = if key == "secondary_ammo" { "ammo" } else { key };
            let mut items = json_strings(&root, source);
            if key == "weapon" {
                items.extend(json_strings(&root, "2h"));
            }
            sort_dedup(&mut items);
            lists.insert(key.to_string(), items);
        }
        let two_handed = json_strings(&root, "2h").into_iter().map(|item| item.to_lowercase()).collect();
        Self { lists, two_handed }
    }

    fn is_two_handed(&self, item: &str) -> bool {
        !is_blank(item) && self.two_handed.contains(&item.to_lowercase())
    }

    fn canonical(&self, key: &str, item: &str) -> String {
        if is_blank(item) {
            return String::new();
        }
        if let Some(list) = self.lists.get(key) {
            if let Some(found) = list.iter().find(|name| name.eq_ignore_ascii_case(item)) {
                return found.clone();
            }
        }
        sentence_case(item)
    }
}

fn parse_sets(text: &str) -> Option<Vec<Set>> {
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let object = value.as_object()?;
    let mut order = top_level_keys(text);
    for key in object.keys() {
        if !order.iter().any(|existing| existing.eq_ignore_ascii_case(key)) {
            order.push(key.clone());
        }
    }
    let mut sets = Vec::new();
    for key in order {
        let Some(slots_value) = object.iter().find(|(name, _)| name.eq_ignore_ascii_case(&key)).map(|(_, value)| value) else {
            continue;
        };
        let Some(slots_object) = slots_value.as_object() else {
            continue;
        };
        let mut slots = HashMap::new();
        for key in SLOT_KEYS {
            if let Some(item) = slots_object.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)).and_then(|(_, value)| value.as_str()) {
                if !is_blank(item) {
                    slots.insert(key.to_string(), item.to_string());
                }
            }
        }
        let name = unique_name(&sets, key.trim());
        sets.push(Set { name, slots });
    }
    Some(sets)
}

fn top_level_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0i32;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            '"' if depth == 1 => {
                let mut key = String::new();
                while let Some(next) = chars.next() {
                    if next == '\\' {
                        if let Some(escaped) = chars.next() {
                            key.push(escaped);
                        }
                        continue;
                    }
                    if next == '"' {
                        break;
                    }
                    key.push(next);
                }
                let mut look = chars.clone();
                while look.peek().is_some_and(|ch| ch.is_whitespace()) {
                    look.next();
                }
                if look.peek() == Some(&':') {
                    keys.push(key);
                }
            }
            _ => {}
        }
    }
    keys
}

fn unique_name(sets: &[Set], base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "New loadout" } else { base };
    if !sets.iter().any(|set| set.name.eq_ignore_ascii_case(base)) {
        return base.to_string();
    }
    for number in 2..1000 {
        let candidate = format!("{base} {number}");
        if !sets.iter().any(|set| set.name.eq_ignore_ascii_case(&candidate)) {
            return candidate;
        }
    }
    format!("{base} {}", sets.len() + 1)
}

fn resolve_loadout(sets: &[Set], section: &str, name_key: &str, index_key: &str) -> String {
    let name = crate::agility::ini_get(section, name_key).unwrap_or_default();
    if let Some(set) = sets.iter().find(|set| set.name.eq_ignore_ascii_case(name.trim())) {
        return set.name.clone();
    }
    if !name.trim().is_empty() {
        return String::new();
    }
    let index = crate::agility::ini_get(section, index_key).and_then(|value| value.parse::<i32>().ok()).unwrap_or(-1);
    usize::try_from(index).ok().and_then(|index| sets.get(index)).map(|set| set.name.clone()).unwrap_or_default()
}

fn sets_json(sets: &[Set]) -> String {
    let mut out = String::from("{\n");
    for (set_index, set) in sets.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&serde_json::Value::String(set.name.clone()).to_string());
        out.push_str(": {\n");
        for (item_index, key) in SLOT_KEYS.iter().enumerate() {
            let value = set
                .slots
                .get(*key)
                .map(|value| value.trim())
                .filter(|value| !is_blank(value))
                .unwrap_or("None");
            out.push_str("    ");
            out.push_str(&serde_json::Value::String((*key).to_string()).to_string());
            out.push_str(": ");
            out.push_str(&serde_json::Value::String(value.to_string()).to_string());
            if item_index + 1 != SLOT_KEYS.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }");
        if set_index + 1 != sets.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn json_strings(root: &serde_json::Value, key: &str) -> Vec<String> {
    root.get(key)
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter_map(|value| value.as_str()).map(sentence_case).collect())
        .unwrap_or_default()
}

fn sort_dedup(items: &mut Vec<String>) {
    items.sort_by(|left, right| left.to_lowercase().cmp(&right.to_lowercase()));
    items.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

fn sentence_case(value: &str) -> String {
    let mut out = String::new();
    let mut capitalise = true;
    for ch in value.chars() {
        if ch.is_whitespace() {
            capitalise = true;
            out.push(ch);
        } else if capitalise {
            out.extend(ch.to_uppercase());
            capitalise = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn style_or(value: &str, fallback: &str) -> String {
    STYLES.iter().find(|style| style.eq_ignore_ascii_case(value.trim())).copied().unwrap_or(fallback).to_string()
}

fn slot_label(key: &str) -> &'static str {
    match key {
        "head" => "Head",
        "secondary_ammo" => "Quiver",
        "cape" => "Cape",
        "neck" => "Neck",
        "ammo" => "Ammo",
        "weapon" => "Weapon",
        "body" => "Body",
        "shield" => "Shield",
        "legs" => "Legs",
        "hands" => "Hands",
        "feet" => "Feet",
        "ring" => "Ring",
        _ => "Slot",
    }
}

fn is_blank(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.eq_ignore_ascii_case("none")
}

fn note_color(note: &str) -> Color32 {
    if note.starts_with("Worn gear") || note == "Gear saved." {
        Color32::from_rgb(140, 190, 150)
    } else {
        Color32::from_rgb(232, 196, 96)
    }
}

fn read_failure(stderr: &str, account: &str) -> String {
    let line = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("Users\\") && !line.contains("Users/"))
        .unwrap_or("");
    let hides_account = !account.is_empty() && line.to_lowercase().contains(&account.to_lowercase());
    if line.is_empty() || hides_account || line.chars().count() > 160 {
        "Could not read worn gear. Log in on that client, then try again.".into()
    } else {
        format!("Could not read worn gear. {line}")
    }
}

fn read_pipe(pipe: Option<impl Read>) -> String {
    let mut text = String::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_string(&mut text);
    }
    text
}

const READ_SCRIPT: &str = r#"{$I SRL-B/osr.simba}
{$I BashLib/osr.simba}
{$IFHASFILE credentials.simba}{$I credentials.simba}{$ENDIF}
{$I BashLib/optional/handlers/bashlauncher.simba}

procedure ReadWornGear();
var
  slot: ERSEquipmentSlot;
  items: TRSItemArray;
  item: String;
begin
  BASHUseLauncherAccount();
  BASHSetupLauncherRemoteInput();
  if not RSClient.IsLoggedIn() then
    if not Login.LoginPlayer() then
      TerminateScript('Could not log in to read worn gear.');
  for slot := Low(ERSEquipmentSlot) to High(ERSEquipmentSlot) do
  begin
    items := Equipment.Discover(slot);
    item := '';
    if Length(items) > 0 then
      item := ToStr(items[0]);
    WriteLn('BASHGEAR ' + ToStr(slot).ToLower() + '=' + item);
  end;
end;

begin
  ReadWornGear();
end.
"#;
