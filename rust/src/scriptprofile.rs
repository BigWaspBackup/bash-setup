//! Account, antiban, webhook, farm, and birdhouse settings.
//!
//! These are the same files the Simba form writes. The launcher can open any
//! page on its own, and the AIO Agility run setup includes all of them.

use std::fs;

use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea};

use crate::{
    agility::{self, Settings as CourseSettings},
    catalog::ScriptEntry,
    clients::ClientWindow,
    paths::SimbaPaths,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Course,
    Script,
    Gear,
    Accounts,
    Antiban,
    Webhooks,
    Farm,
    Birdhouse,
}

pub enum EditorEvent {
    None,
    Close,
    /// Settings were saved, then the window should close. A script is not started.
    CloseSaved,
    Start,
}

#[derive(Clone)]
pub struct Account {
    pub name: String,
    pub pin: String,
    pub worlds: String,
}

#[derive(Clone, PartialEq)]
struct BreakSlot {
    enabled: bool,
    interval: String,
    length: String,
    random: String,
    logout: String,
}

#[derive(Clone, PartialEq)]
struct TaskSlot {
    enabled: bool,
    interval: String,
    random: String,
}

#[derive(Clone, PartialEq)]
struct Antiban {
    breaks_enabled: bool,
    breaks: Vec<BreakSlot>,
    sleep_enabled: bool,
    sleep_hour: String,
    sleep_length: String,
    sleep_random: String,
    tasks: Vec<TaskSlot>,
    hop_enabled: bool,
    hop_interval: String,
    hop_variation: String,
    re_mode: String,
    re_skill: String,
    re_genie: bool,
    re_genie_action: String,
    re_count: bool,
    re_count_action: String,
}

#[derive(Clone)]
struct Webhook {
    enabled: bool,
    url: String,
    test_note: String,
}

#[derive(Clone)]
struct Farm {
    herb: usize,
    flower: usize,
    allotment: usize,
    bush: usize,
    cactus: usize,
    anima: usize,
    tree: usize,
    fruit: usize,
    compost: usize,
    patches: Vec<bool>,
    travel: Vec<usize>,
    barb_dibber: bool,
    magic_secateurs: bool,
    lumby_elite: bool,
    run_now: bool,
}

#[derive(Clone)]
struct Birdhouse {
    log: usize,
    seed: usize,
    best_log: bool,
    run_now: bool,
}

pub struct Editor {
    pub page: Page,
    pub run_script: Option<ScriptEntry>,
    pub account_label: String,
    accounts: Vec<Account>,
    selected: usize,
    course: Option<CourseSettings>,
    pub(crate) script: Option<std::collections::HashMap<String, String>>,
    /// Tormented Demons gear. Empty for every other script.
    gear: Option<crate::gear::Editor>,
    pub(crate) client_hwnd: Option<isize>,
    antiban: Antiban,
    /// Account name the unsaved break and antiban numbers were built from.
    /// Empty once a saved profile is loaded, or the numbers are edited.
    antiban_hashed_for: Option<String>,
    webhook: Webhook,
    farm: Farm,
    bird: Birdhouse,
    show_pin: bool,
    show_webhook: bool,
    worlds_picker: crate::worlds::Picker,
    /// RuneLite account already applied to the list, so a later login can replace it.
    seen_login: String,
    pub error: String,
}

const TASKS: [&str; 8] = [
    "Camera zoom",
    "Camera angle",
    "Mouse",
    "Lose focus",
    "Chat",
    "Game tabs",
    "Hover skills",
    "Bank",
];
/// Camera zoom, camera angle, and mouse stay off. The indexes match the script's task list.
const VISIBLE_TASKS: [usize; 5] = [3, 4, 5, 6, 7];
const TASK_MINUTES: [i32; 8] = [3, 7, 8, 10, 9, 12, 18, 13];
const BREAK_INTERVALS: [i32; 4] = [30, 45, 60, 120];
const BREAK_LENGTHS: [i32; 4] = [2, 5, 10, 30];
const BREAK_LOGOUTS: [i32; 4] = [1, 15, 20, 50];
/// Keys match `ERSLampSkill.ToKey`. Thieving is stored as `theiving`.
const LAMP_SKILLS: &[(&str, &str)] = &[
    ("agility", "Agility"),
    ("attack", "Attack"),
    ("construction", "Construction"),
    ("cooking", "Cooking"),
    ("crafting", "Crafting"),
    ("defence", "Defence"),
    ("farming", "Farming"),
    ("firemaking", "Firemaking"),
    ("fishing", "Fishing"),
    ("fletching", "Fletching"),
    ("herblore", "Herblore"),
    ("hitpoints", "Hitpoints"),
    ("hunter", "Hunter"),
    ("magic", "Magic"),
    ("mining", "Mining"),
    ("prayer", "Prayer"),
    ("range", "Range"),
    ("runecrafting", "Runecrafting"),
    ("sailing", "Sailing"),
    ("slayer", "Slayer"),
    ("smithing", "Smithing"),
    ("strength", "Strength"),
    ("theiving", "Thieving"),
    ("woodcutting", "Woodcutting"),
];

const HERBS: [&str; 16] = [
    "None", "Guam seed", "Marrentill seed", "Tarromin seed", "Harralander seed", "Ranarr seed", "Toadflax seed",
    "Irit seed", "Avantoe seed", "Kwuarm seed", "Snapdragon seed", "Cadantine seed", "Lantadyme seed",
    "Dwarf weed seed", "Torstol seed", "Huasca seed",
];
const FLOWERS: [&str; 7] = [
    "None", "Marigold seed", "Rosemary seed", "Nasturtium seed", "Woad seed", "Limpwurt seed", "White lily seed",
];
const ALLOTS: [&str; 9] = [
    "None", "Potato seed", "Onion seed", "Cabbage seed", "Tomato seed", "Sweetcorn seed", "Strawberry seed",
    "Watermelon seed", "Snape grass seed",
];
const BUSHES: [&str; 7] = [
    "None", "Redberry seed", "Cadavaberry seed", "Dwellberry seed", "Jangerberry seed", "Whiteberry seed",
    "Poison ivy seed",
];
const CACTI: [&str; 3] = ["None", "Cactus seed", "Potato cactus seed"];
const ANIMAS: [&str; 4] = ["None", "Attas seed", "Iasor seed", "Kronos seed"];
const TREES: [&str; 7] = [
    "None", "Oak sapling", "Willow sapling", "Maple sapling", "Yew sapling", "Magic sapling", "Redwood sapling",
];
const FRUITS: [&str; 9] = [
    "None", "Apple sapling", "Banana sapling", "Orange sapling", "Curry sapling", "Pineapple sapling",
    "Papaya tree sapling", "Palm tree sapling", "Dragonfruit sapling",
];
const COMPOSTS: [&str; 5] = [
    "Ultracompost", "Supercompost", "Compost", "Bottomless compost bucket", "Fertile Soil",
];
const PATCHES: &[(&str, &str)] = &[
    ("Herbs", "Weiss herb"),
    ("Herbs", "Stronghold herb"),
    ("Herbs", "Ardougne herb"),
    ("Herbs", "Falador herb"),
    ("Herbs", "Catherby herb"),
    ("Herbs", "Phasmatys herb"),
    ("Herbs", "Hosidius herb"),
    ("Herbs", "Ortus herb"),
    ("Herbs", "Guild herb"),
    ("Flowers", "Ardougne flower"),
    ("Flowers", "Falador flower"),
    ("Flowers", "Catherby flower"),
    ("Flowers", "Phasmatys flower"),
    ("Flowers", "Hosidius flower"),
    ("Flowers", "Ortus flower"),
    ("Flowers", "Guild flower"),
    ("Allotments", "Ardougne allot N"),
    ("Allotments", "Ardougne allot S"),
    ("Allotments", "Falador allot N"),
    ("Allotments", "Falador allot S"),
    ("Allotments", "Catherby allot N"),
    ("Allotments", "Catherby allot S"),
    ("Allotments", "Phasmatys allot N"),
    ("Allotments", "Phasmatys allot S"),
    ("Allotments", "Hosidius allot N"),
    ("Allotments", "Hosidius allot S"),
    ("Allotments", "Ortus allot N"),
    ("Allotments", "Ortus allot S"),
    ("Allotments", "Guild allot N"),
    ("Allotments", "Guild allot S"),
    ("Other", "Guild bush"),
    ("Other", "Guild cactus"),
    ("Other", "Guild anima"),
    ("Trees", "Lumbridge tree"),
    ("Trees", "Varrock tree"),
    ("Trees", "Falador tree"),
    ("Trees", "Taverley tree"),
    ("Trees", "Gnome tree"),
    ("Trees", "Guild tree"),
    ("Trees", "Guild redwood"),
    ("Fruit", "Gnome stronghold fruit"),
    ("Fruit", "Catherby fruit"),
    ("Fruit", "Gnome village fruit"),
    ("Fruit", "Brimhaven fruit"),
    ("Fruit", "Lletya fruit"),
    ("Fruit", "Guild fruit"),
];

