//! Per-account player-owned house layout. Writes `Configs/house/{account}.json`, the file `THouseLoader` reads.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use eframe::egui::{self, ComboBox, RichText, ScrollArea};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::RgbaImage;
use serde_json::{json, Map, Value};

use crate::paths::SimbaPaths;

const LOCATIONS: [&str; 9] = [
    "Rimmington",
    "Taverley",
    "Pollnivneach",
    "Hosidius",
    "Rellekka",
    "Aldarin",
    "Brimhaven",
    "Yanille",
    "Prifddinas",
];
const STYLES: [&str; 10] = [
    "Basic wood",
    "Basic stone",
    "Whitewashed stone",
    "Fremennik wood",
    "Tropical wood",
    "Fancy stone",
    "Deathly mansion",
    "Twisted",
    "Hosidius",
    "Cosy cabin",
];
const ROOMS: [&str; 23] = [
    "Empty",
    "Parlour",
    "Garden",
    "Kitchen",
    "Dining room",
    "Workshop",
    "Bedroom",
    "Skill hall",
    "League hall",
    "Games room",
    "Combat room",
    "Quest hall",
    "Menagerie (outdoors)",
    "Menagerie (indoors)",
    "Study",
    "Costume room",
    "Chapel",
    "Portal chamber",
    "Formal garden",
    "Throne room",
    "Superior garden",
    "Portal nexus",
    "Achievement gallery",
];
const PORTALS: [&str; 32] = [
    "None",
    "Arceuus Library",
    "Draynor Manor",
    "Battlefront",
    "Varrock",
    "Mind Altar",
    "Lumbridge",
    "Falador",
    "Salve Graveyard",
    "Camelot",
    "Fenkenstrain's Castle",
    "Kourend Castle",
    "East Ardougne",
    "Civitas illa Fortis",
    "Watchtower",
    "Senntisten",
    "West Ardougne",
    "Marim",
    "Harmony Island",
    "Kharyrll",
    "Moonclan",
    "Cemetery",
    "Waterbirth Island",
    "Barrows",
    "Carrallanger",
    "Fishing Guild",
    "Catherby",
    "Annakarl",
    "Ape Atoll Dungeon",
    "Ghorrock",
    "Troll Stronghold",
    "Weiss",
];
const LARDERS: [&str; 4] = ["None", "Wooden", "Oak", "Teak"];
const LECTERNS: [&str; 9] = [
    "None",
    "Oak",
    "Eagle",
    "Demon",
    "Teak eagle",
    "Teak demon",
    "Mahogany eagle",
    "Mahogany demon",
    "Marble",
];
const CHAPEL_ALTARS: [&str; 8] = ["None", "Oak", "Teak", "Cloth", "Mahogany", "Limestone", "Marble", "Gilded"];
const POOLS: [&str; 6] = ["None", "Restoration", "Revitalisation", "Rejuvenation", "Fancy rejuvenation", "Ornate rejuvenation"];
const GARDEN_TELEPORTS: [&str; 5] = ["None", "Spirit tree", "Obelisk", "Fairy ring", "Spirit tree and fairy ring"];
const BOXES: [&str; 4] = ["None", "Basic", "Fancy", "Ornate"];
const SPELL_ALTARS: [&str; 5] = ["None", "Ancient", "Lunar", "Dark", "Occult"];
const AMULETS: [&str; 3] = ["None", "Xeric's talisman", "Digsite pendant"];
const YES_NO: [&str; 2] = ["No", "Yes"];
const TROPHIES: [&str; 3] = ["None", "Amulet of glory", "Mythical cape"];

const EXIT: u8 = 0;
const LARDER: u8 = 1;
const TEAK_LARDER: u8 = 3;
const REPAIR: u8 = 4;
const SERVANT: u8 = 5;
const FANCY_RANGE: u8 = 6;
const GLORY: u8 = 7;
const MYTH: u8 = 8;
const OAK_LECTERN: u8 = 9;
const MARBLE_LECTERN: u8 = 16;
const LEFT_BURNER: u8 = 17;
const RIGHT_BURNER: u8 = 18;
const OAK_ALTAR: u8 = 19;
const GILDED_ALTAR: u8 = 25;
const LEFT_PORTAL: u8 = 26;
const MIDDLE_PORTAL: u8 = 27;
const RIGHT_PORTAL: u8 = 28;
const NEXUS: u8 = 29;
const LEFT_AMULET: u8 = 30;
const RIGHT_AMULET: u8 = 31;
const RESTORATION: u8 = 32;
const ORNATE_POOL: u8 = 36;
const SPIRIT_TREE: u8 = 37;
const SPIRITUAL: u8 = 40;
const BASIC_BOX: u8 = 41;
const ORNATE_BOX: u8 = 43;
const ANCIENT: u8 = 44;
const OCCULT: u8 = 47;
const CAPE: u8 = 48;
const HIGH_OBJECT: i64 = 48;
const HIGH_TELEPORT: i64 = 33;
const XERIC: u8 = 33;
const DIGSITE: u8 = 32;

const PORTAL_CHAMBER: u8 = 17;
const SUPERIOR_GARDEN: u8 = 20;
const PORTAL_NEXUS: u8 = 21;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    room: u8,
    rotation: u8,
    objects: Vec<u8>,
    portals: [u8; 3],
}

impl Cell {
    fn empty() -> Self {
        Self { room: 0, rotation: 0, objects: Vec::new(), portals: [0, 0, 0] }
    }
}

pub struct Dialog {
    open: bool,
    wants_open: bool,
    account: String,
    index: usize,
    location: usize,
    decoration: usize,
    teleport: usize,
    amount: usize,
    size: i64,
    cells: Vec<Cell>,
    selected: (usize, usize),
    warning: String,
    error: String,
    note: String,
    extra: Map<String, Value>,
    tiles: HashMap<(u8, u8, usize), egui::TextureHandle>,
    read_rx: Option<Receiver<String>>,
    /// House coordinate captured when a square was right-clicked.
    menu_cell: Option<(usize, usize)>,
}

struct Clicked {
    save: bool,
    cancel: bool,
    read: bool,
    clear: bool,
}

