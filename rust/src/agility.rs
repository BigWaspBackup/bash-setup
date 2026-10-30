//! AIO Agility setup shown by the launcher before Simba starts.
//!
//! The script reads the same `Configs/BASettings.ini` section the Simba form
//! writes: `{account} AIO Agility Settings`.

use std::{fs, path::Path};

use eframe::egui::{self, Button, ComboBox, RichText, ScrollArea};

use crate::{catalog::ScriptEntry, clients::ClientWindow, paths::SimbaPaths, ui as chrome};

pub const SCRIPT_ID: &str = "c8f3d1a4-7b2e-4e91-a6c5-9d0f2b8e4a17";
const SETTINGS_SUFFIX: &str = " AIO Agility Settings";
const SETTINGS_FILE: &str = "BASettings.ini";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Rooftop,
    Wilderness,
    Prif,
    Wyrm,
    Ape,
}

struct Course {
    name: &'static str,
    kind: Kind,
    level: i32,
    marks: bool,
    returns: &'static [&'static str],
    extra: &'static [&'static str],
}

const COURSES: &[Course] = &[
    Course {
        name: "Al Kharid Rooftop",
        kind: Kind::Rooftop,
        level: 20,
        marks: true,
        returns: &["Amulet of Glory", "Ring of Dueling", "Ring of elements"],
        extra: &[],
    },
    Course {
        name: "Ape Atoll",
        kind: Kind::Ape,
        level: 48,
        marks: true,
        returns: &[],
        extra: &["Ninja monkey greegree (small, medium, or Kruk)", "Monkey Madness I (chapter 2)"],
    },
    Course {
        name: "Ardougne Rooftop",
        kind: Kind::Rooftop,
        level: 90,
        marks: true,
        returns: &["Ardougne Teleport", "Ardougne Cloak"],
        extra: &["Easy Ardougne Diary"],
    },
    Course {
        name: "Canifis Rooftop",
        kind: Kind::Rooftop,
        level: 40,
        marks: true,
        returns: &[
            "Salve Graveyard Teleport",
            "Salve Graveyard Tablet",
            "Fenkenstrain Teleport",
            "Fenkenstrain Tablet",
        ],
        extra: &["Priest in Peril (Canifis access)"],
    },
    Course {
        name: "Colossal Wyrm",
        kind: Kind::Wyrm,
        level: 50,
        marks: false,
        returns: &[
            "Colossal wyrm teleport scroll",
            "Quetzal whistle",
            "Civitas teleport",
            "Fairy ring (AJP)",
        ],
        extra: &["Children of the Sun (Varlamore)", "62+ uses the advanced route"],
    },
    Course {
        name: "Draynor Village Rooftop",
        kind: Kind::Rooftop,
        level: 1,
        marks: true,
        returns: &["Amulet of Glory"],
        extra: &[],
    },
    Course {
        name: "Falador Rooftop",
        kind: Kind::Rooftop,
        level: 50,
        marks: true,
        returns: &["Falador Teleport", "Ring of Wealth"],
        extra: &[],
    },
    Course {
        name: "Pollnivneach Rooftop",
        kind: Kind::Rooftop,
        level: 70,
        marks: true,
        returns: &[],
        extra: &[],
    },
    Course {
        name: "Prifddinas",
        kind: Kind::Prif,
        level: 75,
        marks: false,
        returns: &[
            "Construction cape",
            "Construction cape trimmed",
            "House teleport tab",
            "House teleport runes",
            "Teleport crystal",
            "Eternal teleport crystal",
        ],
        extra: &["Song of the Elves"],
    },
    Course {
        name: "Rellekka Rooftop",
        kind: Kind::Rooftop,
        level: 80,
        marks: true,
        returns: &[],
        extra: &[],
    },
    Course {
        name: "Seers' Village Rooftop",
        kind: Kind::Rooftop,
        level: 60,
        marks: true,
        returns: &["Camelot Teleport"],
        extra: &[],
    },
    Course {
        name: "Varrock Rooftop",
        kind: Kind::Rooftop,
        level: 30,
        marks: true,
        returns: &["Varrock Teleport", "Varrock Tablet", "Amulet of Glory"],
        extra: &[],
    },
    Course {
        name: "Wilderness Agility Course",
        kind: Kind::Wilderness,
        level: 52,
        marks: false,
        returns: &[],
        extra: &["Food", "Entity Hider (hide players and NPCs Enabled)"],
    },
];