const LOGS: [&str; 9] = [
    "Regular", "Oak", "Willow", "Teak", "Maple", "Mahogany", "Yew", "Magic", "Redwood",
];
const SEEDS: &[&str] = &[
    "Asgarnian seed", "Barley seed", "Cabbage seed", "Hammerstone seed", "Jute seed", "Krandorian seed",
    "Marigold seed", "Onion seed", "Potato seed", "Rosemary seed", "Strawberry seed", "Sweetcorn seed",
    "Tomato seed", "Watermelon seed", "Wildblood seed", "Yanillian seed",
];

fn travels() -> Vec<(&'static str, &'static str, &'static [&'static str])> {
    vec![
        ("Weiss", "Weiss TP", &["Icy basalt"]),
        ("Stronghold", "Stronghold TP", &["Stony basalt"]),
        ("Ardougne", "Ardougne TP", &["Ardougne Cloak (Farm)", "Ardougne Spell", "Ardougne Tablet"]),
        ("Falador", "Falador TP", &["Explorers Ring (Farm)", "Falador Spell", "Falador Tablet"]),
        ("Catherby", "Catherby TP", &["Lunar Catherby", "Camelot Spell", "Camelot Tablet", "Catherby Tablet"]),
        ("Phasmatys", "Phasmatys TP", &["Ectophial"]),
        (
            "Hosidius",
            "Hosidius TP",
            &["Kharedst Lunch (Hosidius)", "Hosidius Tablet", "House Spell", "House Tablet", "Xeric's Talisman"],
        ),
        (
            "Ortus",
            "Ortus TP",
            &["Hunter Guild (Hunters Cape)", "Quetzal Whistle", "Civitas Tablet", "Civitas Teleport"],
        ),
        ("Guild", "Guild TP", &["Skills Necklace", "Farming Cape", "Fairy CIR (Ardy Cloak)", "Farming Cape (t)"]),
        ("Lumbridge tree", "Lumbridge tree TP", &["Lumbridge Spell"]),
        ("Varrock tree", "Varrock tree TP", &["Varrock Spell"]),
        (
            "Falador tree",
            "Falador tree TP",
            &["Ring of Wealth (Falador Park)", "Falador Spell", "Falador Tablet"],
        ),
        ("Taverley tree", "Taverley tree TP", &["Taverley Tablet"]),
        ("Gnome tree", "Gnome tree TP", &["Fairy CIQ (Yanille)", "Slayer Ring (Stronghold Cave)"]),
        ("Gnome fruit", "Gnome fruit TP", &["Fairy CIQ (Yanille)", "Slayer Ring (Stronghold Cave)"]),
        ("Gnome village fruit", "Gnome village fruit TP", &["Fairy CIQ (Yanille)"]),
        ("Brimhaven fruit", "Brimhaven fruit TP", &["Brimhaven Tablet", "Fairy CKR (Tai Bwo Wannai)"]),
        ("Lletya fruit", "Lletya fruit TP", &["Teleport Crystal"]),
        (
            "Catherby fruit",
            "Catherby fruit TP",
            &["Lunar Catherby", "Camelot Spell", "Camelot Tablet", "Catherby Tablet"],
        ),
    ]
}

impl Editor {
    pub fn for_run(paths: &SimbaPaths, script: &ScriptEntry, client: &ClientWindow) -> Self {
        let label = client.account();
        let key = agility::account_key(&label);
        let mut editor = Self::load(paths, &key, &label);
        editor.run_script = Some(script.clone());
        if crate::agility::is_aio_agility(script) {
            editor.page = Page::Course;
            editor.course = Some(CourseSettings::load_for(paths, &key));
        } else if crate::scriptsetup::supports(&script.name) {
            editor.page = Page::Script;
            editor.script = Some(crate::scriptsetup::load(&paths.configs(), &script.name, &key));
        }
        editor.client_hwnd = Some(client.hwnd);
        if script.name.eq_ignore_ascii_case("Tormented Demons") {
            editor.gear = Some(crate::gear::Editor::load(paths));
            editor.page = Page::Gear;
        }
        editor
    }

    pub fn for_page(paths: &SimbaPaths, client: Option<&ClientWindow>, page: Page) -> Self {
        let label = client.map(|client| client.account()).unwrap_or_else(|| "Not logged in".into());
        let key = agility::account_key(&label);
        let mut editor = Self::load(paths, &key, &label);
        editor.page = page;
        editor
    }

    fn load(paths: &SimbaPaths, account_key: &str, label: &str) -> Self {
        let mut accounts = read_accounts(&paths.root.join("credentials.simba"));
        let mut selected = accounts.iter().position(|account| account.name.eq_ignore_ascii_case(account_key));
        if selected.is_none() && account_key != "NoUserNameSelected" {
            accounts.push(Account {
                name: account_key.to_string(),
                pin: String::new(),
                worlds: String::new(),
            });
            selected = Some(accounts.len() - 1);
        }
        let selected = selected.unwrap_or(0);
        let index = if accounts.is_empty() { 0 } else { selected };
        let (antiban, antiban_hashed_for) = Antiban::load(&ini_text(&paths.configs().join("BASettings.ini")), account_key);
        Self {
            page: Page::Accounts,
            run_script: None,
            account_label: label.to_string(),
            antiban,
            antiban_hashed_for,
            webhook: Webhook::load(&ini_text(&paths.configs().join("BASettings.ini")), account_key),
            farm: Farm::load(&ini_text(&paths.configs().join("BASHFarmSettings")), index),
            bird: Birdhouse::load(&ini_text(&paths.configs().join("BASHBirdHouseSettings")), index),
            accounts,
            selected,
            course: None,
            script: None,
            gear: None,
            client_hwnd: None,
            show_pin: false,
            show_webhook: false,
            worlds_picker: crate::worlds::Picker::default(),
            seen_login: if account_key == "NoUserNameSelected" {
                String::new()
            } else {
                account_key.to_string()
            },
            error: String::new(),
        }
    }

    /// Select or fill the account that is logged in on RuneLite, then load that account's settings.
    pub fn note_logged_in(&mut self, paths: &SimbaPaths, label: &str) {
        let key = agility::account_key(label);
        if key == "NoUserNameSelected" || self.seen_login == key {
            return;
        }
        self.seen_login = key.clone();
        self.account_label = label.to_string();
        if let Some(index) = self.accounts.iter().position(|account| account.name.eq_ignore_ascii_case(&key)) {
            if index != self.selected {
                self.reload_account(paths, index);
            }
            return;
        }
        if let Some(account) = self.accounts.get_mut(self.selected) {
            let current = account.name.trim();
            if current.is_empty() || current.eq_ignore_ascii_case("New account") {
                account.name = key;
                let index = self.selected;
                self.reload_account(paths, index);
                return;
            }
        }
        self.accounts.push(Account {
            name: key,
            pin: String::new(),
            worlds: String::new(),
        });
        let index = self.accounts.len() - 1;
        self.reload_account(paths, index);
    }

    pub fn validate_course(&self) -> Result<(), String> {
        if let Some(course) = &self.course {
            course.validate()?;
        }
        if let (Some(script), Some(values)) = (&self.run_script, &self.script) {
            crate::scriptsetup::validate(&script.name, values)?;
        }
        if let Some(gear) = &self.gear {
            gear.validate()?;
        }
        Ok(())
    }

    /// Farm and Birdhouse tabs only show when this run supports them and the matching checkbox is on.
    /// Opening Accounts from the header keeps both tabs.
    fn side_run_tabs(&self) -> (bool, bool) {
        let Some(script) = &self.run_script else {
            return (true, true);
        };
        let catalog_farm = script.features.iter().any(|feature| feature == "farm_runs");
        let catalog_bird = script.features.iter().any(|feature| feature == "birdhouses");
        if let Some(course) = &self.course {
            return (catalog_farm && course.farm, catalog_bird && course.birdhouse);
        }
        if let Some(values) = &self.script {
            let (farm, bird) = crate::scriptsetup::side_runs_enabled(values);
            return (catalog_farm && farm, catalog_bird && bird);
        }
        (false, false)
    }

