//! Account, antiban, webhook, farm, and birdhouse settings.
//!
//! These are the same files the Simba form writes. The launcher can open any
//! page on its own, and the AIO Agility run setup includes all of them.

use std::collections::HashMap;
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
    Boat,
    Items,
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
    ping_rare: bool,
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
    enable_patches: bool,
    enable_birdhouse: bool,
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
    prayer_hotkey: String,
    inventory_hotkey: String,
    farm: Farm,
    bird: Birdhouse,
    show_pin: bool,
    show_webhook: bool,
    worlds_picker: crate::worlds::Picker,
    aichat: crate::aichat::Dialog,
    house: crate::house::Dialog,
    pub error: String,
}

impl Editor {
    pub fn blocks_input(&self) -> bool {
        self.house.is_open() || self.aichat.is_open()
    }
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
        let saved_as = editor.account_key();
        if crate::agility::is_aio_agility(script) {
            editor.page = Page::Course;
            editor.course = Some(CourseSettings::load_for(paths, &saved_as));
        } else if crate::scriptsetup::supports(&script.name) {
            editor.page = Page::Script;
            editor.script = Some(crate::scriptsetup::load(&paths.configs(), &script.name, &saved_as));
        }
        editor.client_hwnd = Some(client.hwnd);
        if script.name.eq_ignore_ascii_case("Tormented Demons") {
            editor.gear = Some(crate::gear::Editor::load(paths, &saved_as));
        }
        editor
    }

    pub fn for_page(paths: &SimbaPaths, client: Option<&ClientWindow>, page: Page) -> Self {
        let label = client.map(|client| client.account()).unwrap_or_else(|| "Not logged in".into());
        let key = agility::account_key(&label);
        let mut editor = Self::load(paths, &key, &label);
        editor.page = page;
        if page == Page::Accounts {
            editor.use_open_client(paths);
        }
        editor
    }

    fn load(paths: &SimbaPaths, account_key: &str, label: &str) -> Self {
        let accounts = read_accounts(&paths.root.join("credentials.simba"));
        let selected = accounts
            .iter()
            .position(|account| account.name.eq_ignore_ascii_case(account_key))
            .unwrap_or(0);
        let index = if accounts.is_empty() { 0 } else { selected };
        let settings_key = accounts
            .get(selected)
            .map(|account| account.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| account_key.to_string());
        let (antiban, antiban_hashed_for) = Antiban::load(&ini_text(&paths.configs().join("BASettings.ini")), &settings_key);
        let (prayer_hotkey, inventory_hotkey) = load_hotkeys(paths, &settings_key);
        Self {
            page: Page::Accounts,
            run_script: None,
            account_label: label.to_string(),
            antiban,
            antiban_hashed_for,
            webhook: Webhook::load(&ini_text(&paths.configs().join("BASettings.ini")), &settings_key),
            prayer_hotkey,
            inventory_hotkey,
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
            aichat: crate::aichat::Dialog::load(paths, &settings_key),
            house: crate::house::Dialog::load(paths, &settings_key, selected),
            error: String::new(),
        }
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
        if self
            .run_script
            .as_ref()
            .is_some_and(|script| script.name.eq_ignore_ascii_case("Standalone Farm and Birdhouse Runs"))
            && !self.farm.enable_patches
            && !self.farm.enable_birdhouse
        {
            return Err("Turn on farm runs, birdhouse runs, or both.".into());
        }
        Ok(())
    }

    /// Farm and Birdhouse tabs only show when this run supports them and the matching checkbox is on.
    /// Opening Accounts from the header keeps both tabs.
    fn side_run_tabs(&self) -> (bool, bool) {
        let Some(script) = &self.run_script else {
            return (true, true);
        };
        if script.name.eq_ignore_ascii_case("Standalone Farm and Birdhouse Runs") {
            return (self.farm.enable_patches, self.farm.enable_birdhouse);
        }
        let catalog_farm = script.features.iter().any(|feature| feature == "farm_runs");
        let catalog_bird = script.features.iter().any(|feature| feature == "birdhouses");
        if let Some(course) = &self.course {
            return (catalog_farm && course.farm, catalog_bird && course.birdhouse);
        }
        if let Some(values) = &self.script {
            if script.name.eq_ignore_ascii_case("AIO Bankstander") && !crate::scriptsetup::bank_side_runs_allowed(values) {
                return (false, false);
            }
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
            .map(|account| account.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "NoUserNameSelected".into())
    }

    /// Accounts page, with this account selected and the worlds list open.
    pub fn focus_worlds(&mut self, paths: &SimbaPaths, account: &str) -> bool {
        self.page = Page::Accounts;
        let name = account.trim();
        let Some(index) = self.accounts.iter().position(|item| item.name.eq_ignore_ascii_case(name)) else {
            return false;
        };
        if index != self.selected {
            self.reload_account(paths, index);
        }
        self.worlds_picker.target = index;
        let current = self.accounts[index].worlds.clone();
        self.worlds_picker.open_with(&current);
        true
    }

    /// Select the account whose name is on the only logged-in RuneLite window.
    fn use_open_client(&mut self, paths: &SimbaPaths) {
        let Some(name) = logged_in_account_name() else {
            return;
        };
        let Some(index) = self
            .accounts
            .iter()
            .position(|account| account.name.trim().eq_ignore_ascii_case(name.trim()))
        else {
            return;
        };
        if index != self.selected {
            self.reload_account(paths, index);
        }
    }

    pub fn save(&mut self, paths: &SimbaPaths) -> Result<(), String> {
        let key = self.account_key();
        if let Some(gear) = &mut self.gear {
            gear.persist(paths, &key)?;
        }
        if let Some(course) = &self.course {
            course.save(&paths.configs().join("BASettings.ini"), &key)?;
        }
        if let (Some(script), Some(values)) = (&self.run_script, &self.script) {
            crate::scriptsetup::save(&paths.configs(), &script.name, &key, values)?;
        }
        write_accounts(&paths.root.join("credentials.simba"), &self.accounts)?;
        self.antiban.save(&paths.configs().join("BASettings.ini"), &key)?;
        let ping_rare = self
            .run_script
            .as_ref()
            .is_some_and(|script| script.name.eq_ignore_ascii_case("Tormented Demons"));
        self.webhook.save(&paths.configs().join("BASettings.ini"), &key, ping_rare)?;
        save_hotkeys(&paths.configs().join("BASettings.ini"), &key, &self.prayer_hotkey, &self.inventory_hotkey)?;
        self.aichat.save_for(paths, &key)?;
        self.house.save_for(paths, &key)?;
        let index = self.selected;
        self.farm.save(&paths.configs().join("BASHFarmSettings"), index)?;
        self.bird.save(&paths.configs().join("BASHBirdHouseSettings"), index)?;
        Ok(())
    }

    fn reload_account(&mut self, paths: &SimbaPaths, index: usize) {
        let Some(account) = self.accounts.get(index) else {
            return;
        };
        let trimmed = account.name.trim();
        let key = if trimmed.is_empty() {
            "NoUserNameSelected".to_string()
        } else {
            trimmed.to_string()
        };
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
            self.gear = Some(crate::gear::Editor::load(paths, &key));
        }
        let (prayer_hotkey, inventory_hotkey) = load_hotkeys(paths, &key);
        self.prayer_hotkey = prayer_hotkey;
        self.inventory_hotkey = inventory_hotkey;
        let keep_open = self.aichat.is_open();
        self.aichat = crate::aichat::Dialog::load(paths, &key);
        self.aichat.set_open(keep_open);
        let house_open = self.house.is_open();
        self.house = crate::house::Dialog::load(paths, &key, index);
        self.house.set_open(house_open);
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
    let mut reload_only: Option<usize> = None;
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
    let blocked = editor.blocks_input();
    let logged_in = logged_in_names();
    let shown = egui::Window::new(title)
        .id(egui::Id::new("bash-settings"))
        .enabled(!blocked)
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
            account_picker(ui, editor, &mut switch_to, &logged_in);
            if page_tabs(ui, editor) {
                editor.use_open_client(paths);
            }
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
                            let standalone = editor.run_script.as_ref().is_some_and(|script| {
                                script.name.eq_ignore_ascii_case("Standalone Farm and Birdhouse Runs")
                            });
                            let request = if standalone {
                                standalone_script_page(ui, &mut editor.farm);
                                None
                            } else if let Some(script) = editor.run_script.clone() {
                                if let Some(values) = editor.script.as_mut() {
                                    crate::scriptsetup::show(ui, &script.name, values);
                                    crate::salvage::take_page(values)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if request.as_deref() == Some("Boat") {
                                editor.page = Page::Boat;
                            }
                        }
                        Page::Boat => {
                            let request = if let Some(values) = editor.script.as_mut() {
                                crate::salvage::show_boat(ui, values);
                                crate::salvage::take_page(values)
                            } else {
                                None
                            };
                            if request.as_deref() == Some("Items") {
                                editor.page = Page::Items;
                            }
                        }
                        Page::Items => {
                            if let Some(values) = editor.script.as_mut() {
                                crate::salvage::show_items(ui, values);
                            }
                        }
                        Page::Gear => {
                            let hwnd = editor.client_hwnd;
                            let account = editor.account_key();
                            if let Some(gear) = editor.gear.as_mut() {
                                gear.ui(ui, paths, hwnd, &account);
                            }
                        }
                        Page::Accounts => accounts_page(ui, editor, &mut switch_to, &mut reload_only, &logged_in),
                        Page::Antiban => {
                            let label = editor.selected_name();
                            let hash_name = editor
                                .accounts
                                .get(editor.selected)
                                .map(|account| account.name.trim().to_string())
                                .unwrap_or_default();
                            let script_name = editor.run_script.as_ref().map(|script| script.name.clone());
                            let mut refreshed = false;
                            antiban_page(
                                ui,
                                &mut editor.antiban,
                                &label,
                                &hash_name,
                                &mut refreshed,
                                script_name.as_deref(),
                                editor.script.as_mut(),
                                &mut editor.aichat,
                            );
                            if refreshed {
                                editor.antiban_hashed_for = Some(hash_name);
                            }
                        }
                        Page::Webhooks => {
                            let account = editor.selected_name();
                            let ping_rare = editor
                                .run_script
                                .as_ref()
                                .is_some_and(|script| script.name.eq_ignore_ascii_case("Tormented Demons"));
                            webhook_page(ui, &mut editor.webhook, &mut editor.show_webhook, &account, ping_rare);
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
    if blocked {
        if let Some(shown) = &shown {
            shield_window(ctx, shown.response.layer_id);
        }
    }
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
    } else if let Some(index) = reload_only {
        if editor.accounts.is_empty() {
            editor.selected = 0;
        } else {
            editor.reload_account(paths, index);
        }
    }
    if let Some(worlds) = crate::worlds::show(ctx, &mut editor.worlds_picker, !blocked) {
        let index = editor.worlds_picker.target;
        if let Some(account) = editor.accounts.get_mut(index) {
            account.worlds = worlds;
        }
    }
    let account = editor.account_key();
    let house_index = editor.selected;
    crate::aichat::show(ctx, paths, &mut editor.aichat, &account);
    crate::house::show(ctx, paths, &mut editor.house, &account, house_index);
    event
}

fn logged_in_names() -> Vec<String> {
    let mut names = Vec::new();
    for client in crate::clients::game_clients() {
        let name = client.account();
        if crate::agility::account_key(&name) == "NoUserNameSelected" {
            continue;
        }
        if names.iter().any(|have: &String| have.eq_ignore_ascii_case(&name)) {
            continue;
        }
        names.push(name);
    }
    names
}

fn logged_in_account_name() -> Option<String> {
    let mut names = logged_in_names();
    if names.len() == 1 {
        names.pop()
    } else {
        None
    }
}

fn account_is_logged_in(name: &str, logged_in: &[String]) -> bool {
    let name = name.trim();
    !name.is_empty() && logged_in.iter().any(|have| have.eq_ignore_ascii_case(name))
}

fn account_list_label(name: &str, logged_in: &[String]) -> String {
    let shown = if name.trim().is_empty() { "Unnamed".to_string() } else { name.trim().to_string() };
    if account_is_logged_in(name, logged_in) {
        format!("{shown} (Logged In)")
    } else {
        shown
    }
}

fn shield_window(ctx: &egui::Context, window: egui::LayerId) {
    let screen = ctx.screen_rect();
    let shown = egui::Area::new(egui::Id::new("bash-settings-shield"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .constrain(false)
        .interactable(true)
        .show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
        });
    ctx.set_sublayer(window, shown.response.layer_id);
}

fn page_tabs(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut pages = Vec::new();
    let salvage = editor.run_script.as_ref().is_some_and(|script| crate::salvage::is_script(&script.name));
    if editor.course.is_some() {
        pages.push((Page::Course, "Course"));
    } else if editor.script.is_some() {
        pages.push((Page::Script, "Script"));
        if salvage {
            pages.push((Page::Boat, "Boat"));
            let station = editor.script.as_ref().is_some_and(crate::salvage::station_enabled);
            if station {
                pages.push((Page::Items, "Items"));
            } else if editor.page == Page::Items {
                editor.page = Page::Boat;
            }
        }
    }
    if matches!(editor.page, Page::Boat | Page::Items) && !salvage {
        editor.page = if editor.script.is_some() { Page::Script } else { Page::Accounts };
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
    let mut pick_client = false;
    ui.horizontal(|ui| {
        let width = ((SETTINGS_WIDTH - 16.0) / pages.len() as f32) - 4.0;
        for (page, label) in pages {
            if ui.add_sized([width, 30.0], egui::SelectableLabel::new(editor.page == page, label)).clicked() {
                editor.page = page;
                if page == Page::Accounts {
                    pick_client = true;
                }
            }
        }
    });
    pick_client
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

fn account_picker(ui: &mut egui::Ui, editor: &Editor, switch_to: &mut Option<usize>, logged_in: &[String]) {
    if editor.accounts.is_empty() {
        ui.label("Add an account on the Accounts tab. Each account keeps its own settings.");
        ui.add_space(6.0);
        return;
    }
    let raw = editor.accounts.get(editor.selected).map(|account| account.name.clone()).unwrap_or_default();
    let current = account_list_label(&raw, logged_in);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Account").strong());
        ComboBox::from_id_salt("settings-account")
            .width(320.0)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for index in 0..editor.accounts.len() {
                    let name = account_list_label(&editor.accounts[index].name, logged_in);
                    if ui.selectable_label(index == editor.selected, name).clicked() && index != editor.selected {
                        *switch_to = Some(index);
                    }
                }
            });
        crate::ui::help_tip(ui, "Settings are saved for this account and loaded when you change it.");
    });
    ui.add_space(6.0);
}