impl Dialog {
    pub fn load(paths: &SimbaPaths, account: &str, index: usize) -> Self {
        let mut dialog = Self::blank(account, index);
        let Some(path) = read_path(paths, account, index) else {
            return dialog;
        };
        let Ok(text) = fs::read_to_string(path) else {
            return dialog;
        };
        if text.trim().is_empty() {
            return dialog;
        }
        let mut map = match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            Ok(Value::Object(map)) => map,
            _ => {
                dialog.error = "House settings could not be read. Defaults are shown, and Done replaces the file.".into();
                return dialog;
            }
        };
        dialog.size = take_i64(&mut map, "size", 32).clamp(1, 256);
        dialog.amount = take_i64(&mut map, "amount", 13).clamp(1, 25) as usize;
        dialog.teleport = take_i64(&mut map, "teleport", 0).clamp(0, 2) as usize;
        dialog.cells = vec![Cell::empty(); dialog.amount * dialog.amount];
        let layout = take_string(&mut map, "layout", "");
        if !layout.trim().is_empty() {
            match decode_layout(&layout, dialog.amount) {
                Ok(decoded) => {
                    dialog.location = decoded.location.min(LOCATIONS.len() - 1);
                    dialog.decoration = decoded.decoration.min(STYLES.len() - 1);
                    if decoded.amount > dialog.amount {
                        dialog.amount = decoded.amount.min(25);
                        dialog.cells = vec![Cell::empty(); dialog.amount * dialog.amount];
                    }
                    for (at, cell) in decoded.cells {
                        if let Some(slot) = dialog.cells.get_mut(at.1 * dialog.amount + at.0) {
                            *slot = cell;
                        }
                    }
                    dialog.warning = decoded.warning;
                }
                Err(message) => dialog.error = message,
            }
        }
        dialog.extra = map;
        dialog.selected = dialog.first_room();
        dialog
    }

    fn blank(account: &str, index: usize) -> Self {
        let amount = 13;
        Self {
            open: false,
            wants_open: false,
            account: account.trim().to_string(),
            index,
            location: 0,
            decoration: 0,
            teleport: 0,
            amount,
            size: 32,
            cells: vec![Cell::empty(); amount * amount],
            selected: (amount / 2, amount / 2),
            warning: String::new(),
            error: String::new(),
            note: String::new(),
            extra: Map::new(),
            tiles: HashMap::new(),
            read_rx: None,
            menu_cell: None,
        }
    }

    pub fn request(&mut self) {
        self.wants_open = true;
    }

    pub fn has_account(&self) -> bool {
        let name = self.account.trim();
        !name.is_empty() && !name.eq_ignore_ascii_case("NoUserNameSelected")
    }

    pub fn status_line(&self) -> String {
        if !self.has_account() {
            return "Add an account to set up the house.".into();
        }
        let rooms = self.filled();
        if rooms == 0 {
            return "House not set up".into();
        }
        let place = LOCATIONS.get(self.location).copied().unwrap_or("House");
        format!("House set · {place} · {rooms} rooms")
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
            fs::create_dir_all(parent).map_err(|error| format!("Could not create the house folder: {error}"))?;
        }
        let mut map = self.extra.clone();
        map.insert("size".into(), json!(self.size));
        map.insert("amount".into(), json!(self.amount as i64));
        map.insert("teleport".into(), json!(self.teleport as i64));
        map.insert("layout".into(), json!(encode_layout(self)));
        let text = serde_json::to_string_pretty(&Value::Object(map)).map_err(|error| format!("Could not write house settings: {error}"))?;
        fs::write(path, format!("{text}\n")).map_err(|error| format!("Could not save house settings: {error}"))
    }

    fn filled(&self) -> usize {
        self.cells.iter().filter(|cell| cell.room != 0).count()
    }

    fn first_room(&self) -> (usize, usize) {
        for y in 0..self.amount {
            for x in 0..self.amount {
                if self.cells[y * self.amount + x].room != 0 {
                    return (x, y);
                }
            }
        }
        (self.amount / 2, self.amount / 2)
    }

    fn cell_mut(&mut self) -> &mut Cell {
        let (x, y) = self.selected;
        let index = y * self.amount + x;
        &mut self.cells[index]
    }

    fn start_garden(&mut self) {
        let center = self.amount / 2;
        let cell = &mut self.cells[center * self.amount + center];
        *cell = Cell { room: 2, rotation: 0, objects: vec![EXIT], portals: [0, 0, 0] };
        self.selected = (center, center);
    }

    /// A layout with no rooms is the garden and exit portal. That is the blank house.
    fn ensure_default_garden(&mut self) {
        if self.filled() == 0 && self.read_rx.is_none() {
            self.start_garden();
        }
    }

    fn is_default_garden(&self) -> bool {
        let center = self.amount / 2;
        let garden = center * self.amount + center;
        self.cells.iter().enumerate().all(|(index, cell)| {
            if index == garden {
                cell.room == 2 && cell.rotation == 0 && cell.objects == vec![EXIT] && cell.portals == [0, 0, 0]
            } else {
                cell.room == 0
            }
        })
    }

    fn clear_layout(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::empty();
        }
        self.note.clear();
        self.warning.clear();
        self.menu_cell = None;
        self.start_garden();
    }

    fn start_read(&mut self, paths: &SimbaPaths, hwnd: isize) {
        if self.read_rx.is_some() {
            return;
        }
        let simba = paths.simba_exe();
        if !simba.is_file() {
            self.error = format!("Simba was not found at {}.", simba.display());
            self.note.clear();
            return;
        }
        let script = paths.include("BashLib").join("optional").join("handlers").join("house").join("readclient.simba");
        if !script.is_file() {
            self.error = "The house reader script is missing.".into();
            self.note.clear();
            return;
        }
        let result = std::env::temp_dir().join(format!("bash-house-read-{}-{hwnd}.json", std::process::id()));
        let _ = fs::remove_file(&result);
        let root = paths.root.clone();
        let account = self.account.clone();
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("house-read".into())
            .spawn(move || {
                let _ = tx.send(run_client_read(&root, &simba, &script, &result, hwnd, &account));
            })
            .ok();
        self.read_rx = Some(rx);
        self.note = "Reading the house from the client. Leave the house viewer open.".into();
        self.error.clear();
    }
}