#[derive(Clone)]
pub struct Settings {
    pub course: String,
    pub farm: bool,
    pub birdhouse: bool,
    pub return_method: String,
    pub stop_marks: String,
    pub stop_termites: String,
    pub stop_shards: String,
    pub stop_tickets: String,
    pub disable_hop: bool,
    pub disable_breaks: bool,
    pub disable_sleeps: bool,
    pub eat_at: String,
    pub overheal: bool,
    pub wyrm_route: String,
    pub lumby_elite: bool,
    pub wilderness_ack: bool,
    pub max_time: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            course: COURSES[0].name.to_string(),
            farm: false,
            birdhouse: false,
            return_method: "(none)".into(),
            stop_marks: "-1".into(),
            stop_termites: "-1".into(),
            stop_shards: "-1".into(),
            stop_tickets: "-1".into(),
            disable_hop: false,
            disable_breaks: false,
            disable_sleeps: false,
            eat_at: "75".into(),
            overheal: false,
            wyrm_route: "Automatic".into(),
            lumby_elite: false,
            wilderness_ack: false,
            max_time: "0".into(),
        }
    }
}

pub struct Setup {
    pub script: ScriptEntry,
    /// Logged-in account, or `NoUserNameSelected` when the client has no name yet.
    pub account_key: String,
    pub account_label: String,
    pub settings: Settings,
    pub error: String,
}

pub enum SetupEvent {
    None,
    Cancel,
    Start,
}

pub fn is_aio_agility(script: &ScriptEntry) -> bool {
    script.id == SCRIPT_ID || script.name.eq_ignore_ascii_case("AIO Agility")
}

/// INI section user. Matches `BASHPremiumCurrentUser` when nobody is selected.
pub fn account_key(account: &str) -> String {
    let name = account.trim();
    if name.is_empty() || name == "Not logged in" || name == "RuneLite" {
        "NoUserNameSelected".to_string()
    } else {
        name.to_string()
    }
}

impl Setup {
    pub fn open(paths: &SimbaPaths, script: &ScriptEntry, client: &ClientWindow) -> Self {
        let account_label = client.account();
        let account_key = account_key(&account_label);
        let settings = Settings::load(&paths.configs().join(SETTINGS_FILE), &account_key);
        Self {
            script: script.clone(),
            account_key,
            account_label,
            settings,
            error: String::new(),
        }
    }

    pub fn save(&self, paths: &SimbaPaths) -> Result<(), String> {
        let dir = paths.configs();
        fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        self.settings.save(&dir.join(SETTINGS_FILE), &self.account_key)
    }
}