    fn selected_name(&self) -> String {
        self.accounts
            .get(self.selected)
            .map(|account| account.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "this account".into())
    }

    pub fn account_key(&self) -> String {
        self.accounts
            .get(self.selected)
            .map(|account| account.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "NoUserNameSelected".into())
    }

    pub fn save(&mut self, paths: &SimbaPaths) -> Result<(), String> {
        if let Some(gear) = &mut self.gear {
            gear.persist(paths)?;
        }
        let key = self.account_key();
        if let Some(course) = &self.course {
            course.save(&paths.configs().join("BASettings.ini"), &key)?;
        }
        if let (Some(script), Some(values)) = (&self.run_script, &self.script) {
            crate::scriptsetup::save(&paths.configs(), &script.name, &key, values)?;
        }
        write_accounts(&paths.root.join("credentials.simba"), &self.accounts)?;
        self.antiban.save(&paths.configs().join("BASettings.ini"), &key)?;
        self.webhook.save(&paths.configs().join("BASettings.ini"), &key)?;
        let index = self.selected;
        self.farm.save(&paths.configs().join("BASHFarmSettings"), index)?;
        self.bird.save(&paths.configs().join("BASHBirdHouseSettings"), index)?;
        Ok(())
    }

    fn reload_account(&mut self, paths: &SimbaPaths, index: usize) {
        let Some(account) = self.accounts.get(index) else {
            return;
        };
        let key = account.name.clone();
        let (antiban, hashed_for) = Antiban::load(&ini_text(&paths.configs().join("BASettings.ini")), &key);
        self.antiban = antiban;
        self.antiban_hashed_for = hashed_for;
        self.webhook = Webhook::load(&ini_text(&paths.configs().join("BASettings.ini")), &key);
        self.farm = Farm::load(&ini_text(&paths.configs().join("BASHFarmSettings")), index);
        self.bird = Birdhouse::load(&ini_text(&paths.configs().join("BASHBirdHouseSettings")), index);
        if self.course.is_some() {
            self.course = Some(CourseSettings::load_for(paths, &key));
        }
        if self.script.is_some() {
            let name = self.run_script.as_ref().map(|script| script.name.clone());
            if let Some(name) = name {
                self.script = Some(crate::scriptsetup::load(&paths.configs(), &name, &key));
            }
        }
        if self.gear.is_some() {
            self.gear = Some(crate::gear::Editor::load(paths));
        }
        self.selected = index;
    }

    /// Keep a new account's break and antiban numbers on that account's biohash
    /// until a saved profile is loaded or the numbers are edited.
    fn follow_account_name(&mut self) {
        let Some(previous) = self.antiban_hashed_for.clone() else {
            return;
        };
        let name = self
            .accounts
            .get(self.selected)
            .map(|account| account.name.trim().to_string())
            .unwrap_or_default();
        if name == previous {
            return;
        }
        if self.antiban != Antiban::automatic(&previous) {
            self.antiban_hashed_for = None;
            return;
        }
        self.antiban = Antiban::automatic(&name);
        self.antiban_hashed_for = Some(name);
    }
}

pub fn show(ctx: &egui::Context, paths: &SimbaPaths, editor: &mut Editor) -> EditorEvent {
    if let Some(gear) = editor.gear.as_mut() {
        gear.poll(ctx);
    }
    let mut event = EditorEvent::None;
    let mut switch_to: Option<usize> = None;
    let mut close_requested = false;
    let running = editor.run_script.is_some();
    let title = if running {
        editor
            .run_script
            .as_ref()
            .map(|script| script.name.clone())
            .unwrap_or_else(|| "Script".into())
    } else {
        "Accounts".into()
    };
    let mut open = true;
    egui::Window::new(title)
        .id(egui::Id::new("bash-settings"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .fade_in(false)
        .fade_out(false)
        .default_size([SETTINGS_WIDTH, SETTINGS_MIN_HEIGHT])
        .min_width(SETTINGS_WIDTH)
        .max_width(SETTINGS_WIDTH)
        .min_height(SETTINGS_MIN_HEIGHT)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(SETTINGS_WIDTH, SETTINGS_MIN_HEIGHT));
            page_tabs(ui, editor);
            ui.separator();
            ScrollArea::vertical()
                .id_salt(("bash-settings-body", editor.page))
                .max_height(SETTINGS_BODY)
                .min_scrolled_height(SETTINGS_BODY)
                .max_width(SETTINGS_WIDTH - 16.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(SETTINGS_WIDTH - 16.0, SETTINGS_BODY));
                    match editor.page {
                        Page::Course => {
                            if let Some(course) = editor.course.as_mut() {
                                agility::course_ui(ui, course);
                            }
                        }
                        Page::Script => {
                            if let Some(script) = editor.run_script.clone() {
                                if let Some(values) = editor.script.as_mut() {
                                    crate::scriptsetup::show(ui, &script.name, values);
                                }
                            }
                        }
                        Page::Gear => {
                            let hwnd = editor.client_hwnd;
                            let account = editor.account_key();
                            if let Some(gear) = editor.gear.as_mut() {
                                gear.ui(ui, paths, hwnd, &account);
                            }
                        }
                        Page::Accounts => accounts_page(ui, editor, &mut switch_to),
                        Page::Antiban => {
                            let label = editor.selected_name();
                            let hash_name = editor
                                .accounts
                                .get(editor.selected)
                                .map(|account| account.name.trim().to_string())
                                .unwrap_or_default();
                            let mut refreshed = false;
                            antiban_page(ui, &mut editor.antiban, &label, &hash_name, &mut refreshed);
                            if refreshed {
                                editor.antiban_hashed_for = Some(hash_name);
                            }
                        }
                        Page::Webhooks => {
                            let account = editor.selected_name();
                            webhook_page(ui, &mut editor.webhook, &mut editor.show_webhook, &account);
                        }
                        Page::Farm => {
                            let account = editor.selected_name();
                            farm_page(ui, &mut editor.farm, &account);
                        }
                        Page::Birdhouse => {
                            let account = editor.selected_name();
                            bird_page(ui, &mut editor.bird, &account);
                        }
                    }
                });
            if editor.error.is_empty() {
                ui.add_space(18.0);
            } else {
                ui.label(RichText::new(&editor.error).color(egui::Color32::from_rgb(220, 120, 110)));
            }
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(if running { "Cancel" } else { "Close" }).clicked() {
                        close_requested = true;
                    }
                    if running {
                        let ready = editor.account_key() != "NoUserNameSelected";
                        if ui.add_enabled(ready, Button::new("Start")).clicked() {
                            event = EditorEvent::Start;
                        }
                    } else if ui.button("Save").clicked() {
                        event = EditorEvent::Start;
                    }
                });
            });
        });
    editor.follow_account_name();
    if !open || close_requested {
        event = confirm_close(editor, paths);
    }
    if let Some(index) = switch_to {
        if let Err(error) = editor.save(paths) {
            editor.error = error;
        } else {
            editor.reload_account(paths, index);
        }
    }
    if let Some(worlds) = crate::worlds::show(ctx, &mut editor.worlds_picker) {
        let index = editor.worlds_picker.target;
        if let Some(account) = editor.accounts.get_mut(index) {
            account.worlds = worlds;
        }
    }
    event
}

fn page_tabs(ui: &mut egui::Ui, editor: &mut Editor) {
    let mut pages = Vec::new();
    if editor.course.is_some() {
        pages.push((Page::Course, "Course"));
    } else if editor.script.is_some() {
        pages.push((Page::Script, "Script"));
    }
    if editor.gear.is_some() {
        pages.push((Page::Gear, "Gear"));
    }
    pages.extend([
        (Page::Accounts, "Accounts"),
        (Page::Antiban, "Antiban"),
        (Page::Webhooks, "Webhooks"),
    ]);
    let (show_farm, show_bird) = editor.side_run_tabs();
    if show_farm {
        pages.push((Page::Farm, "Farm"));
    }
    if show_bird {
        pages.push((Page::Birdhouse, "Birdhouse"));
    }
    if matches!(editor.page, Page::Farm) && !show_farm {
        editor.page = Page::Accounts;
    }
    if matches!(editor.page, Page::Birdhouse) && !show_bird {
        editor.page = Page::Accounts;
    }
    ui.horizontal(|ui| {
        let width = ((SETTINGS_WIDTH - 16.0) / pages.len() as f32) - 4.0;
        for (page, label) in pages {
            if ui.add_sized([width, 30.0], egui::SelectableLabel::new(editor.page == page, label)).clicked() {
                editor.page = page;
            }
        }
    });
}