pub fn show(ctx: &egui::Context, paths: &SimbaPaths, dialog: &mut Dialog, account: &str, index: usize) {
    if std::mem::take(&mut dialog.wants_open) && !dialog.open {
        *dialog = Dialog::load(paths, account, index);
        dialog.open = true;
    } else if dialog.account.trim() != account.trim() && !dialog.open {
        *dialog = Dialog::load(paths, account, index);
    }
    if !dialog.open {
        return;
    }
    poll_read(ctx, dialog);
    dialog.ensure_default_garden();
    let mut clicked = Clicked { save: false, cancel: false, read: false, clear: false };
    egui::Modal::new(egui::Id::new("bash-house")).show(ctx, |ui| {
        ui.set_min_width(880.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("House setup").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    clicked.cancel = true;
                }
            });
        });
        ui.add_space(6.0);
        draw(ui, dialog, &mut clicked);
    });
    if clicked.clear {
        dialog.clear_layout();
    }
    if clicked.read {
        if let Some(client) = client_for_account(&dialog.account) {
            dialog.start_read(paths, client.hwnd);
            ctx.request_repaint();
        }
    }
    if clicked.cancel || (!dialog.open && !clicked.save) {
        let account = dialog.account.clone();
        let index = dialog.index;
        *dialog = Dialog::load(paths, &account, index);
        dialog.open = false;
    }
    if clicked.save {
        match dialog.save(paths) {
            Ok(()) => dialog.open = false,
            Err(message) => dialog.error = message,
        }
    }
}

fn draw(ui: &mut egui::Ui, dialog: &mut Dialog, clicked: &mut Clicked) {
    ui.set_min_width(880.0);
    ui.label(RichText::new("Lay out this account's house so scripts can walk to the portal, pool, nexus, and other rooms.").weak());
    ui.label(RichText::new(format!("Saved for {}.", dialog.account.trim())).weak());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        labelled_choice(ui, "house-location", "House location", "Which portal this house is built at.", &mut dialog.location, &LOCATIONS);
        ui.add_space(16.0);
        labelled_choice(ui, "house-style", "House style", "The floor style. The pictures use the same colours the script matches.", &mut dialog.decoration, &STYLES);
    });
    ui.add_space(10.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Layout").strong());
            let busy = dialog.read_rx.is_some();
            let ready = client_for_account(&dialog.account).is_some();
            ui.horizontal(|ui| {
                let label = if busy { "Reading..." } else { "Read from Client" };
                if ui
                    .add_enabled(ready && !busy, egui::Button::new(label))
                    .on_hover_text(if ready {
                        "Starts Simba on this account's RuneLite, reads the open house viewer, and fills this layout."
                    } else {
                        "Log into this account in RuneLite first."
                    })
                    .clicked()
                {
                    clicked.read = true;
                }
                crate::ui::help_tip(
                    ui,
                    "Open the house viewer on that client, with the smallest room icons. Remote input is used to look at the client.",
                );
                if ui
                    .add_enabled(!dialog.is_default_garden() && !busy, egui::Button::new("Clear all"))
                    .on_hover_text("Removes every room and puts the garden and exit portal back. Location and house style stay.")
                    .clicked()
                    && crate::ui::ask_yes_no(
                        "Clear every room in this house?\n\nLocation and house style stay. The garden and exit portal are put back. Press Done to save.",
                    )
                {
                    clicked.clear = true;
                }
            });
            if !ready {
                ui.label(RichText::new("Log into this account in RuneLite to read the house.").weak());
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Click a room, or right-click a square to change it. Rotate until it faces the same way as in the game.").weak());
            ui.add_space(4.0);
            draw_grid(ui, dialog);
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.set_min_width(280.0);
            draw_room(ui, dialog);
        });
    });
    if !dialog.error.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(&dialog.error).color(egui::Color32::from_rgb(220, 120, 110)));
    }
    if !dialog.note.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(&dialog.note).weak());
    } else if dialog.error.is_empty() && !dialog.warning.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(&dialog.warning).color(egui::Color32::from_rgb(210, 170, 90)));
    }
    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                clicked.cancel = true;
            }
            if ui.button("Done").on_hover_text("Saves this account's house.").clicked() {
                clicked.save = true;
            }
        });
    });
}

fn draw_grid(ui: &mut egui::Ui, dialog: &mut Dialog) {
    let (x0, y0, x1, y1) = view_bounds(dialog);
    let decoration = dialog.decoration;
    let amount = dialog.amount;
    ScrollArea::both().id_salt("house-grid").max_height(480.0).show(ui, |ui| {
        egui::Grid::new("house-cells").spacing([4.0, 4.0]).show(ui, |ui| {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    ui.push_id((x, y), |ui| {
                        let (room, rotation, left_amulet, right_amulet) = {
                            let cell = &dialog.cells[y * amount + x];
                            (cell.room, cell.rotation, cell.portals[0], cell.portals[2])
                        };
                        let selected = dialog.selected == (x, y);
                        let id = room_texture(ui.ctx(), &mut dialog.tiles, room, rotation, decoration).id();
                        let image = egui::Image::new(egui::load::SizedTexture::new(id, egui::vec2(32.0, 32.0)))
                            .fit_to_exact_size(egui::vec2(64.0, 64.0))
                            .sense(egui::Sense::click());
                        let hover = format!(
                            "Column {x}, row {y}. {}. Right-click to change the room.",
                            ROOMS.get(room as usize).copied().unwrap_or("Room")
                        );
                        let response = ui.add(image).on_hover_text(hover);
                        if selected {
                            ui.painter().rect_stroke(
                                response.rect.expand(1.0),
                                0.0,
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 196, 64)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if room == PORTAL_NEXUS {
                            draw_amulet_mark(ui, response.rect, rotation, left_amulet, (7.5, 20.0));
                            draw_amulet_mark(ui, response.rect, rotation, right_amulet, (23.5, 20.0));
                        }
                        if response.secondary_clicked() {
                            dialog.menu_cell = Some((x, y));
                        }
                        response.context_menu(|ui| {
                            let (cx, cy) = dialog.menu_cell.unwrap_or((x, y));
                            let current = dialog.cells.get(cy * amount + cx).map(|cell| cell.room as usize).unwrap_or(0);
                            ui.set_min_width(180.0);
                            for (index, name) in ROOMS.iter().enumerate() {
                                if ui.selectable_label(current == index, *name).clicked() {
                                    if let Some(cell) = dialog.cells.get_mut(cy * amount + cx) {
                                        assign_room(cell, index as u8);
                                    }
                                    dialog.selected = (cx, cy);
                                    dialog.menu_cell = None;
                                    ui.close_menu();
                                }
                            }
                        });
                        if response.clicked() {
                            dialog.selected = (x, y);
                        }
                    });
                }
                ui.end_row();
            }
        });
    });
}