impl Settings {
    fn course(&self) -> &'static Course {
        COURSES.iter().find(|course| course.name == self.course).unwrap_or(&COURSES[0])
    }

    fn supports_side_runs(&self) -> bool {
        !self.course().returns.is_empty()
    }

    pub fn load_for(paths: &SimbaPaths, account: &str) -> Self {
        let mut settings = Self::load(&paths.configs().join(SETTINGS_FILE), account);
        let session = format!("{account} BigAussies AIO Agility Session");
        if let Ok(text) = fs::read_to_string(paths.configs().join(SETTINGS_FILE)) {
            let body = ini_section(&text, &session);
            if let Some(max_time) = ini_get(&body, "MaxTime") {
                settings.max_time = max_time;
            }
        }
        settings
    }

    pub fn validate(&self) -> Result<(), String> {
        let course = self.course();
        if course.kind == Kind::Wilderness && !self.wilderness_ack {
            return Err("Read the Wilderness warning and tick the box before starting.".into());
        }
        if (self.farm || self.birdhouse) && !self.supports_side_runs() {
            return Err(format!("Farm / birdhouse runs are not available at {}.", course.name));
        }
        if (self.farm || self.birdhouse)
            && (course.returns.is_empty() || self.return_method.trim().is_empty() || self.return_method == "(none)")
        {
            return Err("Farm / birdhouse runs need a return method for this course.".into());
        }
        Ok(())
    }

    fn load(path: &Path, account: &str) -> Self {
        let mut settings = Self::default();
        let Ok(text) = fs::read_to_string(path) else {
            settings.sync_return_method();
            return settings;
        };
        let section = ini_section(&text, &section_name(account));
        if section.is_empty() {
            settings.sync_return_method();
            return settings;
        }
        if let Some(course) = ini_get(&section, "Course") {
            if COURSES.iter().any(|item| item.name == course) {
                settings.course = course;
            }
        }
        settings.farm = ini_bool(&section, "EnableFarmRuns", false);
        settings.birdhouse = ini_bool(&section, "EnableBirdhouseRuns", false);
        settings.return_method = ini_get(&section, "BankReturnMethod").unwrap_or_else(|| "(none)".into());
        settings.stop_marks = ini_get(&section, "StopAtMarks").unwrap_or_else(|| "-1".into());
        settings.stop_termites = ini_get(&section, "StopAtTermites").unwrap_or_else(|| "-1".into());
        settings.stop_shards = ini_get(&section, "StopAtShards").unwrap_or_else(|| "-1".into());
        settings.stop_tickets = ini_get(&section, "StopAtTickets").unwrap_or_else(|| "-1".into());
        settings.disable_hop = ini_bool(&section, "DisableWorldHopping", false);
        settings.disable_breaks = ini_bool(&section, "DisableBreaks", false);
        settings.disable_sleeps = ini_bool(&section, "DisableSleeps", false);
        settings.eat_at = ini_get(&section, "EatAtHPPercent").unwrap_or_else(|| "75".into());
        settings.overheal = ini_bool(&section, "OverhealAnglerfish", false);
        settings.wyrm_route = match ini_get(&section, "WyrmRoute").as_deref() {
            Some(route) if route.eq_ignore_ascii_case("Basic") => "Basic".into(),
            Some(route) if route.eq_ignore_ascii_case("Advanced") => "Advanced".into(),
            _ => "Automatic".into(),
        };
        settings.lumby_elite = ini_bool(&section, "WyrmLumbyElite", false);
        settings.wilderness_ack = ini_bool(&section, "WildernessWarningShown3", false);
        settings.max_time = "0".into();
        settings.sync_return_method();
        if !settings.supports_side_runs() {
            settings.farm = false;
            settings.birdhouse = false;
        }
        settings
    }

    pub fn save(&self, path: &Path, account: &str) -> Result<(), String> {
        let eat_at = clamp_percent(&self.eat_at);
        let pairs = [
            ("Course", self.course().name.to_string()),
            ("EnableFarmRuns", bool_text(self.farm && self.supports_side_runs())),
            ("EnableBirdhouseRuns", bool_text(self.birdhouse && self.supports_side_runs())),
            ("BankReturnMethod", self.return_method.clone()),
            ("StopAtMarks", int_text(&self.stop_marks)),
            ("StopAtTermites", int_text(&self.stop_termites)),
            ("StopAtShards", int_text(&self.stop_shards)),
            ("StopAtTickets", int_text(&self.stop_tickets)),
            ("DisableWorldHopping", bool_text(self.disable_hop)),
            ("DisableBreaks", bool_text(self.disable_breaks)),
            ("DisableSleeps", bool_text(self.disable_sleeps)),
            ("EatAtHPPercent", eat_at.to_string()),
            ("OverhealAnglerfish", bool_text(self.overheal)),
            ("WyrmRoute", self.wyrm_route.clone()),
            ("WyrmLumbyElite", bool_text(self.lumby_elite)),
            ("WildernessWarningShown3", bool_text(self.wilderness_ack)),
        ];
        let existing = fs::read_to_string(path).unwrap_or_default();
        let text = write_section(&existing, &section_name(account), &pairs);
        let session = format!("{account} BigAussies AIO Agility Session");
        let text = write_section(
            &text,
            &session,
            &[("MaxTime", int_text(&self.max_time).parse::<i32>().unwrap_or(0).max(0).to_string())],
        );
        fs::write(path, text).map_err(|e| format!("Could not save {}: {e}", path.display()))
    }

    fn sync_return_method(&mut self) {
        let methods = self.course().returns;
        if methods.is_empty() {
            self.return_method = "(none)".into();
            return;
        }
        if !methods.iter().any(|method| method.eq_ignore_ascii_case(&self.return_method)) {
            self.return_method = methods[0].to_string();
        }
    }

    fn on_course_changed(&mut self) {
        if !self.supports_side_runs() {
            self.farm = false;
            self.birdhouse = false;
        }
        self.sync_return_method();
    }

    fn required_text(&self) -> String {
        let course = self.course();
        let mut lines = vec![format!("Agility {}+", course.level)];
        for extra in course.extra {
            push_unique(&mut lines, extra);
        }
        push_unique(&mut lines, "Agility plugin overlay");
        if (self.farm || self.birdhouse) && self.supports_side_runs() {
            if let Some(item) = return_item(&self.return_method) {
                push_unique(&mut lines, &item);
            }
        }
        if course.kind == Kind::Wilderness && self.overheal {
            push_unique(&mut lines, "Blighted anglerfish");
        }
        lines.join("\n")
    }
}