fn confirm_close(editor: &mut Editor, paths: &SimbaPaths) -> EditorEvent {
    match crate::ui::ask_save_changes(
        "Do you want to save your changes before closing?\n\nYes saves and closes. No closes without saving. Cancel goes back.",
    ) {
        crate::ui::SaveChoice::Cancel => EditorEvent::None,
        crate::ui::SaveChoice::No => EditorEvent::Close,
        crate::ui::SaveChoice::Yes => match editor.save(paths) {
            Ok(()) => EditorEvent::CloseSaved,
            Err(error) => {
                editor.error = error;
                EditorEvent::None
            }
        },
    }
}

fn accounts_page(ui: &mut egui::Ui, editor: &mut Editor, switch_to: &mut Option<usize>) {
    ui.label("Pick an account on the left. Name, bank pin, and worlds are shared by every script.");
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(230.0);
            ui.label(RichText::new("Accounts").strong());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                if editor.accounts.is_empty() {
                    ui.label("None yet.");
                } else {
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for index in 0..editor.accounts.len() {
                            let name = editor.accounts[index].name.trim();
                            let name = if name.is_empty() { "Unnamed" } else { name };
                            if ui.selectable_label(index == editor.selected, name).clicked() && index != editor.selected {
                                *switch_to = Some(index);
                            }
                        }
                    });
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Add").clicked() {
                    editor.accounts.push(Account {
                        name: "New account".into(),
                        pin: String::new(),
                        worlds: String::new(),
                    });
                    *switch_to = Some(editor.accounts.len() - 1);
                }
                if ui.add_enabled(!editor.accounts.is_empty(), Button::new("Delete")).clicked() {
                    let name = editor.selected_name();
                    if crate::ui::ask_yes_no(&format!("Delete {name}?\n\nThe name, bank pin, and worlds are removed.")) {
                        editor.accounts.remove(editor.selected);
                        editor.selected = editor.selected.min(editor.accounts.len().saturating_sub(1));
                    }
                }
            });
        });
        ui.add_space(18.0);
        ui.vertical(|ui| {
            ui.set_min_width(620.0);
            if editor.accounts.is_empty() {
                ui.label("Add an account to set the name, bank pin, and worlds.");
                return;
            }
            ui.label(RichText::new("Details").strong());
            ui.add_space(4.0);
            let mut show_pin = editor.show_pin;
            let mut open_worlds = false;
            {
                let account = &mut editor.accounts[editor.selected];
                egui::Grid::new("account-identity").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Account name");
                    ui.add(egui::TextEdit::singleline(&mut account.name).desired_width(280.0));
                    ui.end_row();
                    ui.label("Bank pin");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut account.pin).desired_width(120.0).password(!show_pin));
                        ui.checkbox(&mut show_pin, "Show");
                    });
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.label("Worlds");
                let summary = if account.worlds.trim().is_empty() {
                    "None yet. A script will not start until this account has worlds.".to_string()
                } else {
                    account.worlds.clone()
                };
                ui.add(egui::Label::new(summary).wrap());
                if ui.button("Generate worlds").clicked() {
                    open_worlds = true;
                }
            }
            editor.show_pin = show_pin;
            if open_worlds {
                editor.worlds_picker.target = editor.selected;
                let current = editor.accounts[editor.selected].worlds.clone();
                editor.worlds_picker.open_with(&current);
            }
        });
    });
}

fn tip_check(ui: &mut egui::Ui, value: &mut bool, label: &str, tip: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(value, label);
        crate::ui::help_tip(ui, tip);
    });
}

fn tip_heading(ui: &mut egui::Ui, label: &str, tip: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        crate::ui::help_tip(ui, tip);
    });
}

fn antiban_page(ui: &mut egui::Ui, antiban: &mut Antiban, label: &str, account: &str, refreshed: &mut bool) {
    for_account(ui, label);
    ui.horizontal(|ui| {
        if ui.button("Randomise").on_hover_text("Picks new break, sleep, task, and world hop numbers.").clicked() {
            antiban.randomise();
        }
        if ui.button("BioHash/Default").on_hover_text("Uses this account name, so the numbers stay the same every time.").clicked() {
            antiban.apply_biohash(account);
            *refreshed = true;
        }
    });
    ui.add_space(8.0);
    tip_check(
        ui,
        &mut antiban.breaks_enabled,
        "Breaks",
        "Take short breaks while the script runs. Every is minutes between breaks. Length is how long each break lasts. Random % varies the timing. Logout % is the chance to logout for that break.",
    );
    let breaks_on = antiban.breaks_enabled;
    ui.add_enabled_ui(breaks_on, |ui| {
        ui.label(RichText::new("Times are in minutes.").weak());
        egui::Grid::new("antiban-breaks").num_columns(5).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Every").strong());
            ui.label(RichText::new("Length").strong());
            ui.label(RichText::new("Random %").strong());
            ui.label(RichText::new("Logout %").strong());
            ui.end_row();
            for (index, slot) in antiban.breaks.iter_mut().enumerate() {
                ui.checkbox(&mut slot.enabled, format!("Break {}", index + 1));
                let row_on = slot.enabled;
                number_box(ui, &mut slot.interval, row_on);
                number_box(ui, &mut slot.length, row_on);
                number_box(ui, &mut slot.random, row_on);
                number_box(ui, &mut slot.logout, row_on);
                ui.end_row();
            }
        });
    });
    ui.add_space(10.0);
    tip_check(
        ui,
        &mut antiban.sleep_enabled,
        "Sleep",
        "Logout overnight at this hour for a longer rest. Length is hours asleep. Vary % moves the start time earlier or later.",
    );
    let sleep_on = antiban.sleep_enabled;
    ui.add_enabled_ui(sleep_on, |ui| {
        ui.horizontal(|ui| {
            ui.label("At");
            ui.add(egui::TextEdit::singleline(&mut antiban.sleep_hour).desired_width(90.0));
            ui.label("for");
            number_box(ui, &mut antiban.sleep_length, true);
            ui.label("hours, vary");
            number_box(ui, &mut antiban.sleep_random, true);
            ui.label("%");
        });
    });
    ui.add_space(10.0);
    tip_heading(
        ui,
        "Random actions",
        "Small actions between breaks. Every is minutes between that action. Vary % is how early or late it can fire.",
    );
    egui::Grid::new("antiban-tasks").num_columns(6).spacing([8.0, 4.0]).show(ui, |ui| {
        for index in VISIBLE_TASKS {
            let slot = &mut antiban.tasks[index];
            ui.checkbox(&mut slot.enabled, TASKS[index]);
            let row_on = slot.enabled;
            ui.add_enabled(row_on, egui::Label::new("every"));
            number_box(ui, &mut slot.interval, row_on);
            ui.add_enabled(row_on, egui::Label::new("minutes, vary"));
            number_box(ui, &mut slot.random, row_on);
            ui.add_enabled(row_on, egui::Label::new("%"));
            ui.end_row();
        }
    });
    ui.add_space(10.0);
    tip_check(
        ui,
        &mut antiban.hop_enabled,
        "Hop worlds",
        "Change worlds on this timer so the account is not stuck in one world for too long. Uses the worlds list on the Accounts tab.",
    );
    let hop_on = antiban.hop_enabled;
    ui.add_enabled_ui(hop_on, |ui| {
        ui.horizontal(|ui| {
            ui.label("Every");
            number_box(ui, &mut antiban.hop_interval, true);
            ui.label("minutes, vary");
            number_box(ui, &mut antiban.hop_variation, true);
            ui.label("%");
        });
    });
    ui.add_space(10.0);
    tip_heading(ui, "Random events", "What to do when a random event shows up while a script is running.");
    choice_combo(
        ui,
        "re-mode",
        "When one appears",
        "Leave it ignores the event. Solve it handles supported events and uses the lamp skill below when a lamp is offered.",
        &mut antiban.re_mode,
        &[("off", "Leave it"), ("interact", "Solve it")],
    );
    let lamp_on = antiban.re_mode == "interact";
    ui.add_enabled_ui(lamp_on, |ui| {
        choice_combo(
            ui,
            "re-skill",
            "Lamp skill",
            "Skill to put a genie or similar lamp on when Solve it is selected.",
            &mut antiban.re_skill,
            LAMP_SKILLS,
        );
    });
}