fn draw_room(ui: &mut egui::Ui, dialog: &mut Dialog) {
    let (x, y) = dialog.selected;
    ui.label(RichText::new("Selected room").strong());
    ui.label(RichText::new(format!("Column {x}, row {y}.")).weak());
    ui.add_space(6.0);
    let room = dialog.cell_mut().room as usize;
    if let Some(picked) = choice(ui, "house-room", "Room", &ROOMS, room) {
        assign_room(dialog.cell_mut(), picked as u8);
    }
    let room = dialog.cell_mut().room;
    ui.add_space(6.0);
    if room != 0 {
        ui.horizontal(|ui| {
            if ui.button("Rotate left").clicked() {
                turn(dialog.cell_mut(), -1);
            }
            if ui.button("Rotate right").clicked() {
                turn(dialog.cell_mut(), 1);
            }
        });
        ui.add_space(8.0);
    }
    ui.label(RichText::new("In this room").strong());
    ui.add_space(4.0);
    match room {
        0 => {
            ui.label("Pick a room to add it here.");
        }
        2 | 18 => flag_choice(ui, "house-exit", "Exit portal", "The portal you click to leave the house.", dialog.cell_mut(), EXIT, true),
        3 => {
            span_choice(ui, "house-larder", "Larder", &LARDERS, dialog.cell_mut(), LARDER, TEAK_LARDER);
            ui.add_space(4.0);
            span_choice(ui, "house-range", "Range", &["None", "Fancy"], dialog.cell_mut(), FANCY_RANGE, FANCY_RANGE);
        }
        5 => flag_choice(ui, "house-repair", "Armour repair stand", "The stand used to repair armour.", dialog.cell_mut(), REPAIR, true),
        6 => flag_choice(ui, "house-servant", "Servant money bag", "The bag that pays the servant.", dialog.cell_mut(), SERVANT, true),
        11 => trophy(ui, dialog.cell_mut()),
        14 => span_choice(ui, "house-lectern", "Lectern", &LECTERNS, dialog.cell_mut(), OAK_LECTERN, MARBLE_LECTERN),
        16 => {
            span_choice(ui, "house-chapel-altar", "Altar", &CHAPEL_ALTARS, dialog.cell_mut(), OAK_ALTAR, GILDED_ALTAR);
            ui.add_space(4.0);
            burners(ui, dialog.cell_mut());
        }
        17 => portals(ui, dialog.cell_mut()),
        20 => {
            span_choice(ui, "house-pool", "Pool", &POOLS, dialog.cell_mut(), RESTORATION, ORNATE_POOL);
            ui.add_space(4.0);
            span_choice(ui, "house-garden-teleport", "Teleport", &GARDEN_TELEPORTS, dialog.cell_mut(), SPIRIT_TREE, SPIRITUAL);
        }
        21 => nexus(ui, dialog.cell_mut()),
        22 => {
            span_choice(ui, "house-box", "Jewellery box", &BOXES, dialog.cell_mut(), BASIC_BOX, ORNATE_BOX);
            ui.add_space(4.0);
            span_choice(ui, "house-spell-altar", "Altar", &SPELL_ALTARS, dialog.cell_mut(), ANCIENT, OCCULT);
            ui.add_space(4.0);
            flag_choice(ui, "house-cape", "Cape hanger", "The hanger used for cape teleports.", dialog.cell_mut(), CAPE, false);
        }
        _ => {
            ui.label("Nothing else to set in this room.");
        }
    }
    if room != 0 {
        ui.add_space(10.0);
        if ui.button("Clear this room").clicked() {
            *dialog.cell_mut() = Cell::empty();
        }
    }
}

fn labelled_choice(ui: &mut egui::Ui, id: &str, label: &str, tip: &str, value: &mut usize, options: &[&str]) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            crate::ui::help_tip(ui, tip);
        });
        let current = *value;
        if let Some(picked) = choice(ui, id, "", options, current) {
            *value = picked;
        }
    });
}

fn choice(ui: &mut egui::Ui, id: &str, label: &str, options: &[&str], current: usize) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    if !label.is_empty() {
        ui.label(label);
    }
    let mut value = current.min(options.len() - 1);
    let shown = options[value];
    let mut changed = false;
    ComboBox::from_id_salt(id).width(240.0).selected_text(shown).show_ui(ui, |ui| {
        for (index, option) in options.iter().enumerate() {
            if ui.selectable_value(&mut value, index, *option).clicked() {
                changed = true;
            }
        }
    });
    if changed && value != current { Some(value) } else { None }
}

fn flag_choice(ui: &mut egui::Ui, id: &str, label: &str, tip: &str, cell: &mut Cell, object: u8, replace_all: bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        crate::ui::help_tip(ui, tip);
    });
    let current = usize::from(cell.objects.contains(&object));
    if let Some(picked) = choice(ui, id, "", &YES_NO, current) {
        if replace_all {
            cell.objects.clear();
            if picked == 1 {
                cell.objects.push(object);
            }
        } else if picked == 1 {
            insert_object(&mut cell.objects, object);
        } else {
            cell.objects.retain(|id| *id != object);
        }
    }
}

fn span_choice(ui: &mut egui::Ui, id: &str, label: &str, options: &[&str], cell: &mut Cell, start: u8, end: u8) {
    ui.label(label);
    let current = span_index(&cell.objects, start, end).unwrap_or(0);
    let shown = if span_index(&cell.objects, start, end).is_none() && has_span(&cell.objects, start, end) {
        "As saved"
    } else {
        options.get(current).copied().unwrap_or(options[0])
    };
    let mut value = current.min(options.len() - 1);
    let mut changed = false;
    ComboBox::from_id_salt(id).width(240.0).selected_text(shown).show_ui(ui, |ui| {
        for (index, option) in options.iter().enumerate() {
            if ui.selectable_value(&mut value, index, *option).clicked() {
                changed = true;
            }
        }
    });
    if changed {
        cell.objects.retain(|id| *id < start || *id > end);
        if value > 0 {
            let id = start + (value as u8 - 1);
            if id <= end {
                insert_object(&mut cell.objects, id);
            }
        }
    }
}

fn trophy(ui: &mut egui::Ui, cell: &mut Cell) {
    ui.label("Mounted trophy");
    let current = if cell.objects.contains(&GLORY) {
        1
    } else if cell.objects.contains(&MYTH) {
        2
    } else if cell.objects.is_empty() {
        0
    } else {
        0
    };
    if let Some(picked) = choice(ui, "house-trophy", "", &TROPHIES, current) {
        cell.objects.clear();
        if picked == 1 {
            cell.objects.push(GLORY);
        } else if picked == 2 {
            cell.objects.push(MYTH);
        }
    }
}