pub fn course_ui(ui: &mut egui::Ui, settings: &mut Settings) {
    let mut course_changed = false;
    let course_name = settings.course.clone();
    ComboBox::from_id_salt("agility-course")
        .width(320.0)
        .selected_text(course_name)
        .show_ui(ui, |ui| {
            for course in COURSES {
                if ui.selectable_value(&mut settings.course, course.name.to_string(), course.name).changed() {
                    course_changed = true;
                }
            }
        });
    let kind = settings.course().kind;
    let marks = settings.course().marks;
    let side_runs = settings.supports_side_runs();
    let return_methods: Vec<String> = settings.course().returns.iter().map(|method| (*method).to_string()).collect();
    let fairy_return = settings.return_method.starts_with("Fairy ring (");
    ui.add_space(8.0);
    if kind == Kind::Wilderness {
        ui.label("Wilderness agility is dangerous. The script does not fight or escape other players. Do not risk what you are not willing to lose. Start inside the course, with food, a looting bag, and Entity Hider hiding players and NPCs.");
        if ui.link("Agility friends chat guide").clicked() {
            chrome::open_url("https://oldschool.runescape.wiki/w/Guide:Agility_FC");
        }
        ui.checkbox(&mut settings.wilderness_ack, "I have read the Wilderness warning");
        ui.checkbox(&mut settings.disable_hop, "Disable world hopping");
        ui.checkbox(&mut settings.disable_breaks, "Disable breaks");
        ui.checkbox(&mut settings.disable_sleeps, "Disable sleeps");
        number_row(ui, "Eat at HP percent", &mut settings.eat_at);
        ui.checkbox(&mut settings.overheal, "Overheal with anglerfish");
        number_row(ui, "Stop at tickets (-1 = never)", &mut settings.stop_tickets);
    }
    if kind == Kind::Wyrm {
        ComboBox::from_id_salt("agility-wyrm")
            .width(200.0)
            .selected_text(&settings.wyrm_route)
            .show_ui(ui, |ui| {
                for route in ["Automatic", "Basic", "Advanced"] {
                    ui.selectable_value(&mut settings.wyrm_route, route.to_string(), route);
                }
            });
        ui.label("Automatic uses the advanced route at Agility 62+.");
        if fairy_return {
            ui.checkbox(&mut settings.lumby_elite, "Lumbridge elite");
        }
        number_row(ui, "Stop at termites (-1 = never)", &mut settings.stop_termites);
        number_row(ui, "Stop at blessed bone shards (-1 = never)", &mut settings.stop_shards);
    }
    if marks {
        number_row(ui, "Stop at marks (-1 = never)", &mut settings.stop_marks);
    }
    if side_runs {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.farm, "Farm runs");
            crate::ui::help_tip(ui, "Pause the course for a farm run when the timer is due, then come back.");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.birdhouse, "Birdhouse runs");
            crate::ui::help_tip(ui, "Pause the course for a birdhouse run when the timer is due, then come back.");
        });
        if settings.farm || settings.birdhouse {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Return method").strong());
                crate::ui::help_tip(ui, "How to get back to the course after farm or birdhouse runs.");
            });
            ComboBox::from_id_salt("agility-return")
                .width(280.0)
                .selected_text(&settings.return_method)
                .show_ui(ui, |ui| {
                    for method in &return_methods {
                        ui.selectable_value(&mut settings.return_method, method.clone(), method);
                    }
                });
        }
    }
    number_row(ui, "Max time minutes (0 = unlimited)", &mut settings.max_time);
    ui.add_space(8.0);
    ui.label(RichText::new("Required").strong());
    ui.label(settings.required_text());
    if course_changed {
        settings.on_course_changed();
    }
}