fn accounts_page(ui: &mut egui::Ui, editor: &mut Editor, switch_to: &mut Option<usize>, reload_only: &mut Option<usize>, logged_in: &[String]) {
    ui.label("Pick an account on the left. Name, bank pin, and worlds are shared by every script.");
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(280.0);
            ui.label(RichText::new("Accounts").strong());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                if editor.accounts.is_empty() {
                    ui.label("None yet.");
                } else {
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for index in 0..editor.accounts.len() {
                            let name = account_list_label(&editor.accounts[index].name, logged_in);
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
                        if editor.selected < editor.accounts.len() {
                            editor.accounts.remove(editor.selected);
                        }
                        editor.selected = if editor.accounts.is_empty() {
                            0
                        } else {
                            editor.selected.min(editor.accounts.len() - 1)
                        };
                        *reload_only = Some(editor.selected);
                    }
                }
            });
        });
        ui.add_space(18.0);
        ui.vertical(|ui| {
            ui.set_min_width(620.0);
            if editor.accounts.is_empty() {
                ui.label("Add an account to set the name, bank pin, and worlds.");
                ui.add_space(14.0);
                ui.label(RichText::new("House").strong());
                ui.label(RichText::new(editor.house.status_line()).weak());
                house_button(ui, &mut editor.house);
                ui.add_space(14.0);
                ui.label(RichText::new("AI Chat").strong());
                ui.label(RichText::new(editor.aichat.status_line()).weak());
                aichat_button(ui, &mut editor.aichat);
                return;
            }
            ui.label(RichText::new("Details").strong());
            ui.add_space(4.0);
            if editor.selected >= editor.accounts.len() {
                editor.selected = editor.accounts.len() - 1;
            }
            let mut show_pin = editor.show_pin;
            let mut open_worlds = false;
            {
                let account = &mut editor.accounts[editor.selected];
                egui::Grid::new("account-identity").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Account name");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut account.name).desired_width(280.0));
                        if account_is_logged_in(&account.name, logged_in) {
                            ui.label(RichText::new("Logged In").weak());
                        }
                    });
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
            ui.add_space(14.0);
            ui.label(RichText::new("Game tabs").strong());
            ui.label(RichText::new("Saved with this account. Every script uses these keys.").weak());
            ui.add_space(4.0);
            hotkey_row(ui, "Prayer hotkey", "The OSRS Controls key bound to the Prayer book. It must match Settings, Controls.", &mut editor.prayer_hotkey);
            hotkey_row(
                ui,
                "Inventory hotkey",
                "The OSRS Controls key bound to the Inventory. It must match Settings, Controls, and it cannot match the prayer hotkey.",
                &mut editor.inventory_hotkey,
            );
            if editor.prayer_hotkey.eq_ignore_ascii_case(&editor.inventory_hotkey) {
                editor.inventory_hotkey = next_hotkey(&editor.prayer_hotkey);
            }
            ui.add_space(14.0);
            ui.label(RichText::new("House").strong());
            ui.label(RichText::new(editor.house.status_line()).weak());
            ui.label(RichText::new("Saved for this account. Scripts use it to walk inside the house.").weak());
            ui.add_space(4.0);
            house_button(ui, &mut editor.house);
            ui.add_space(14.0);
            ui.label(RichText::new("AI Chat").strong());
            ui.label(RichText::new(editor.aichat.status_line()).weak());
            ui.label(RichText::new("Saved for this account. Scripts use it when this account starts.").weak());
            ui.add_space(4.0);
            aichat_button(ui, &mut editor.aichat);
        });
    });
}