fn burners(ui: &mut egui::Ui, cell: &mut Cell) {
    ui.horizontal(|ui| {
        ui.label("Incense burners");
        crate::ui::help_tip(ui, "Both burners, on either side of the altar.");
    });
    let current = usize::from(cell.objects.contains(&LEFT_BURNER) || cell.objects.contains(&RIGHT_BURNER));
    if let Some(picked) = choice(ui, "house-burners", "", &YES_NO, current) {
        cell.objects.retain(|id| *id != LEFT_BURNER && *id != RIGHT_BURNER);
        if picked == 1 {
            insert_object(&mut cell.objects, LEFT_BURNER);
            insert_object(&mut cell.objects, RIGHT_BURNER);
        }
    }
}

fn portals(ui: &mut egui::Ui, cell: &mut Cell) {
    portal_slot(ui, "house-left-portal", "Left portal", 0, LEFT_PORTAL, cell);
    ui.add_space(4.0);
    portal_slot(ui, "house-middle-portal", "Middle portal", 1, MIDDLE_PORTAL, cell);
    ui.add_space(4.0);
    portal_slot(ui, "house-right-portal", "Right portal", 2, RIGHT_PORTAL, cell);
}

fn portal_slot(ui: &mut egui::Ui, id: &str, label: &str, slot: usize, object: u8, cell: &mut Cell) {
    ui.label(label);
    let current = cell.portals[slot] as usize;
    let current = if current < PORTALS.len() { current } else { 0 };
    if let Some(picked) = choice(ui, id, "", &PORTALS, current) {
        cell.objects.retain(|id| *id != object);
        cell.portals[slot] = picked as u8;
        if picked > 0 {
            insert_object(&mut cell.objects, object);
        }
    }
}

fn nexus(ui: &mut egui::Ui, cell: &mut Cell) {
    amulet_slot(ui, "house-left-amulet", "Left side", 0, LEFT_AMULET, cell);
    ui.add_space(4.0);
    amulet_slot(ui, "house-right-amulet", "Right side", 2, RIGHT_AMULET, cell);
    ui.add_space(4.0);
    flag_choice(ui, "house-nexus", "Portal nexus", "The nexus in the middle of the room.", cell, NEXUS, false);
}

fn amulet_slot(ui: &mut egui::Ui, id: &str, label: &str, slot: usize, object: u8, cell: &mut Cell) {
    ui.label(label);
    let current = match cell.portals[slot] {
        XERIC => 1,
        DIGSITE => 2,
        _ => 0,
    };
    if let Some(picked) = choice(ui, id, "", &AMULETS, current) {
        cell.objects.retain(|id| *id != object);
        cell.portals[slot] = match picked {
            1 => XERIC,
            2 => DIGSITE,
            _ => 0,
        };
        if picked > 0 {
            insert_object(&mut cell.objects, object);
        }
    }
}

fn room_texture<'a>(
    ctx: &egui::Context,
    cache: &'a mut HashMap<(u8, u8, usize), egui::TextureHandle>,
    room: u8,
    rotation: u8,
    decoration: usize,
) -> &'a egui::TextureHandle {
    let room = room.min(ROOMS.len() as u8 - 1);
    let rotation = rotation % 4;
    let decoration = decoration.min(STYLES.len() - 1);
    let key = (room, rotation, decoration);
    if !cache.contains_key(&key) {
        let image = compose_room(room, rotation, decoration);
        let size = [image.width() as usize, image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = ctx.load_texture(
            format!("house-{room}-{rotation}-{decoration}"),
            color,
            egui::TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
                mipmap_mode: None,
            },
        );
        cache.insert(key, texture);
    }
    cache.get(&key).expect("room texture")
}

fn compose_room(room: u8, rotation: u8, decoration: usize) -> RgbaImage {
    let sheet = room_sheet();
    let index = room as usize;
    let floor = sheet.floors.get(index).unwrap_or(&sheet.floors[0]);
    let icon = sheet.icons.get(index).unwrap_or(&sheet.icons[0]);
    let paint = floor_rgb(decoration, outdoors(room));
    let mut image = RgbaImage::new(floor.width(), floor.height());
    for (x, y, pixel) in floor.enumerate_pixels() {
        let mut rgb = [pixel[0], pixel[1], pixel[2]];
        if rgb == [255, 255, 255] {
            rgb = paint;
        }
        let mark = icon.get_pixel(x, y).0;
        if mark[0] != 0 || mark[1] != 0 || mark[2] != 0 {
            rgb = [mark[0], mark[1], mark[2]];
            if rgb == [255, 255, 255] {
                rgb = paint;
            }
        }
        image.put_pixel(x, y, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    }
    for _ in 0..rotation {
        image = rotate_clockwise(&image);
    }
    image
}

fn rotate_clockwise(source: &RgbaImage) -> RgbaImage {
    let (width, height) = source.dimensions();
    let mut turned = RgbaImage::new(height, width);
    for y in 0..height {
        for x in 0..width {
            turned.put_pixel(height - 1 - y, x, *source.get_pixel(x, y));
        }
    }
    turned
}

fn outdoors(room: u8) -> bool {
    matches!(room, 0 | 2 | 12 | 18 | 20)
}

fn floor_rgb(decoration: usize, outdoors: bool) -> [u8; 3] {
    const COLORS: [[ [u8; 3]; 2]; 10] = [
        [[103, 145, 45], [143, 128, 89]],
        [[103, 145, 45], [126, 97, 66]],
        [[209, 196, 130], [155, 150, 112]],
        [[123, 112, 76], [131, 112, 69]],
        [[103, 145, 45], [143, 128, 89]],
        [[103, 145, 45], [166, 158, 120]],
        [[103, 145, 45], [167, 134, 87]],
        [[32, 27, 27], [89, 84, 84]],
        [[103, 145, 45], [167, 134, 87]],
        [[215, 210, 210], [167, 134, 87]],
    ];
    let pair = COLORS.get(decoration).copied().unwrap_or(COLORS[0]);
    if outdoors { pair[0] } else { pair[1] }
}

struct RoomSheet {
    floors: Vec<RgbaImage>,
    icons: Vec<RgbaImage>,
}

fn room_sheet() -> &'static RoomSheet {
    static SHEET: OnceLock<RoomSheet> = OnceLock::new();
    SHEET.get_or_init(|| RoomSheet {
        floors: slice_tiles(include_bytes!("../../../BashLib/osr/walker/house.png")),
        icons: slice_tiles(include_bytes!("../../../BashLib/osr/walker/house_icons.png")),
    })
}