pub fn show(ctx: &egui::Context, setup: &mut Setup) -> SetupEvent {
    let mut event = SetupEvent::None;
    let mut course_changed = false;
    egui::Window::new("AIO Agility")
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(480.0);
            ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                ui.label(RichText::new(&setup.account_label).strong().size(16.0));
                if setup.account_key == "NoUserNameSelected" {
                    ui.label("Log in on this client first. Settings are saved to that account.");
                } else {
                    ui.label("Settings are saved for this account, then the script starts.");
                }
                ui.add_space(8.0);
                let course_name = setup.settings.course.clone();
                ComboBox::from_id_salt("agility-course")
                    .width(320.0)
                    .selected_text(&course_name)
                    .show_ui(ui, |ui| {
                        for course in COURSES {
                            if ui.selectable_value(&mut setup.settings.course, course.name.to_string(), course.name).changed() {
                                course_changed = true;
                            }
                        }
                    });
                let kind = setup.settings.course().kind;
                let marks = setup.settings.course().marks;
                let side_runs = setup.settings.supports_side_runs();
                let return_methods: Vec<String> = setup.settings.course().returns.iter().map(|method| (*method).to_string()).collect();
                let fairy_return = setup.settings.return_method.starts_with("Fairy ring (");
                ui.add_space(8.0);
                if kind == Kind::Wilderness {
                    ui.label("Wilderness agility is dangerous. The script does not fight or escape other players. Do not risk what you are not willing to lose. Start inside the course, with food, a looting bag, and Entity Hider hiding players and NPCs.");
                    if ui.link("Agility friends chat guide").clicked() {
                        chrome::open_url("https://oldschool.runescape.wiki/w/Guide:Agility_FC");
                    }
                    ui.checkbox(&mut setup.settings.wilderness_ack, "I have read the Wilderness warning");
                    ui.checkbox(&mut setup.settings.disable_hop, "Disable world hopping");
                    ui.checkbox(&mut setup.settings.disable_breaks, "Disable breaks");
                    ui.checkbox(&mut setup.settings.disable_sleeps, "Disable sleeps");
                    number_row(ui, "Eat at HP percent", &mut setup.settings.eat_at);
                    ui.checkbox(&mut setup.settings.overheal, "Overheal with anglerfish");
                    number_row(ui, "Stop at tickets (-1 = never)", &mut setup.settings.stop_tickets);
                }
                if kind == Kind::Wyrm {
                    ComboBox::from_id_salt("agility-wyrm")
                        .width(200.0)
                        .selected_text(&setup.settings.wyrm_route)
                        .show_ui(ui, |ui| {
                            for route in ["Automatic", "Basic", "Advanced"] {
                                ui.selectable_value(&mut setup.settings.wyrm_route, route.to_string(), route);
                            }
                        });
                    ui.label("Automatic uses the advanced route at Agility 62+.");
                    if fairy_return {
                        ui.checkbox(&mut setup.settings.lumby_elite, "Lumbridge elite");
                    }
                    number_row(ui, "Stop at termites (-1 = never)", &mut setup.settings.stop_termites);
                    number_row(ui, "Stop at blessed bone shards (-1 = never)", &mut setup.settings.stop_shards);
                }
                if marks {
                    number_row(ui, "Stop at marks (-1 = never)", &mut setup.settings.stop_marks);
                }
                if side_runs {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut setup.settings.farm, "Farm runs");
                        crate::ui::help_tip(ui, "Pause the course for a farm run when the timer is due, then come back.");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut setup.settings.birdhouse, "Birdhouse runs");
                        crate::ui::help_tip(ui, "Pause the course for a birdhouse run when the timer is due, then come back.");
                    });
                    if setup.settings.farm || setup.settings.birdhouse {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Return method").strong());
                            crate::ui::help_tip(ui, "How to get back to the course after farm or birdhouse runs.");
                        });
                        ComboBox::from_id_salt("agility-return")
                            .width(280.0)
                            .selected_text(&setup.settings.return_method)
                            .show_ui(ui, |ui| {
                                for method in &return_methods {
                                    ui.selectable_value(&mut setup.settings.return_method, method.clone(), method);
                                }
                            });
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Required").strong());
                ui.label(setup.settings.required_text());
                if !setup.error.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&setup.error).color(egui::Color32::from_rgb(220, 120, 110)));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let logged_in = setup.account_key != "NoUserNameSelected";
                if ui.add_enabled(logged_in, Button::new("Start")).clicked() {
                    event = SetupEvent::Start;
                }
                if ui.button("Cancel").clicked() {
                    event = SetupEvent::Cancel;
                }
            });
        });
    if course_changed {
        setup.settings.on_course_changed();
    }
    event
}

fn number_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(80.0));
    });
}

fn section_name(account: &str) -> String {
    format!("{account}{SETTINGS_SUFFIX}")
}

fn bool_text(value: bool) -> String {
    if value { "true".into() } else { "false".into() }
}

fn int_text(value: &str) -> String {
    value.trim().parse::<i32>().unwrap_or(-1).to_string()
}