fn webhook_page(ui: &mut egui::Ui, webhook: &mut Webhook, show_url: &mut bool, account: &str) {
    for_account(ui, account);
    tip_check(
        ui,
        &mut webhook.enabled,
        "Send progress and session messages to Discord",
        "Posts progress and session messages to a Discord channel using a webhook URL.",
    );
    ui.add_space(6.0);
    if ui.link("How to create a Discord webhook").clicked() {
        crate::ui::open_url("https://support.discord.com/hc/en-us/articles/228383668-Intro-to-Webhooks");
    }
    ui.add_space(8.0);
    let send_on = webhook.enabled;
    ui.add_enabled_ui(send_on, |ui| {
        tip_heading(
            ui,
            "Webhook URL",
            "Paste the Discord webhook URL for the channel that should receive messages for this account.",
        );
        ui.horizontal(|ui| {
            ui.add_enabled(
                send_on,
                egui::TextEdit::singleline(&mut webhook.url).password(!*show_url).desired_width(460.0),
            );
            ui.checkbox(show_url, "Show");
            if ui.button("Test").on_hover_text("Sends \"Test message from BASH\" to this webhook.").clicked() {
                webhook.test_note = test_webhook(&webhook.url);
            }
        });
    });
    if !webhook.test_note.is_empty() {
        ui.add_space(6.0);
        ui.label(&webhook.test_note);
    }
}

fn test_webhook(url: &str) -> String {
    let url = url.trim();
    if url.is_empty() {
        return "Please enter a Webhook URL".into();
    }
    let discord = url.starts_with("https://discord.com/api/webhooks/")
        || url.starts_with("https://discordapp.com/api/webhooks/");
    if !discord {
        return "That does not look like a Discord webhook URL.".into();
    }
    let client = match reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(15)).build() {
        Ok(client) => client,
        Err(_) => return "Could not send the test message.".into(),
    };
    match client
        .post(url)
        .header("Content-Type", "application/json")
        .body(r#"{"content":"Test message from BASH"}"#)
        .send()
    {
        Ok(response) if response.status().is_success() => "Test message sent. Please check Discord.".into(),
        Ok(_) => "Discord rejected the webhook. Check the URL and try again.".into(),
        Err(_) => "Could not send the test message.".into(),
    }
}

const SETTINGS_WIDTH: f32 = 960.0;
const SETTINGS_BODY: f32 = 500.0;
/// Tabs, separators, the error line, and the buttons under the scrolling body.
const SETTINGS_MIN_HEIGHT: f32 = SETTINGS_BODY + 120.0;

fn farm_page(ui: &mut egui::Ui, farm: &mut Farm, account: &str) {
    farm.tree = 0;
    farm.fruit = 0;
    farm.cactus = 0;
    farm.anima = 0;
    clear_unset_patches(farm);
    for_account(ui, account);
    ui.label("Pick a seed, then tick the locations. Allotments is both patches.");
    ui.add_space(4.0);
    tip_check(
        ui,
        &mut farm.run_now,
        "Do a farm run as soon as the script starts",
        "Otherwise the first farm run waits about 90 minutes before it is due.",
    );
    ui.add_space(6.0);
    tip_heading(ui, "Seeds", "Choose what to plant. None skips that patch type. Locations only enable after a seed is chosen.");
    egui::Grid::new("farm-seeds").num_columns(3).spacing([18.0, 4.0]).show(ui, |ui| {
        seed_combo(ui, "Herb", &mut farm.herb, &HERBS);
        seed_combo(ui, "Flower", &mut farm.flower, &FLOWERS);
        seed_combo(ui, "Allotment", &mut farm.allotment, &ALLOTS);
        ui.end_row();
        seed_combo(ui, "Bush", &mut farm.bush, &BUSHES);
        seed_combo(ui, "Compost", &mut farm.compost, &COMPOSTS);
        ui.label("");
        ui.end_row();
    });
    ui.add_space(4.0);
    tip_heading(ui, "Tools", "Bring these if they are unlocked. They improve planting or skip needing a Dramen staff.");
    ui.horizontal(|ui| {
        tip_check(ui, &mut farm.barb_dibber, "Barbarian dibber", "Plants allotments without a seed dibber in the inventory.");
        tip_check(ui, &mut farm.magic_secateurs, "Magic secateurs", "Worn for better herb yields when harvesting.");
        tip_check(
            ui,
            &mut farm.lumby_elite,
            "Lumbridge elite, no Dramen staff",
            "Skip carrying a Dramen or Lunar staff for fairy rings when Lumbridge elite diary is done.",
        );
    });
    ui.add_space(10.0);
    ui.columns(2, |columns| {
        tip_heading(
            &mut columns[0],
            "Locations",
            "Tick the patches to visit. Travel picks how to reach that location when more than one method is available.",
        );
        location_grid(&mut columns[0], farm);
        columns[1].label(RichText::new("Trees and fruit trees").strong());
        columns[1].label("Coming soon");
    });
}

fn location_grid(ui: &mut egui::Ui, farm: &mut Farm) {
    let herb_on = farm.herb != 0;
    let flower_on = farm.flower != 0;
    let allot_on = farm.allotment != 0;
    let bush_on = farm.bush != 0;
    egui::Grid::new("farm-locations")
        .num_columns(6)
        .spacing([10.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Herb").strong());
            ui.label(RichText::new("Flower").strong());
            ui.label(RichText::new("Allot").strong());
            ui.label(RichText::new("Bush").strong());
            ui.label(RichText::new("Travel").strong());
            ui.end_row();
            let rows: &[(&str, Option<usize>, Option<usize>, Option<(usize, usize)>, Option<usize>, usize)] = &[
                ("Weiss", Some(0), None, None, None, 0),
                ("Stronghold", Some(1), None, None, None, 1),
                ("Ardougne", Some(2), Some(9), Some((16, 17)), None, 2),
                ("Falador", Some(3), Some(10), Some((18, 19)), None, 3),
                ("Catherby", Some(4), Some(11), Some((20, 21)), None, 4),
                ("Phasmatys", Some(5), Some(12), Some((22, 23)), None, 5),
                ("Hosidius", Some(6), Some(13), Some((24, 25)), None, 6),
                ("Civitas", Some(7), Some(14), Some((26, 27)), None, 7),
                ("Guild", Some(8), Some(15), Some((28, 29)), Some(30), 8),
            ];
            for (name, herb, flower, allot, bush, travel) in rows {
                ui.label(*name);
                patch_check(ui, farm, *herb, "", herb_on);
                patch_check(ui, farm, *flower, "", flower_on);
                allot_check(ui, farm, *allot, allot_on);
                patch_check(ui, farm, *bush, "", bush_on);
                let on = patch_on(farm, *herb)
                    || patch_on(farm, *flower)
                    || patch_on(farm, *bush)
                    || allot.is_some_and(|(north, south)| patch_on(farm, Some(north)) || patch_on(farm, Some(south)));
                travel_pick(ui, farm, *travel, on);
                ui.end_row();
            }
        });
}

fn patch_on(farm: &Farm, index: Option<usize>) -> bool {
    index.is_some_and(|index| farm.patches.get(index).copied().unwrap_or(false))
}

fn clear_unset_patches(farm: &mut Farm) {
    let groups = [
        (farm.herb, 0, 9),
        (farm.flower, 9, 16),
        (farm.allotment, 16, 30),
        (farm.bush, 30, 31),
        (farm.cactus, 31, 32),
        (farm.anima, 32, 33),
        (farm.tree, 33, 40),
        (farm.fruit, 40, 46),
    ];
    for (seed, start, end) in groups {
        if seed != 0 {
            continue;
        }
        for slot in farm.patches.iter_mut().take(end).skip(start) {
            *slot = false;
        }
    }
}

fn patch_check(ui: &mut egui::Ui, farm: &mut Farm, index: Option<usize>, label: &str, enabled: bool) {
    let Some(index) = index else {
        ui.label("");
        return;
    };
    let mut on = enabled && farm.patches.get(index).copied().unwrap_or(false);
    ui.add_enabled_ui(enabled, |ui| {
        if ui.checkbox(&mut on, label).changed() {
            if let Some(slot) = farm.patches.get_mut(index) {
                *slot = on;
            }
        }
    });
}