fn slice_tiles(bytes: &[u8]) -> Vec<RgbaImage> {
    let image = image::load_from_memory(bytes).map(|image| image.to_rgba8()).unwrap_or_else(|_| RgbaImage::new(32, 32));
    let tile = 32;
    let count = (image.width() / tile).max(1);
    (0..count)
        .map(|index| image::imageops::crop_imm(&image, index * tile, 0, tile, tile).to_image())
        .collect()
}

fn client_for_account(account: &str) -> Option<crate::clients::ClientWindow> {
    let want = account.trim();
    if want.is_empty() || want.eq_ignore_ascii_case("NoUserNameSelected") {
        return None;
    }
    crate::clients::game_clients().into_iter().find(|client| client.account().eq_ignore_ascii_case(want))
}

fn poll_read(ctx: &egui::Context, dialog: &mut Dialog) {
    let message = {
        let Some(rx) = &dialog.read_rx else { return };
        match rx.try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(200));
                return;
            }
            Err(TryRecvError::Disconnected) => None,
        }
    };
    dialog.read_rx = None;
    let Some(message) = message else {
        dialog.error = "The house read stopped before it finished.".into();
        dialog.note.clear();
        return;
    };
    if let Some(text) = message.strip_prefix("ERR ") {
        dialog.error = text.to_string();
        dialog.note.clear();
        return;
    }
    match apply_client_read(dialog, &message) {
        Ok(note) => {
            dialog.note = note;
            dialog.error.clear();
            dialog.warning.clear();
        }
        Err(error) => {
            dialog.error = error;
            dialog.note.clear();
        }
    }
}

fn run_client_read(root: &std::path::Path, simba: &std::path::Path, script: &std::path::Path, result: &std::path::Path, hwnd: isize, account: &str) -> String {
    let mut command = Command::new(simba);
    command
        .current_dir(root)
        .arg(format!("--target={}", hwnd as i32))
        .arg("--run")
        .arg(script)
        .env("BASH_HOUSE_RESULT", result)
        .env("BASH_ACCOUNT", account)
        .env("BASH_LAUNCHER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return format!("ERR Could not start Simba: {error}"),
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_thread = thread::spawn(move || read_pipe(stdout));
    let err_thread = thread::spawn(move || read_pipe(stderr));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(120) => {
                let _ = child.kill();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return "ERR Reading the house took too long. Leave the house viewer open and try again.".into();
            }
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(error) => return format!("ERR Could not read the client: {error}"),
        }
    }
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    if let Ok(text) = fs::read_to_string(result) {
        let _ = fs::remove_file(result);
        if !text.trim().is_empty() {
            return text;
        }
    }
    format!("ERR {}", client_failure(&stdout, &stderr))
}

fn read_pipe(pipe: Option<impl Read>) -> String {
    let Some(mut pipe) = pipe else { return String::new() };
    let mut text = String::new();
    let _ = pipe.read_to_string(&mut text);
    text
}

fn client_failure(stdout: &str, stderr: &str) -> String {
    let combined = format!("{stdout}\n{stderr}");
    if combined.contains("Please refocus the client") || combined.contains("No client targeted") {
        return "That RuneLite window could not be used. Log into this account and try again.".into();
    }
    combined
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("Succesfully") && !line.starts_with("Simba:"))
        .unwrap_or("The house could not be read from the client.")
        .chars()
        .take(180)
        .collect()
}

fn apply_client_read(dialog: &mut Dialog, text: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "The client read could not be understood.".to_string())?;
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    let detail = value.get("detail").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if status != "ok" {
        return Err(if detail.is_empty() { "The house could not be read.".into() } else { detail });
    }
    let top = pair(&value, "top_left");
    let bottom = pair(&value, "bottom_right");
    let center = dialog.amount as i32 / 2;
    let offset_x = (bottom[0] - top[0] - center).abs();
    let offset_y = (bottom[1] - top[1] - center).abs();
    for cell in &mut dialog.cells {
        *cell = Cell::empty();
    }
    let mut placed = 0usize;
    let rooms = value.get("rooms").and_then(Value::as_array).cloned().unwrap_or_default();
    for room in rooms {
        let kind = room.get("room").and_then(Value::as_u64).unwrap_or(0) as u8;
        if kind == 0 || kind as usize >= ROOMS.len() {
            continue;
        }
        let x = room.get("x").and_then(Value::as_i64).unwrap_or(0) as i32 + offset_x + 1;
        let y = room.get("y").and_then(Value::as_i64).unwrap_or(0) as i32 + offset_y + 1;
        let rotation = (room.get("rotation").and_then(Value::as_i64).unwrap_or(0) as i32).rem_euclid(4) as u8;
        if x < 0 || y < 0 || x as usize >= dialog.amount || y as usize >= dialog.amount {
            continue;
        }
        let index = y as usize * dialog.amount + x as usize;
        dialog.cells[index] = Cell { room: kind, rotation, objects: Vec::new(), portals: [0, 0, 0] };
        if placed == 0 {
            dialog.selected = (x as usize, y as usize);
        }
        placed += 1;
    }
    if placed == 0 {
        return Err("Rooms were found, but they did not fit on the house grid.".into());
    }
    let noun = if placed == 1 { "room" } else { "rooms" };
    Ok(format!("Read {placed} {noun} from the client. Set what is inside each room, then press Done."))
}

fn pair(value: &Value, key: &str) -> [i32; 2] {
    let Some(array) = value.get(key).and_then(Value::as_array) else { return [0, 0] };
    let at = |index: usize| array.get(index).and_then(Value::as_i64).unwrap_or(0) as i32;
    [at(0), at(1)]
}