fn clamp_percent(value: &str) -> i32 {
    match value.trim().parse::<i32>() {
        Ok(n) if (1..=99).contains(&n) => n,
        _ => 75,
    }
}

fn push_unique(lines: &mut Vec<String>, item: &str) {
    if !lines.iter().any(|line| line.eq_ignore_ascii_case(item)) {
        lines.push(item.to_string());
    }
}

fn return_item(method: &str) -> Option<String> {
    let method = method.trim();
    if method.is_empty() || method.eq_ignore_ascii_case("(none)") {
        return None;
    }
    let named = match method.to_ascii_lowercase().as_str() {
        "amulet of glory" => "Amulet of glory",
        "ring of dueling" => "Ring of dueling",
        "ring of elements" => "Ring of the elements",
        "ring of wealth" => "Ring of wealth",
        "varrock teleport" => "Varrock Teleport (3 air, 1 fire, 1 law)",
        "varrock tablet" => "Varrock teleport tablet",
        "falador teleport" => "Falador Teleport (3 air, 1 water, 1 law)",
        "camelot teleport" => "Camelot Teleport (5 air, 1 law)",
        "ardougne teleport" => "Ardougne Teleport (2 law, 2 water)",
        "ardougne cloak" => "Ardougne cloak",
        "enchanted lyre" => "Enchanted lyre",
        "colossal wyrm teleport scroll" => "Colossal wyrm teleport scroll",
        "quetzal whistle" => "Perfected quetzal whistle",
        "civitas teleport" => "Civitas illa Fortis Teleport (2 law, 1 fire, 1 earth)",
        "salve graveyard teleport" => "Salve Graveyard Teleport (Arceuus spellbook)",
        "salve graveyard tablet" => "Salve graveyard teleport",
        "fenkenstrain teleport" => "Fenkenstrain's Castle Teleport (Arceuus spellbook)",
        "fenkenstrain tablet" => "Fenkenstrain's castle teleport",
        "teleport crystal" => "Teleport crystal",
        "eternal teleport crystal" => "Eternal teleport crystal",
        "construction cape" => "Construct. cape",
        "construction cape trimmed" => "Construct. cape(t)",
        "house teleport tab" => "Teleport to house",
        "house teleport runes" => "House Teleport (5 law, 5 earth, 5 air)",
        _ if method.to_ascii_lowercase().starts_with("fairy ring (") => {
            "Ardougne cloak, and Dramen or Lunar staff unless Lumbridge elite is done"
        }
        _ => method,
    };
    Some(named.to_string())
}

pub(crate) fn ini_section(text: &str, section: &str) -> String {
    let header = format!("[{section}]");
    let mut inside = false;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if inside {
                break;
            }
            inside = trimmed == header;
            continue;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub(crate) fn ini_get(section: &str, key: &str) -> Option<String> {
    for line in section.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim().to_string());
        }
    }
    None
}

pub(crate) fn ini_bool(section: &str, key: &str, default: bool) -> bool {
    match ini_get(section, key).as_deref() {
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => true,
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => false,
        _ => default,
    }
}

pub(crate) fn write_section(existing: &str, section: &str, pairs: &[(&str, String)]) -> String {
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let header = format!("[{section}]");
    let mut lines: Vec<String> = existing.split('\n').map(|line| line.trim_end_matches('\r').to_string()).collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let start = lines.iter().position(|line| line.trim() == header);
    let mut body: Vec<String> = pairs.iter().map(|(key, value)| format!("{key}={value}")).collect();
    if let Some(start) = start {
        let end = lines
            .iter()
            .skip(start + 1)
            .position(|line| {
                let trimmed = line.trim();
                trimmed.starts_with('[') && trimmed.ends_with(']')
            })
            .map(|offset| start + 1 + offset)
            .unwrap_or(lines.len());
        for line in &lines[start + 1..end] {
            let Some((name, _)) = line.split_once('=') else {
                continue;
            };
            if pairs.iter().any(|(key, _)| name.trim().eq_ignore_ascii_case(key)) {
                continue;
            }
            body.push(line.clone());
        }
        let mut next = Vec::new();
        next.extend(lines[..start].iter().cloned());
        next.push(header);
        next.extend(body);
        next.extend(lines[end..].iter().cloned());
        lines = next;
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(|line| line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.extend(body);
    }
    let mut text = lines.join(newline);
    if !text.ends_with(newline) {
        text.push_str(newline);
    }
    text
}