fn house_button(ui: &mut egui::Ui, house: &mut crate::house::Dialog) {
    if ui
        .add_enabled(house.has_account(), egui::Button::new("House Setup"))
        .on_hover_text("Rooms, portals, pool, and how this account gets to the house.")
        .clicked()
    {
        house.request();
    }
}

fn aichat_button(ui: &mut egui::Ui, aichat: &mut crate::aichat::Dialog) {
    if ui
        .add_enabled(
            aichat.has_account(),
            egui::Button::new("Setup AIChat"),
        )
        .on_hover_text("API key, reply style, and custom instructions for this account.")
        .clicked()
    {
        aichat.request();
    }
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

fn antiban_page(
    ui: &mut egui::Ui,
    antiban: &mut Antiban,
    label: &str,
    account: &str,
    refreshed: &mut bool,
    script: Option<&str>,
    mut activity: Option<&mut HashMap<String, String>>,
    aichat: &mut crate::aichat::Dialog,
) {
    let chances = script.map(crate::scriptsetup::activity_chances).unwrap_or(&[]);
    for_account(ui, label);
    ui.horizontal(|ui| {
        if ui.button("Randomise").on_hover_text("Picks new break, sleep, task, world hop, and activity numbers.").clicked() {
            antiban.randomise();
            if let Some(values) = activity.as_deref_mut() {
                roll_activity(chances, values, account, true);
            }
        }
        if ui.button("BioHash/Default").on_hover_text("Uses this account name, so the numbers stay the same every time.").clicked() {
            antiban.apply_biohash(account);
            if let Some(values) = activity.as_deref_mut() {
                roll_activity(chances, values, account, false);
            }
            *refreshed = true;
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        aichat_button(ui, aichat);
        ui.label(RichText::new(aichat.status_line()).weak());
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
    if !chances.is_empty() {
        if let Some(values) = activity.as_deref_mut() {
            ui.add_space(10.0);
            tip_heading(
                ui,
                "Activity actions",
                "Chances for this script. Randomise and BioHash/Default fill these along with the break and sleep numbers.",
            );
            for chance in chances {
                let mut text = values.get(chance.key).cloned().unwrap_or_default();
                if text.trim().is_empty() {
                    text = chance.default.to_string();
                }
                ui.horizontal(|ui| {
                    ui.label(chance.label);
                    crate::ui::help_tip(ui, chance.tip);
                    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(52.0));
                    if response.lost_focus() {
                        text = text.trim().parse::<i32>().unwrap_or(chance.default).clamp(0, 100).to_string();
                    }
                });
                values.insert(chance.key.to_string(), text);
            }
        }
    }
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

fn webhook_page(ui: &mut egui::Ui, webhook: &mut Webhook, show_url: &mut bool, account: &str, ping_rare: bool) {
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
        if ping_rare {
            ui.add_space(6.0);
            tip_check(
                ui,
                &mut webhook.ping_rare,
                "Ping on rare drop",
                "When a Tormented synapse or Burning claw is looted, send a Discord webhook with a screenshot.",
            );
        }
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

fn standalone_script_page(ui: &mut egui::Ui, farm: &mut Farm) {
    ui.label(RichText::new("Standalone Farm and Birdhouse Runs").strong());
    ui.add_space(8.0);
    tip_check(
        ui,
        &mut farm.enable_patches,
        "Farm runs",
        "Herb, flower, allotment, and bush runs. Turn this off to skip farming.",
    );
    tip_check(
        ui,
        &mut farm.enable_birdhouse,
        "Birdhouse runs",
        "Birdhouse runs on Fossil Island. Turn this off to skip birdhouses.",
    );
    ui.add_space(8.0);
    tip_heading(ui, "Location", "Grand Exchange is the only starting location right now.");
    ui.add_enabled_ui(false, |ui| {
        ComboBox::from_id_salt("standalone-start-location")
            .width(360.0)
            .selected_text("Grand Exchange")
            .show_ui(ui, |_| {});
    });
    ui.add_space(8.0);
    if farm.enable_patches {
        ui.label("Choose the patches on the Farm tab.");
    }
    if farm.enable_birdhouse {
        ui.label("Choose the log and seed on the Birdhouse tab.");
    }
}

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
            ping_rare: agility::ini_bool(&body, "PingOnRareDrop", true),
            test_note: String::new(),
        }
    }

    fn save(&self, path: &std::path::Path, account: &str, ping_rare: bool) -> Result<(), String> {
        let mut pairs = vec![
            ("WebhookURL", self.url.trim().to_string()),
            ("EnableWebhooks", bool_text(self.enabled)),
        ];
        if ping_rare {
            pairs.push(("PingOnRareDrop", bool_text(self.ping_rare)));
        }
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = agility::write_section(&existing, &format!("{account} Webhook Settings"), &pairs);
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
            enable_patches: true,
            enable_birdhouse: true,
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
        farm.enable_patches = agility::ini_bool(&body, "Enable patches", true);
        farm.enable_birdhouse = agility::ini_bool(&body, "Enable birdhouse", true);
        farm
    }

    fn save(&self, path: &std::path::Path, index: usize) -> Result<(), String> {
        let mut pairs = vec![
            ("Herb seed".into(), self.herb.to_string()),
            ("Flower seed".into(), self.flower.to_string()),
            ("Allotment seed".into(), self.allotment.to_string()),
            ("Bush seed".into(), self.bush.to_string()),
            ("Cactus seed".into(), self.cactus.to_string()),
            ("Anima seed".into(), self.anima.to_string()),
            ("Tree sapling".into(), self.tree.to_string()),
            ("Fruit sapling".into(), self.fruit.to_string()),
            ("Compost".into(), self.compost.to_string()),
            ("Barb dibber".into(), simba_bool(self.barb_dibber)),
            ("Magic secateurs".into(), simba_bool(self.magic_secateurs)),
            ("Lumby elite".into(), simba_bool(self.lumby_elite)),
            ("Run now".into(), simba_bool(self.run_now)),
            ("Enable patches".into(), simba_bool(self.enable_patches)),
            ("Enable trees".into(), "False".into()),
            ("Enable birdhouse".into(), simba_bool(self.enable_birdhouse)),
            ("Starting location".into(), "0".into()),
        ];
        for (i, (_, key)) in PATCHES.iter().enumerate() {
            let on = self.patches.get(i).copied().unwrap_or(false);
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

fn roll_activity(chances: &[crate::scriptsetup::ActivityChance], values: &mut HashMap<String, String>, account: &str, random: bool) {
    for chance in chances {
        let value = if random {
            roll_between(chance.low, chance.high + 1)
        } else {
            let span = (chance.high - chance.low).max(0);
            chance.low + (field_hash(account, chance.key) * f64::from(span)).round() as i32
        };
        values.insert(chance.key.to_string(), value.clamp(0, 100).to_string());
    }
}

const HOTKEYS: [&str; 8] = ["ESC", "F1", "F2", "F3", "F4", "F5", "F6", "F7"];

fn snap_hotkey(value: &str, default: &str) -> String {
    HOTKEYS
        .iter()
        .find(|item| item.eq_ignore_ascii_case(value.trim()))
        .copied()
        .unwrap_or(default)
        .to_string()
}

fn next_hotkey(taken: &str) -> String {
    HOTKEYS.iter().find(|item| !item.eq_ignore_ascii_case(taken)).copied().unwrap_or("F1").to_string()
}

fn hotkey_row(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut String) {
    let default = if label.to_ascii_lowercase().contains("inventory") { "ESC" } else { "F3" };
    let mut current = snap_hotkey(value, default);
    ui.horizontal(|ui| {
        ui.label(label);
        crate::ui::help_tip(ui, tip);
        ComboBox::from_id_salt(("account-hotkey", label))
            .width(120.0)
            .selected_text(&current)
            .show_ui(ui, |ui| {
                for option in HOTKEYS {
                    ui.selectable_value(&mut current, option.to_string(), option);
                }
            });
    });
    *value = current;
}

fn load_hotkeys(paths: &SimbaPaths, account: &str) -> (String, String) {
    let settings = ini_text(&paths.configs().join("BASettings.ini"));
    let controls = agility::ini_section(&settings, &format!("{account} Controls"));
    let demons = ini_text(&paths.configs().join("tormenteddemons.ini"));
    let combat = agility::ini_section(&demons, &format!("{account} Combat"));
    let shared = agility::ini_section(&demons, "Combat");
    let prayer = agility::ini_get(&controls, "PrayerHotkey")
        .or_else(|| agility::ini_get(&combat, "PrayerHotkey"))
        .or_else(|| agility::ini_get(&shared, "PrayerHotkey"))
        .unwrap_or_else(|| "F3".into());
    let inventory = agility::ini_get(&controls, "InventoryHotkey")
        .or_else(|| agility::ini_get(&combat, "InventoryHotkey"))
        .or_else(|| agility::ini_get(&shared, "InventoryHotkey"))
        .unwrap_or_else(|| "ESC".into());
    let prayer = snap_hotkey(&prayer, "F3");
    let mut inventory = snap_hotkey(&inventory, "ESC");
    if prayer.eq_ignore_ascii_case(&inventory) {
        inventory = next_hotkey(&prayer);
    }
    (prayer, inventory)
}

fn save_hotkeys(path: &std::path::Path, account: &str, prayer: &str, inventory: &str) -> Result<(), String> {
    let prayer = snap_hotkey(prayer, "F3");
    let mut inventory = snap_hotkey(inventory, "ESC");
    if prayer.eq_ignore_ascii_case(&inventory) {
        inventory = next_hotkey(&prayer);
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    let text = agility::write_section(
        &existing,
        &format!("{account} Controls"),
        &[("PrayerHotkey", prayer), ("InventoryHotkey", inventory)],
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, text).map_err(|error| format!("Could not save the account hotkeys: {error}"))
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

pub fn account_has_worlds(paths: &SimbaPaths, account: &str) -> bool {
    let name = account.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("NoUserNameSelected") {
        return false;
    }
    read_accounts(&paths.root.join("credentials.simba"))
        .into_iter()
        .any(|item| item.name.eq_ignore_ascii_case(name) && worlds_present(&item.worlds))
}

pub fn account_is_saved(paths: &SimbaPaths, account: &str) -> bool {
    let name = account.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("NoUserNameSelected") {
        return false;
    }
    read_accounts(&paths.root.join("credentials.simba"))
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(name))
}

fn worlds_present(worlds: &str) -> bool {
    worlds.split(|ch: char| ch == ',' || ch.is_whitespace()).any(|part| part.trim().parse::<i32>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::worlds_present;

    #[test]
    fn a_world_number_counts_as_a_list() {
        assert!(worlds_present("301, 302"));
        assert!(worlds_present("  394"));
        assert!(!worlds_present(""));
        assert!(!worlds_present("none"));
    }
}