fn allot_check(ui: &mut egui::Ui, farm: &mut Farm, pair: Option<(usize, usize)>, enabled: bool) {
    let Some((north, south)) = pair else {
        ui.label("");
        return;
    };
    let mut on = enabled && (patch_on(farm, Some(north)) || patch_on(farm, Some(south)));
    ui.add_enabled_ui(enabled, |ui| {
        if ui.checkbox(&mut on, "").changed() {
            if let Some(slot) = farm.patches.get_mut(north) {
                *slot = on;
            }
            if let Some(slot) = farm.patches.get_mut(south) {
                *slot = on;
            }
        }
    });
}

fn travel_pick(ui: &mut egui::Ui, farm: &mut Farm, index: usize, enabled: bool) {
    let choices = travels()[index].2;
    if choices.len() <= 1 {
        let text = choices.first().copied().unwrap_or("");
        let shown = if enabled { RichText::new(text) } else { RichText::new(text).weak() };
        ui.label(shown);
        return;
    }
    let selected = choices.get(farm.travel[index]).copied().unwrap_or(choices[0]);
    ui.add_enabled_ui(enabled, |ui| {
        ComboBox::from_id_salt(("farm-travel", index))
            .width(168.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (choice_index, choice) in choices.iter().enumerate() {
                    ui.selectable_value(&mut farm.travel[index], choice_index, *choice);
                }
            });
    });
}

fn bird_page(ui: &mut egui::Ui, bird: &mut Birdhouse, account: &str) {
    for_account(ui, account);
    tip_heading(ui, "Logs", "Logs used to build the birdhouses. Keep them in the bank.");
    seed_combo(ui, "Log", &mut bird.log, &LOGS);
    tip_check(
        ui,
        &mut bird.best_log,
        "Use the best log already in the bank",
        "Ignores the log choice above and uses the highest birdhouse log already in the bank.",
    );
    ui.add_space(8.0);
    tip_heading(ui, "Seed", "Seeds placed in the birdhouses. Keep them in the bank.");
    seed_combo(ui, "Seed", &mut bird.seed, &SEEDS);
    ui.add_space(8.0);
    tip_check(
        ui,
        &mut bird.run_now,
        "Do a birdhouse run as soon as the script starts",
        "Otherwise the first birdhouse run waits until the usual birdhouse timer is due.",
    );
}

fn for_account(ui: &mut egui::Ui, account: &str) {
    ui.label(RichText::new(format!("Saved for {account}")).weak());
    ui.add_space(6.0);
}

fn number_box(ui: &mut egui::Ui, value: &mut String, enabled: bool) {
    ui.add_enabled(enabled, egui::TextEdit::singleline(value).desired_width(52.0));
}

fn choice_combo(ui: &mut egui::Ui, id: &str, label: &str, tip: &str, value: &mut String, choices: &[(&str, &str)]) {
    let shown = choices
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, text)| (*text).to_string())
        .unwrap_or_else(|| value.clone());
    ui.horizontal(|ui| {
        ui.add_sized([140.0, 18.0], egui::Label::new(label));
        crate::ui::help_tip(ui, tip);
        let menu_height = (choices.len() as f32 * 30.0 + 16.0).clamp(80.0, 760.0);
        ComboBox::from_id_salt(id).width(180.0).height(menu_height).selected_text(shown).show_ui(ui, |ui| {
            for (key, text) in choices {
                ui.selectable_value(value, (*key).to_string(), *text);
            }
        });
    });
}

fn seed_combo(ui: &mut egui::Ui, label: &str, index: &mut usize, choices: &[&str]) {
    let selected = choices.get(*index).copied().unwrap_or(choices[0]);
    ui.horizontal(|ui| {
        ui.label(label);
        ComboBox::from_id_salt(("seed", label))
            .width(168.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (choice_index, choice) in choices.iter().enumerate() {
                    ui.selectable_value(index, choice_index, *choice);
                }
            });
    });
}