fn view_bounds(dialog: &Dialog) -> (usize, usize, usize, usize) {
    let last = dialog.amount - 1;
    let (sx, sy) = dialog.selected;
    let mut min_x = sx;
    let mut min_y = sy;
    let mut max_x = sx;
    let mut max_y = sy;
    for y in 0..dialog.amount {
        for x in 0..dialog.amount {
            if dialog.cells[y * dialog.amount + x].room != 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    (min_x.saturating_sub(1), min_y.saturating_sub(1), (max_x + 1).min(last), (max_y + 1).min(last))
}

fn turn(cell: &mut Cell, delta: i8) {
    cell.rotation = (cell.rotation as i8 + delta).rem_euclid(4) as u8;
}

fn assign_room(cell: &mut Cell, room: u8) {
    if room == cell.room {
        return;
    }
    let keep_portals = portal_room(cell.room) && portal_room(room);
    let portals = if keep_portals { cell.portals } else { [0, 0, 0] };
    cell.room = room;
    cell.rotation = 0;
    cell.objects.clear();
    cell.portals = portals;
}

/// Xeric's talisman and the Digsite pendant sit on the two lower icons.
/// The letter stays upright and moves with the icon when the room rotates.
fn draw_amulet_mark(ui: &egui::Ui, rect: egui::Rect, rotation: u8, teleport: u8, anchor: (f32, f32)) {
    let letter = match teleport {
        XERIC => "x",
        DIGSITE => "d",
        _ => return,
    };
    let (x, y) = rotate_center(anchor.0, anchor.1, rotation);
    let scale = rect.width() / 32.0;
    let pos = rect.min + egui::vec2(x * scale, y * scale);
    let font = egui::FontId::proportional(14.0);
    let painter = ui.painter();
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        painter.text(pos + egui::vec2(dx, dy), egui::Align2::CENTER_CENTER, letter, font.clone(), egui::Color32::WHITE);
    }
    painter.text(
        pos,
        egui::Align2::CENTER_CENTER,
        letter,
        font,
        egui::Color32::from_rgb(28, 20, 12),
    );
}

fn rotate_center(mut x: f32, mut y: f32, turns: u8) -> (f32, f32) {
    for _ in 0..turns % 4 {
        let next_x = 32.0 - y;
        let next_y = x;
        x = next_x;
        y = next_y;
    }
    (x, y)
}

fn portal_room(room: u8) -> bool {
    matches!(room, PORTAL_CHAMBER | SUPERIOR_GARDEN | PORTAL_NEXUS)
}

fn insert_object(objects: &mut Vec<u8>, id: u8) {
    if !objects.contains(&id) {
        objects.push(id);
        objects.sort_unstable();
    }
}

fn has_span(objects: &[u8], start: u8, end: u8) -> bool {
    objects.iter().any(|id| *id >= start && *id <= end)
}

fn span_index(objects: &[u8], start: u8, end: u8) -> Option<usize> {
    let hits: Vec<u8> = objects.iter().copied().filter(|id| *id >= start && *id <= end).collect();
    if hits.is_empty() {
        Some(0)
    } else if hits.len() == 1 {
        Some((hits[0] - start) as usize + 1)
    } else {
        None
    }
}

fn config_file(paths: &SimbaPaths, account: &str) -> PathBuf {
    paths.configs().join("house").join(format!("{}.json", sanitize_account(account)))
}

fn read_path(paths: &SimbaPaths, account: &str, index: usize) -> Option<PathBuf> {
    let account = sanitize_account(account);
    if account.is_empty() || account.eq_ignore_ascii_case("NoUserNameSelected") {
        return None;
    }
    let path = paths.configs().join("house").join(format!("{account}.json"));
    let legacy = paths.configs().join("house").join(format!("{index}.json"));
    if !path.exists() && path != legacy && legacy.exists() {
        Some(legacy)
    } else if path.exists() {
        Some(path)
    } else {
        None
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

struct Decoded {
    location: usize,
    decoration: usize,
    amount: usize,
    cells: Vec<((usize, usize), Cell)>,
    warning: String,
}

fn encode_layout(dialog: &Dialog) -> String {
    let mut text = format!("[5 {HIGH_OBJECT} {HIGH_TELEPORT} {} {}]", dialog.location, dialog.decoration);
    for y in 0..dialog.amount {
        for x in 0..dialog.amount {
            let cell = &dialog.cells[y * dialog.amount + x];
            text.push_str(&format!("[{x} {y} {} {}", cell.room, cell.rotation));
            let mut objects = cell.objects.clone();
            objects.sort_unstable();
            objects.dedup();
            if !objects.is_empty() {
                let list = objects.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ");
                text.push(' ');
                text.push_str(&list);
            }
            if portal_room(cell.room) {
                text.push_str(&format!(" ({} {} {})", cell.portals[0], cell.portals[1], cell.portals[2]));
            }
            text.push(']');
        }
    }
    simba_compress(&text)
}

fn decode_layout(encoded: &str, amount: usize) -> Result<Decoded, String> {
    let text = simba_decompress(encoded).map_err(|error| format!("House layout could not be read. {error}"))?;
    let elements = brackets(&text);
    if elements.is_empty() {
        return Err("House layout could not be read.".into());
    }
    let header: Vec<&str> = elements[0].split_whitespace().collect();
    if header.len() < 5 || header[0] != header.len().to_string() {
        return Err("This house file is from an older layout and could not be loaded.".into());
    }
    let skip_objects = header[1].parse::<i64>().ok() != Some(HIGH_OBJECT) || header[2].parse::<i64>().ok() != Some(HIGH_TELEPORT);
    let location = header[3].parse::<usize>().unwrap_or(0);
    let decoration = header[4].parse::<usize>().unwrap_or(0);
    let mut cells = Vec::new();
    let mut used = amount;
    for element in elements.iter().skip(1) {
        let (main, portals) = split_portals(element);
        let values: Vec<&str> = main.split_whitespace().collect();
        if values.len() < 4 {
            continue;
        }
        let Ok(x) = values[0].parse::<usize>() else { continue };
        let Ok(y) = values[1].parse::<usize>() else { continue };
        let parsed = values[2].parse::<u8>().unwrap_or(0);
        let room = if parsed as usize >= ROOMS.len() { 0 } else { parsed };
        let rotation = values[3].parse::<u8>().unwrap_or(0) % 4;
        let mut objects = Vec::new();
        let mut portal_values = [0, 0, 0];
        if !skip_objects {
            for token in values.iter().skip(4) {
                if let Ok(id) = token.parse::<u8>() {
                    if id <= CAPE {
                        insert_object(&mut objects, id);
                    }
                }
            }
            let portal_words: Vec<&str> = portals.split_whitespace().collect();
            if portal_words.len() >= 3 {
                portal_values[0] = portal_words[0].parse().unwrap_or(0);
                portal_values[1] = portal_words[1].parse().unwrap_or(0);
                portal_values[2] = portal_words[2].parse().unwrap_or(0);
            }
        }
        used = used.max(x + 1).max(y + 1);
        cells.push(((x, y), Cell { room, rotation, objects, portals: portal_values }));
    }
    let warning = if skip_objects {
        "This house was saved by an older version. The rooms are here, but the furniture needs to be set again.".into()
    } else {
        String::new()
    };
    Ok(Decoded { location, decoration, amount: used, cells, warning })
}

fn split_portals(element: &str) -> (&str, &str) {
    match element.split_once(" (") {
        Some((main, rest)) => (main, rest.trim().trim_end_matches(')')),
        None => (element, ""),
    }
}

fn brackets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let inside = &after[..end];
        if !inside.is_empty() {
            out.push(inside.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

fn simba_compress(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::new();
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    let mut encoder = ZlibEncoder::new(output, Compression::new(6));
    encoder.write_all(bytes).ok();
    let compressed = encoder.finish().unwrap_or_default();
    base64::engine::general_purpose::STANDARD.encode(compressed)
}

fn simba_decompress(encoded: &str) -> Result<String, String> {
    let cleaned: String = encoded.chars().filter(|ch| !ch.is_whitespace()).collect();
    let raw = base64::engine::general_purpose::STANDARD.decode(cleaned).map_err(|_| "The layout text is not valid.".to_string())?;
    if raw.len() < 5 {
        return Err("The layout text is too short.".into());
    }
    let mut decoder = ZlibDecoder::new(&raw[4..]);
    let mut text = String::new();
    decoder.read_to_string(&mut text).map_err(|_| "The layout text could not be opened.".to_string())?;
    Ok(text)
}

fn take_string(map: &mut Map<String, Value>, key: &str, default: &str) -> String {
    match map.remove(key) {
        Some(Value::String(text)) => text,
        Some(Value::Number(number)) => number.to_string(),
        _ => default.to_string(),
    }
}

fn take_i64(map: &mut Map<String, Value>, key: &str, default: i64) -> i64 {
    match map.remove(key) {
        Some(Value::Number(number)) => number.as_i64().or_else(|| number.as_f64().map(|value| value as i64)).unwrap_or(default),
        Some(Value::String(text)) => text.trim().parse().unwrap_or(default),
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amulet_letter_stays_beside_the_icon_when_the_room_turns() {
        let (x, y) = rotate_center(7.5, 20.0, 1);
        assert!((x - 12.0).abs() < 0.01);
        assert!((y - 7.5).abs() < 0.01);
        let (x, y) = rotate_center(23.5, 20.0, 0);
        assert!((x - 23.5).abs() < 0.01);
        assert!((y - 20.0).abs() < 0.01);
    }

    #[test]
    fn client_rooms_use_the_same_offset_as_the_script() {
        let mut dialog = Dialog::blank("Hero", 0);
        let text = r#"{"status":"ok","detail":"","top_left":[0,0],"bottom_right":[2,1],"rooms":[{"room":2,"x":0,"y":0,"rotation":1}]}"#;
        let note = apply_client_read(&mut dialog, text).unwrap();
        assert_eq!(dialog.cells[6 * 13 + 5].room, 2);
        assert_eq!(dialog.cells[6 * 13 + 5].rotation, 1);
        assert!(note.contains("1 room"));
    }

    #[test]
    fn reads_simba_compressed_text() {
        let text = simba_decompress("CwAAAHic80jNyclXyMgvLU4FABh4BDk=").unwrap();
        assert_eq!(text, "Hello house");
    }

    #[test]
    fn layout_round_trip_keeps_rooms_and_furniture() {
        let mut dialog = Dialog::blank("Hero", 0);
        dialog.location = 3;
        dialog.decoration = 6;
        dialog.teleport = 2;
        dialog.cells[6 * 13 + 6] = Cell { room: 2, rotation: 1, objects: vec![EXIT], portals: [0, 0, 0] };
        dialog.cells[6 * 13 + 7] = Cell {
            room: PORTAL_CHAMBER,
            rotation: 2,
            objects: vec![LEFT_PORTAL, RIGHT_PORTAL],
            portals: [4, 0, 7],
        };
        dialog.cells[5 * 13 + 6] = Cell { room: 3, rotation: 0, objects: vec![2, FANCY_RANGE], portals: [0, 0, 0] };
        let encoded = encode_layout(&dialog);
        let decoded = decode_layout(&encoded, 13).unwrap();
        assert_eq!(decoded.location, 3);
        assert_eq!(decoded.decoration, 6);
        assert!(decoded.warning.is_empty());
        let garden = decoded.cells.iter().find(|((x, y), _)| *x == 6 && *y == 6).unwrap();
        assert_eq!(garden.1.room, 2);
        assert_eq!(garden.1.rotation, 1);
        assert_eq!(garden.1.objects, vec![EXIT]);
        let chamber = decoded.cells.iter().find(|((x, y), _)| *x == 7 && *y == 6).unwrap();
        assert_eq!(chamber.1.room, PORTAL_CHAMBER);
        assert_eq!(chamber.1.portals, [4, 0, 7]);
        assert_eq!(chamber.1.objects, vec![LEFT_PORTAL, RIGHT_PORTAL]);
        let kitchen = decoded.cells.iter().find(|((x, y), _)| *x == 6 && *y == 5).unwrap();
        assert_eq!(kitchen.1.objects, vec![2, FANCY_RANGE]);
    }

    #[test]
    fn save_writes_the_account_file_and_keeps_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("bash-house-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Configs").join("house")).unwrap();
        let paths = SimbaPaths::new(dir.clone());
        let mut dialog = Dialog::blank("Hero", 2);
        dialog.extra.insert("custom_flag".into(), json!(true));
        dialog.cells[6 * 13 + 6].room = 2;
        dialog.save(&paths).unwrap();
        let loaded = Dialog::load(&paths, "Hero", 2);
        assert_eq!(loaded.cells[6 * 13 + 6].room, 2);
        assert_eq!(loaded.extra.get("custom_flag"), Some(&json!(true)));
        assert!(dir.join("Configs").join("house").join("Hero.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_blank_house_is_a_garden_with_the_exit_portal() {
        let mut dialog = Dialog::blank("Hero", 0);
        dialog.ensure_default_garden();
        let center = dialog.amount / 2;
        let cell = &dialog.cells[center * dialog.amount + center];
        assert_eq!(cell.room, 2);
        assert_eq!(cell.rotation, 0);
        assert_eq!(cell.objects, vec![EXIT]);
        dialog.cells[0].room = 5;
        dialog.clear_layout();
        let cell = &dialog.cells[center * dialog.amount + center];
        assert_eq!(cell.room, 2);
        assert_eq!(cell.objects, vec![EXIT]);
        assert_eq!(dialog.filled(), 1);
        assert!(dialog.is_default_garden());
    }
}