fn ini_text(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn section_body(text: &str, section: &str) -> String {
    agility::ini_section(text, section)
}

impl Default for Antiban {
    fn default() -> Self {
        Self {
            breaks_enabled: true,
            breaks: (0..4)
                .map(|i| BreakSlot {
                    enabled: true,
                    interval: BREAK_INTERVALS[i].to_string(),
                    length: BREAK_LENGTHS[i].to_string(),
                    random: "33".into(),
                    logout: BREAK_LOGOUTS[i].to_string(),
                })
                .collect(),
            sleep_enabled: true,
            sleep_hour: "23:00:00".into(),
            sleep_length: "8".into(),
            sleep_random: "10".into(),
            tasks: (0..8)
                .map(|i| TaskSlot {
                    enabled: true,
                    interval: TASK_MINUTES[i].to_string(),
                    random: "33".into(),
                })
                .collect(),
            hop_enabled: true,
            hop_interval: "45".into(),
            hop_variation: "10".into(),
            re_mode: "off".into(),
            re_skill: "agility".into(),
            re_genie: false,
            re_genie_action: "ignore".into(),
            re_count: false,
            re_count_action: "ignore".into(),
        }
    }
}

impl Antiban {
    /// Numbers for an account that has no saved antiban profile yet.
    fn automatic(account: &str) -> Self {
        let mut antiban = Self::default();
        if account_can_biohash(account) {
            antiban.apply_biohash(account.trim());
        }
        antiban
    }

    fn load(text: &str, account: &str) -> (Self, Option<String>) {
        let body = section_body(text, &format!("{account} BASH Antiban"));
        if agility::ini_get(&body, "ProfileVersion").is_none() {
            return (Self::automatic(account), Some(account.trim().to_string()));
        }
        let mut antiban = Self::default();
        antiban.breaks_enabled = agility::ini_bool(&body, "BreaksEnabled", true);
        for (i, slot) in antiban.breaks.iter_mut().enumerate() {
            slot.enabled = agility::ini_bool(&body, &format!("Break{i}Enabled"), true);
            slot.interval = agility::ini_get(&body, &format!("Break{i}Interval")).unwrap_or(slot.interval.clone());
            slot.length = agility::ini_get(&body, &format!("Break{i}Length")).unwrap_or(slot.length.clone());
            slot.random = agility::ini_get(&body, &format!("Break{i}Random")).unwrap_or(slot.random.clone());
            slot.logout = agility::ini_get(&body, &format!("Break{i}Logout")).unwrap_or(slot.logout.clone());
        }
        antiban.sleep_enabled = agility::ini_bool(&body, "SleepEnabled", true);
        antiban.sleep_hour = agility::ini_get(&body, "SleepHour").unwrap_or(antiban.sleep_hour);
        antiban.sleep_length = agility::ini_get(&body, "SleepLength").unwrap_or(antiban.sleep_length);
        antiban.sleep_random = agility::ini_get(&body, "SleepRandom").unwrap_or(antiban.sleep_random);
        for (i, slot) in antiban.tasks.iter_mut().enumerate() {
            slot.enabled = agility::ini_bool(&body, &format!("Task{i}Enabled"), true);
            slot.interval = agility::ini_get(&body, &format!("Task{i}Interval")).unwrap_or(slot.interval.clone());
            slot.random = agility::ini_get(&body, &format!("Task{i}Random")).unwrap_or(slot.random.clone());
        }
        antiban.hop_enabled = agility::ini_bool(&body, "EnableWorldHopping", false);
        antiban.hop_interval = agility::ini_get(&body, "WorldHopInterval").unwrap_or(antiban.hop_interval);
        antiban.hop_variation = agility::ini_get(&body, "WorldHopVariation").unwrap_or(antiban.hop_variation);
        antiban.re_mode = agility::ini_get(&body, "REMode").unwrap_or(antiban.re_mode);
        antiban.re_skill = agility::ini_get(&body, "RESkill").unwrap_or(antiban.re_skill);
        antiban.re_genie = agility::ini_bool(&body, "REGenie", false);
        antiban.re_genie_action = agility::ini_get(&body, "REGenieAction").unwrap_or(antiban.re_genie_action);
        antiban.re_count = agility::ini_bool(&body, "RECountCheck", false);
        antiban.re_count_action = agility::ini_get(&body, "RECountCheckAction").unwrap_or(antiban.re_count_action);
        let solve = antiban.re_mode == "interact"
            || antiban.re_genie
            || antiban.re_genie_action == "interact"
            || antiban.re_count
            || antiban.re_count_action == "interact";
        antiban.re_mode = if solve { "interact".into() } else { "off".into() };
        (antiban, None)
    }

    fn randomise(&mut self) {
        self.breaks_enabled = true;
        self.sleep_enabled = true;
        self.hop_enabled = true;
        self.hop_interval = (30 + roll(31)).to_string();
        self.hop_variation = (5 + roll(21)).to_string();
        for (index, slot) in self.breaks.iter_mut().enumerate() {
            slot.enabled = true;
            let interval_spread = (BREAK_INTERVALS[index] / 3).max(2);
            let length_spread = (BREAK_LENGTHS[index] / 3).max(1);
            slot.interval = clamp_i32(BREAK_INTERVALS[index] + roll_between(-interval_spread, interval_spread + 1), 1, 10000).to_string();
            slot.length = clamp_i32(BREAK_LENGTHS[index] + roll_between(-length_spread, length_spread + 1), 1, 10000).to_string();
            slot.random = (15 + roll(36)).to_string();
            slot.logout = clamp_i32(BREAK_LOGOUTS[index] + roll_between(-10, 11), 0, 100).to_string();
        }
        self.sleep_hour = format!("{:02}:{:02}:00", roll(24), roll(60));
        self.sleep_length = (6 + roll(4)).to_string();
        self.sleep_random = (5 + roll(26)).to_string();
        for (index, slot) in self.tasks.iter_mut().enumerate() {
            slot.enabled = true;
            let spread = (TASK_MINUTES[index] / 2).max(1);
            slot.interval = clamp_i32(TASK_MINUTES[index] + roll_between(-spread, spread + 1), 1, 10000).to_string();
            slot.random = (15 + roll(36)).to_string();
        }
    }

    fn apply_biohash(&mut self, account: &str) {
        self.breaks_enabled = true;
        self.sleep_enabled = true;
        for (index, slot) in self.breaks.iter_mut().enumerate() {
            slot.enabled = true;
            slot.interval = scaled(BREAK_INTERVALS[index], field_hash(account, &format!("break{index}interval"))).to_string();
            slot.length = scaled(BREAK_LENGTHS[index], field_hash(account, &format!("break{index}length"))).to_string();
            slot.random = clamp_i32((20.0 + 30.0 * field_hash(account, &format!("break{index}variation"))).round() as i32, 0, 100).to_string();
            let logout = BREAK_LOGOUTS[index] as f64 + field_hash(account, &format!("break{index}logout")) * 25.0;
            slot.logout = clamp_i32(logout.round() as i32, 0, 100).to_string();
        }
        let sleep_hash = field_hash(account, "sleephour");
        let minutes = (sleep_hash * 1439.0).round().clamp(0.0, 1439.0) as i32;
        self.sleep_hour = format!("{:02}:{:02}:00", minutes / 60, minutes % 60);
        let length = 6.0 + (3.0 * field_hash(account, "sleeplength") * 100.0).round() / 100.0;
        self.sleep_length = decimal_or(&length.to_string(), 8.0);
        self.sleep_random = clamp_i32((10.0 + 20.0 * field_hash(account, "sleepvariation")).round() as i32, 0, 100).to_string();
        for (index, slot) in self.tasks.iter_mut().enumerate() {
            slot.enabled = true;
            slot.interval = scaled(TASK_MINUTES[index], field_hash(account, &format!("task{index}interval"))).to_string();
            slot.random = clamp_i32((20.0 + 30.0 * field_hash(account, &format!("task{index}variation"))).round() as i32, 0, 100).to_string();
        }
        self.hop_enabled = true;
        self.hop_interval = scaled(45, field_hash(account, "hopinterval")).to_string();
        self.hop_variation = clamp_i32((5.0 + 20.0 * field_hash(account, "hopvariation")).round() as i32, 0, 100).to_string();
    }

    fn save(&self, path: &std::path::Path, account: &str) -> Result<(), String> {
        let mut pairs = vec![
            ("ProfileVersion".into(), "1".into()),
            ("BreaksEnabled".into(), bool_text(self.breaks_enabled)),
            ("SleepEnabled".into(), bool_text(self.sleep_enabled)),
            ("SleepHour".into(), self.sleep_hour.trim().to_string()),
            ("SleepLength".into(), decimal_or(&self.sleep_length, 8.0)),
            ("SleepRandom".into(), clamp_number(&self.sleep_random, 0, 100, 10)),
            ("TasksEnabled".into(), "true".into()),
            ("EnableWorldHopping".into(), bool_text(self.hop_enabled)),
            ("WorldHopInterval".into(), clamp_number(&self.hop_interval, 1, 10000, 45)),
            ("WorldHopVariation".into(), clamp_number(&self.hop_variation, 0, 100, 10)),
            ("OverrideBreaks".into(), "false".into()),
            ("REMode".into(), self.re_mode.clone()),
            ("RESkill".into(), self.re_skill.clone()),
            ("REWarning".into(), "true".into()),
            ("REGenie".into(), bool_text(self.re_mode == "interact")),
            ("REGenieAction".into(), if self.re_mode == "interact" { "interact".into() } else { "ignore".into() }),
            ("RECountCheck".into(), bool_text(self.re_mode == "interact")),
            ("RECountCheckAction".into(), if self.re_mode == "interact" { "interact".into() } else { "ignore".into() }),
        ];
        for (i, slot) in self.breaks.iter().enumerate() {
            pairs.push((format!("Break{i}Enabled"), bool_text(slot.enabled)));
            pairs.push((format!("Break{i}Interval"), clamp_number(&slot.interval, 1, 10000, BREAK_INTERVALS[i])));
            pairs.push((format!("Break{i}Length"), clamp_number(&slot.length, 1, 10000, BREAK_LENGTHS[i])));
            pairs.push((format!("Break{i}Random"), clamp_number(&slot.random, 0, 100, 33)));
            pairs.push((format!("Break{i}Logout"), clamp_number(&slot.logout, 0, 100, BREAK_LOGOUTS[i])));
        }
        for (i, slot) in self.tasks.iter().enumerate() {
            let enabled = VISIBLE_TASKS.contains(&i) && slot.enabled;
            pairs.push((format!("Task{i}Enabled"), bool_text(enabled)));
            pairs.push((format!("Task{i}Interval"), clamp_number(&slot.interval, 1, 10000, TASK_MINUTES[i])));
            pairs.push((format!("Task{i}Random"), clamp_number(&slot.random, 0, 100, 33)));
        }
        let owned: Vec<(&str, String)> = pairs.iter().map(|(key, value)| (key.as_str(), value.clone())).collect();
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = agility::write_section(&existing, &format!("{account} BASH Antiban"), &owned);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(path, text).map_err(|e| format!("Could not save antiban: {e}"))
    }
}

impl Webhook {
    fn load(text: &str, account: &str) -> Self {
        let body = section_body(text, &format!("{account} Webhook Settings"));
        Self {
            enabled: agility::ini_bool(&body, "EnableWebhooks", false),
            url: agility::ini_get(&body, "WebhookURL").unwrap_or_default(),
            test_note: String::new(),
        }
    }

    fn save(&self, path: &std::path::Path, account: &str) -> Result<(), String> {
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = agility::write_section(
            &existing,
            &format!("{account} Webhook Settings"),
            &[
                ("WebhookURL", self.url.trim().to_string()),
                ("EnableWebhooks", bool_text(self.enabled)),
            ],
        );
        fs::write(path, text).map_err(|e| format!("Could not save the webhook: {e}"))
    }
}

impl Default for Farm {
    fn default() -> Self {
        Self {
            herb: 0,
            flower: 0,
            allotment: 0,
            bush: 0,
            cactus: 0,
            anima: 0,
            tree: 0,
            fruit: 0,
            compost: 0,
            patches: vec![false; PATCHES.len()],
            travel: travels().iter().map(|_| 0).collect(),
            barb_dibber: false,
            magic_secateurs: false,
            lumby_elite: false,
            run_now: false,
        }
    }
}

impl Farm {
    fn load(text: &str, index: usize) -> Self {
        let body = section_body(text, &format!("BASH Farm{index}"));
        if body.trim().is_empty() {
            return Self::default();
        }
        let mut farm = Self::default();
        farm.herb = idx(&body, "Herb seed", HERBS.len());
        farm.flower = idx(&body, "Flower seed", FLOWERS.len());
        farm.allotment = idx(&body, "Allotment seed", ALLOTS.len());
        farm.bush = idx(&body, "Bush seed", BUSHES.len());
        farm.cactus = idx(&body, "Cactus seed", CACTI.len());
        farm.anima = idx(&body, "Anima seed", ANIMAS.len());
        farm.tree = idx(&body, "Tree sapling", TREES.len());
        farm.fruit = idx(&body, "Fruit sapling", FRUITS.len());
        farm.compost = idx(&body, "Compost", COMPOSTS.len());
        for (i, (_, key)) in PATCHES.iter().enumerate() {
            farm.patches[i] = agility::ini_bool(&body, key, false);
        }
        let options = travels();
        for (i, (_, key, choices)) in options.iter().enumerate() {
            farm.travel[i] = idx(&body, key, choices.len());
        }
        farm.barb_dibber = agility::ini_bool(&body, "Barb dibber", false);
        farm.magic_secateurs = agility::ini_bool(&body, "Magic secateurs", false);
        farm.lumby_elite = agility::ini_bool(&body, "Lumby elite", false);
        farm.run_now = agility::ini_bool(&body, "Run now", false);
        farm
    }

    fn save(&self, path: &std::path::Path, index: usize) -> Result<(), String> {
        let mut pairs = vec![
            ("Herb seed".into(), self.herb.to_string()),
            ("Flower seed".into(), self.flower.to_string()),
            ("Allotment seed".into(), self.allotment.to_string()),
            ("Bush seed".into(), self.bush.to_string()),
            ("Cactus seed".into(), "0".into()),
            ("Anima seed".into(), "0".into()),
            ("Tree sapling".into(), "0".into()),
            ("Fruit sapling".into(), "0".into()),
            ("Compost".into(), self.compost.to_string()),
            ("Barb dibber".into(), simba_bool(self.barb_dibber)),
            ("Magic secateurs".into(), simba_bool(self.magic_secateurs)),
            ("Lumby elite".into(), simba_bool(self.lumby_elite)),
            ("Run now".into(), simba_bool(self.run_now)),
            ("Enable patches".into(), "True".into()),
            ("Enable trees".into(), "False".into()),
            ("Enable birdhouse".into(), "True".into()),
            ("Starting location".into(), "0".into()),
        ];
        for (i, (_, key)) in PATCHES.iter().enumerate() {
            let on = i < 31 && self.patches.get(i).copied().unwrap_or(false);
            pairs.push(((*key).to_string(), simba_bool(on)));
        }
        let options = travels();
        for (i, (_, key, _)) in options.iter().enumerate() {
            pairs.push(((*key).to_string(), self.travel.get(i).copied().unwrap_or(0).to_string()));
        }
        let owned: Vec<(&str, String)> = pairs.iter().map(|(key, value)| (key.as_str(), value.clone())).collect();
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = agility::write_section(&existing, &format!("BASH Farm{index}"), &owned);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(path, text).map_err(|e| format!("Could not save farm settings: {e}"))
    }
}

impl Default for Birdhouse {
    fn default() -> Self {
        Self { log: 4, seed: 8, best_log: false, run_now: false }
    }
}

impl Birdhouse {
    fn load(text: &str, index: usize) -> Self {
        let body = section_body(text, &format!("BASH Birdhouse{index}"));
        if body.trim().is_empty() {
            return Self::default();
        }
        let mut bird = Self::default();
        bird.log = idx(&body, "Log type", LOGS.len());
        if let Some(seed) = agility::ini_get(&body, "Seed") {
            if let Some(found) = SEEDS.iter().position(|name| name.eq_ignore_ascii_case(&seed)) {
                bird.seed = found;
            }
        }
        bird.best_log = agility::ini_bool(&body, "Use best log", false);
        bird.run_now = agility::ini_bool(&body, "Run now", false);
        bird
    }

    fn save(&self, path: &std::path::Path, index: usize) -> Result<(), String> {
        let seed = SEEDS.get(self.seed).copied().unwrap_or("Potato seed");
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = agility::write_section(
            &existing,
            &format!("BASH Birdhouse{index}"),
            &[
                ("Log type", self.log.to_string()),
                ("Seed", seed.to_string()),
                ("Use best log", simba_bool(self.best_log)),
                ("Run now", simba_bool(self.run_now)),
            ],
        );
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(path, text).map_err(|e| format!("Could not save birdhouse settings: {e}"))
    }
}

fn idx(body: &str, key: &str, len: usize) -> usize {
    let parsed = agility::ini_get(body, key).and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    if len == 0 {
        0
    } else {
        parsed.min(len - 1)
    }
}

fn bool_text(value: bool) -> String {
    if value { "true".into() } else { "false".into() }
}

fn simba_bool(value: bool) -> String {
    if value { "True".into() } else { "False".into() }
}

fn decimal_or(value: &str, default: f64) -> String {
    let parsed = value.trim().parse::<f64>().unwrap_or(default);
    let rounded = (parsed * 100.0).round() / 100.0;
    if (rounded - rounded.round()).abs() < 0.001 {
        format!("{}", rounded.round() as i32)
    } else {
        format!("{rounded:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.clamp(min, max)
}

fn roll(span: i32) -> i32 {
    roll_between(0, span)
}

fn roll_between(min: i32, max: i32) -> i32 {
    if max <= min {
        return min;
    }
    min + (next_u32() % (max - min) as u32) as i32
}

fn next_u32() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0xA5A5_A5A5_A5A5_A5A5);
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    if x == 0 {
        x = 0xA5A5_A5A5_A5A5_A5A5;
    }
    STATE.store(x, Ordering::Relaxed);
    x as u32
}

fn account_can_biohash(account: &str) -> bool {
    let name = account.trim();
    !name.is_empty() && !name.eq_ignore_ascii_case("NoUserNameSelected")
}

fn field_hash(user: &str, field: &str) -> f64 {
    let user = user.trim();
    if user.is_empty() {
        return 0.5;
    }
    let text = format!("{}|{field}", user.to_lowercase());
    let mut hash: u32 = 0x811C_9DC5;
    for byte in text.bytes() {
        hash = hash.wrapping_mul(0x0100_0193) ^ u32::from(byte);
    }
    f64::from(hash) / f64::from(0xFFFF_FFFFu32)
}

fn scaled(base: i32, hash: f64) -> i32 {
    clamp_i32((f64::from(base) * (0.6 + hash * 0.8)).round() as i32, 1, 10000)
}

fn clamp_number(value: &str, min: i32, max: i32, default: i32) -> String {
    value.trim().parse::<i32>().unwrap_or(default).clamp(min, max).to_string()
}

fn read_accounts(path: &std::path::Path) -> Vec<Account> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut accounts = Vec::new();
    let mut rest = text.as_str();
    while let Some(found) = rest.find("Login.AddPlayer(") {
        rest = &rest[found + "Login.AddPlayer(".len()..];
        let Some((name, next)) = read_pascal_string(rest) else {
            break;
        };
        rest = next.trim_start().trim_start_matches(',').trim_start();
        let Some((_password, next)) = read_pascal_string(rest) else {
            break;
        };
        rest = next.trim_start().trim_start_matches(',').trim_start();
        let Some((pin, next)) = read_pascal_string(rest) else {
            break;
        };
        rest = next;
        let worlds = rest
            .find('[')
            .and_then(|start| rest[start + 1..].find(']').map(|end| rest[start + 1..start + 1 + end].to_string()))
            .unwrap_or_default();
        accounts.push(Account {
            name,
            pin,
            worlds: worlds.split(',').filter_map(|part| part.trim().parse::<i32>().ok()).map(|world| world.to_string()).collect::<Vec<_>>().join(", "),
        });
    }
    accounts
}

fn read_pascal_string(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let text = text.strip_prefix('\'')?;
    let mut out = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            return Some((out, &text[i + 1..]));
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    None
}

fn write_accounts(path: &std::path::Path, accounts: &[Account]) -> Result<(), String> {
    let mut body = String::from("begin\r\n");
    for account in accounts {
        let name = account.name.trim();
        if name.is_empty() {
            continue;
        }
        let worlds = account
            .worlds
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter_map(|part| part.trim().parse::<i32>().ok())
            .map(|world| world.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!(
            "  Login.AddPlayer('{}', '', '{}', [{}]);\r\n",
            name.replace('\'', "''"),
            account.pin.trim().replace('\'', "''"),
            worlds
        ));
    }
    body.push_str("end;\r\n");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, body).map_err(|e| format!("Could not save accounts: {e}"))
}

pub fn worlds_ready(editor: &Editor) -> Result<(), String> {
    let Some(account) = editor.accounts.get(editor.selected) else {
        return Err("Add the account before starting.".into());
    };
    let any = account.worlds.split(|ch: char| ch == ',' || ch.is_whitespace()).any(|part| part.trim().parse::<i32>().is_ok());
    if !any {
        return Err("Add at least one world for this account. You must have worlds selected for world hopping.".into());
    }
    if account.name.trim().is_empty() {
        return Err("The account needs a name.".into());
    }
    Ok(())
}
