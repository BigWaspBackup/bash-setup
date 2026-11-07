//! Setup pages for premium scripts. Each page writes the same settings the
//! Simba form writes. Opening a script in the editor still shows that form.

use std::collections::HashMap;
use std::path::Path;

use egui::{ComboBox, RichText, Ui};

use crate::agility::{ini_get, ini_section, write_section};
use crate::shops::SHOPS;
use crate::skillcatalog::{self, Place};

pub fn supports(name: &str) -> bool {
    spec_for(name).is_some()
}

fn stored<'a>(key: &'a str, values: &'a HashMap<String, String>) -> Option<(&'a str, String)> {
    values.get(key).map(|value| (key, value.clone()))
}

pub fn load(configs: &Path, script: &str, account: &str) -> HashMap<String, String> {
    let Some(spec) = spec_for(script) else {
        return HashMap::new();
    };
    let mut values = HashMap::new();
    match spec.file {
        Some(file) => {
            let text = std::fs::read_to_string(configs.join(file)).unwrap_or_default();
            let account_section = format!("{account}{}", spec.suffix);
            for row in spec.rows {
                let section_name = if spec.suffix.is_empty() {
                    format!("{account} {}", row.section)
                } else {
                    account_section.clone()
                };
                let section = ini_section(&text, &section_name);
                if let Some(value) = ini_get(&section, row.key) {
                    values.insert(row.key.to_string(), value);
                } else if spec.suffix.is_empty() {
                    let shared = ini_section(&text, row.section);
                    if let Some(value) = ini_get(&shared, row.key) {
                        values.insert(row.key.to_string(), value);
                    }
                }
            }
        }
        None => {
            let text = std::fs::read_to_string(configs.join("BASettings.ini")).unwrap_or_default();
            let section = ini_section(&text, &format!("{account}{}", spec.suffix));
            for key in spec.keys {
                if let Some(value) = ini_get(&section, key) {
                    values.insert((*key).to_string(), value);
                }
            }
            if spec.kind == Kind::Salvage {
                let boat = ini_section(&text, &format!("{account} Boat Setup"));
                for key in crate::salvage::BOAT_KEYS {
                    if let Some(value) = ini_get(&boat, key) {
                        values.insert((*key).to_string(), value);
                    }
                }
            }
        }
    }
    values
}

pub fn save(configs: &Path, script: &str, account: &str, values: &HashMap<String, String>) -> Result<(), String> {
    let Some(spec) = spec_for(script) else {
        return Ok(());
    };
    if spec.keys.is_empty() && spec.rows.is_empty() {
        return Ok(());
    }
    if let Some(file) = spec.file {
        let path = configs.join(file);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut next = existing;
        if spec.suffix.is_empty() {
            let mut sections: Vec<&str> = spec.rows.iter().map(|row| row.section).collect();
            sections.sort_unstable();
            sections.dedup();
            for section in sections {
                let pairs: Vec<(&str, String)> = spec
                    .rows
                    .iter()
                    .filter(|row| row.section == section)
                    .filter_map(|row| stored(row.key, values))
                    .collect();
                if pairs.is_empty() {
                    continue;
                }
                next = write_section(&next, &format!("{account} {section}"), &pairs);
            }
        } else {
            let pairs: Vec<(&str, String)> = spec.rows.iter().filter_map(|row| stored(row.key, values)).collect();
            next = write_section(&next, &format!("{account}{}", spec.suffix), &pairs);
        }
        return std::fs::write(path, next).map_err(|error| format!("Could not save the script settings: {error}"));
    }
    let path = configs.join("BASettings.ini");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let pairs: Vec<(&str, String)> = spec.keys.iter().filter_map(|key| stored(key, values)).collect();
    let mut next = write_section(&existing, &format!("{account}{}", spec.suffix), &pairs);
    if spec.kind == Kind::Salvage {
        let boat_pairs: Vec<(&str, String)> = crate::salvage::BOAT_KEYS.iter().filter_map(|key| stored(key, values)).collect();
        next = write_section(&next, &format!("{account} Boat Setup"), &boat_pairs);
    }
    std::fs::write(path, next).map_err(|error| format!("Could not save the script settings: {error}"))
}

pub fn validate(script: &str, values: &HashMap<String, String>) -> Result<(), String> {
    let Some(spec) = spec_for(script) else {
        return Ok(());
    };
    match spec.kind {
        Kind::Fish | Kind::Wood | Kind::Cook | Kind::Anvil | Kind::Shop => {
            if text(values, "Location").is_empty() {
                return Err("Choose a location.".into());
            }
        }
        Kind::Mine => {
            if text(values, "Ore").is_empty() {
                return Err("Choose an ore.".into());
            }
        }
        Kind::Bank => {
            let mode = text(values, "Mode");
            match mode.as_str() {
                "2" if text(values, "PrimaryItem").trim().is_empty() || text(values, "SecondaryItem").trim().is_empty() => {
                    return Err("Enter the primary and secondary items.".into());
                }
                "3" | "4" if text(values, "ToolItem").trim().is_empty() || text(values, "MaterialItem").trim().is_empty() => {
                    return Err("Enter the tool and the material.".into());
                }
                "6" if values.get("AlchTab").and_then(|value| value.parse::<i32>().ok()).unwrap_or(0) < 1 => {
                    return Err("Choose Inventory or a bank tab for high alching.".into());
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn show(ui: &mut Ui, script: &str, values: &mut HashMap<String, String>) {
    let Some(spec) = spec_for(script) else {
        return;
    };
    ui.label(RichText::new(script).strong());
    ui.add_space(8.0);
    match spec.kind {
        Kind::Fish => fish_page(ui, values),
        Kind::Mine => mine_page(ui, values),
        Kind::Wood => wood_page(ui, values),
        Kind::Cook => cook_page(ui, values),
        Kind::Anvil => anvil_page(ui, values),
        Kind::Shop => shop_page(ui, values),
        Kind::Smelt => smelt_page(ui, values),
        Kind::Bank => bank_page(ui, values),
        Kind::Lunar => lunar_page(ui, values),
        Kind::Salvage => crate::salvage::show(ui, values),
        Kind::Rows => {
            if spec.rows.is_empty() {
                ui.label("Accounts, antiban, and webhooks are on the other tabs.");
            } else {
                rows_page(ui, spec.rows, values);
            }
        }
    }
}

struct Spec {
    names: &'static [&'static str],
    suffix: &'static str,
    file: Option<&'static str>,
    keys: &'static [&'static str],
    rows: &'static [Row],
    kind: Kind,
}

struct Row {
    key: &'static str,
    label: &'static str,
    section: &'static str,
    tip: &'static str,
    kind: RowKind,
    /// Hidden rows are still saved. The Antiban tab draws the activity chances.
    visible: bool,
}

enum RowKind {
    Choice(&'static [&'static str]),
    /// Like `Choice`, but an empty saved value starts on this index.
    ChoiceAt(&'static [&'static str], usize),
    /// Stores the option index.
    Index(&'static [&'static str], usize),
    Check(bool),
    Number(&'static str),
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fish,
    Mine,
    Wood,
    Cook,
    Anvil,
    Shop,
    Smelt,
    Bank,
    Lunar,
    Salvage,
    Rows,
}

fn spec_for(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.names.iter().any(|item| item.eq_ignore_ascii_case(name)))
}

fn text(values: &HashMap<String, String>, key: &str) -> String {
    values.get(key).cloned().unwrap_or_default()
}

fn flag(values: &HashMap<String, String>, key: &str, default: bool) -> bool {
    match values.get(key).map(|value| value.as_str()) {
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => true,
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => false,
        _ => default,
    }
}

/// Farm / birdhouse checkboxes on the Script tab. Either birdhouse key may be used.
pub fn side_runs_enabled(values: &HashMap<String, String>) -> (bool, bool) {
    (
        flag(values, "EnableFarmRuns", false),
        flag(values, "EnableBirdhouseRuns", false) || flag(values, "EnableBirdhouse", false),
    )
}

fn field_label(ui: &mut Ui, label: &str, tip: &str, strong: bool) {
    ui.horizontal(|ui| {
        if strong {
            ui.label(RichText::new(label).strong());
        } else {
            ui.label(label);
        }
        crate::ui::help_tip(ui, tip);
    });
}

fn choose(ui: &mut Ui, salt: &str, label: &str, current: &mut String, options: &[&str]) {
    choose_tip(ui, salt, label, "", current, options);
}

fn choose_tip(ui: &mut Ui, salt: &str, label: &str, tip: &str, current: &mut String, options: &[&str]) {
    if options.is_empty() {
        return;
    }
    if let Some(option) = options.iter().find(|option| option.eq_ignore_ascii_case(current)) {
        *current = (*option).to_string();
    } else if current.trim().is_empty() {
        *current = options[0].to_string();
    }
    field_label(ui, label, tip, true);
    let mut choices: Vec<String> = options.iter().map(|option| (*option).to_string()).collect();
    if !choices.iter().any(|option| option == current) {
        choices.insert(0, current.clone());
    }
    let shown = current.clone();
    ComboBox::from_id_salt(salt)
        .width(360.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for option in &choices {
                ui.selectable_value(current, option.clone(), option);
            }
        });
}

fn choose_index(ui: &mut Ui, salt: &str, label: &str, values: &mut HashMap<String, String>, key: &str, options: &[&str], default: usize) {
    choose_index_tip(ui, salt, label, "", values, key, options, default);
}

/// Shows `options` A to Z. The saved value stays the original index, so existing profiles keep the same furnace.
fn choose_sorted_index_tip(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    tip: &str,
    values: &mut HashMap<String, String>,
    key: &str,
    options: &[&str],
    default: usize,
) {
    if options.is_empty() {
        return;
    }
    let mut index = values.get(key).and_then(|value| value.parse().ok()).unwrap_or(default);
    if index >= options.len() {
        index = default.min(options.len() - 1);
    }
    let mut order: Vec<usize> = (0..options.len()).collect();
    order.sort_by(|&left, &right| options[left].to_ascii_lowercase().cmp(&options[right].to_ascii_lowercase()));
    field_label(ui, label, tip, true);
    let shown = options[index].to_string();
    ComboBox::from_id_salt(salt)
        .width(360.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for item in order {
                ui.selectable_value(&mut index, item, options[item]);
            }
        });
    values.insert(key.to_string(), index.to_string());
}

fn choose_index_tip(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    tip: &str,
    values: &mut HashMap<String, String>,
    key: &str,
    options: &[&str],
    default: usize,
) {
    if options.is_empty() {
        return;
    }
    let mut index = values.get(key).and_then(|value| value.parse().ok()).unwrap_or(default);
    if index >= options.len() {
        index = 0;
    }
    field_label(ui, label, tip, true);
    let shown = options[index].to_string();
    ComboBox::from_id_salt(salt)
        .width(360.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (item, option) in options.iter().enumerate() {
                ui.selectable_value(&mut index, item, *option);
            }
        });
    values.insert(key.to_string(), index.to_string());
}

fn checkbox_tip(ui: &mut Ui, label: &str, tip: &str, values: &mut HashMap<String, String>, key: &str, default: bool) -> bool {
    let mut on = flag(values, key, default);
    ui.horizontal(|ui| {
        ui.checkbox(&mut on, label);
        crate::ui::help_tip(ui, tip);
    });
    values.insert(key.to_string(), if on { "true" } else { "false" }.to_string());
    on
}

fn number_box_tip(ui: &mut Ui, label: &str, tip: &str, values: &mut HashMap<String, String>, key: &str, default: &str) {
    let mut value = text(values, key);
    if value.is_empty() {
        value = default.to_string();
    }
    field_label(ui, label, tip, false);
    ui.text_edit_singleline(&mut value);
    values.insert(key.to_string(), value);
}

const TIP_FARM: &str = "Pause the main activity for a farm run when the timer is due, then come back.";
const TIP_BIRD: &str = "Pause the main activity for a birdhouse run when the timer is due, then come back.";
const TIP_RETURN: &str = "How to get back to the main activity after farm or birdhouse runs.";

fn side_runs(ui: &mut Ui, values: &mut HashMap<String, String>, bird_key: &str, returns: &[&str]) {
    let farm = checkbox_tip(ui, "Farm runs", TIP_FARM, values, "EnableFarmRuns", false);
    let bird = checkbox_tip(ui, "Birdhouse runs", TIP_BIRD, values, bird_key, false);
    if (farm || bird) && !returns.is_empty() {
        ui.add_space(6.0);
        let mut method = text(values, "ReturnMethod");
        if method.is_empty() {
            method = text(values, "BankReturnMethod");
        }
        if let Some(option) = returns.iter().find(|option| option.eq_ignore_ascii_case(&method)) {
            method = (*option).to_string();
        } else {
            method = returns[0].to_string();
        }
        if returns.len() == 1 {
            locked_choice(ui, "script-return", "Return method", TIP_RETURN, &method);
        } else {
            choose_tip(ui, "script-return", "Return method", TIP_RETURN, &mut method, returns);
        }
        values.insert("ReturnMethod".into(), method.clone());
        values.insert("BankReturnMethod".into(), method.clone());
        values.insert("FarmReturnMethod".into(), method.clone());
        values.insert("BirdhouseReturnMethod".into(), method);
    }
}

fn place<'a>(list: &'a [Place], name: &str) -> Option<&'a Place> {
    list.iter().find(|place| place.name == name)
}

fn fish_side(name: &str) -> bool {
    matches!(
        name,
        "Fishing Guild"
            | "Catherby"
            | "Draynor Village"
            | "Al Kharid"
            | "Barbarian Village"
            | "Port Piscarilius East"
            | "Port Piscarilius West"
            | "Hosidius East"
            | "Hosidius Center"
            | "Karambwan (DKP)"
            | "Otto's Grotto"
            | "Burgh de Rott"
            | "Piscatoris"
    )
}

fn fish_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = skillcatalog::FISH.iter().map(|place| place.name).collect();
    let mut location = text(values, "Location");
    choose(ui, "fish-location", "Location", &mut location, &names);
    values.insert("Location".into(), location.clone());
    let Some(spot) = place(skillcatalog::FISH, &location) else {
        return;
    };
    let mut method = text(values, "Method");
    choose_tip(ui, "fish-method", "Method", "Fishing method used at this spot.", &mut method, spot.methods);
    values.insert("Method".into(), method);
    let mut mode = text(values, "Mode");
    choose_tip(
        ui,
        "fish-mode",
        "Mode",
        "Powermine-style options keep fish in the inventory or drop them. Bank modes bank the catch.",
        &mut mode,
        spot.modes,
    );
    values.insert("Mode".into(), mode);
    if fish_side(spot.name) {
        side_runs(ui, values, "EnableBirdhouseRuns", spot.returns);
    }
}

fn mine_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let mut ore = text(values, "Ore");
    if ore.is_empty() {
        ore = "Iron".into();
    }
    choose(ui, "mine-ore", "Ore", &mut ore, skillcatalog::ORES);
    values.insert("Ore".into(), ore.clone());
    let mut sites = vec!["Auto-detect (stand at the rocks)"];
    if let Some((_, names)) = skillcatalog::MINE_SITES.iter().find(|(name, _)| *name == ore) {
        sites.extend(names.iter().copied());
    }
    let mut location = text(values, "Location");
    if location.is_empty() {
        location = "Auto-detect (stand at the rocks)".into();
    }
    choose(ui, "mine-location", "Location", &mut location, &sites);
    values.insert("Location".into(), location.clone());
    let mut mode = text(values, "Mode");
    choose_tip(
        ui,
        "mine-mode",
        "Mode",
        "Powermine drops ore as it is mined. Bank keeps the ore and banks when the inventory is full.",
        &mut mode,
        &["Powermine", "Bank"],
    );
    values.insert("Mode".into(), mode);
    let returns = skillcatalog::MINES
        .iter()
        .find(|(name, _)| *name == location)
        .map(|(_, methods)| *methods)
        .unwrap_or(&[]);
    if !returns.is_empty() {
        side_runs(ui, values, "EnableBirdhouseRuns", returns);
    }
}

fn snap_choice(current: &str, options: &[&str]) -> String {
    options
        .iter()
        .find(|option| option.eq_ignore_ascii_case(current))
        .map(|option| (*option).to_string())
        .or_else(|| options.first().map(|option| (*option).to_string()))
        .unwrap_or_default()
}

fn set_flag(values: &mut HashMap<String, String>, key: &str, on: bool) {
    values.insert(key.to_string(), if on { "true" } else { "false" }.to_string());
}

fn store_return(values: &mut HashMap<String, String>, method: &str) {
    for key in ["ReturnMethod", "BankReturnMethod", "FarmReturnMethod", "BirdhouseReturnMethod"] {
        values.insert(key.to_string(), method.to_string());
    }
}

/// Fletch list for one tree. Matches AIO Woodcutting AddTreeType, not the location's combined list.
fn wood_fletch(tree: &str) -> &'static [&'static str] {
    match tree {
        "Regular" => &["Arrow shaft", "Shortbow", "Longbow", "Wooden stock"],
        "Oak" => &["Arrow shaft", "Oak shortbow", "Oak longbow", "Oak stock", "Oak shield"],
        "Willow" => &["Arrow shaft", "Willow shortbow", "Willow longbow", "Willow stock", "Willow shield"],
        "Teak" => &["Teak stock"],
        "Maple" => &["Arrow shaft", "Maple shortbow", "Maple longbow", "Maple stock", "Maple shield"],
        "Mahogany" => &["Mahogany stock"],
        "Yew" => &["Arrow shaft", "Yew shortbow", "Yew longbow", "Yew stock", "Yew shield"],
        "Blisterwood" => &["Blisterwood stake"],
        "Magic" => &["Arrow shaft", "Magic shortbow", "Magic longbow", "Magic stock", "Magic shield"],
        "Redwood" => &["Arrow shaft", "Redwood shield", "Redwood hiking staff"],
        _ => &[],
    }
}

fn wood_modes<'a>(spot: &'a Place, tree: &str) -> Vec<&'a str> {
    let mut modes: Vec<&str> = spot.modes.to_vec();
    if wood_fletch(tree).is_empty() {
        modes.retain(|mode| !mode.to_ascii_lowercase().contains("fletch"));
    }
    if modes.is_empty() {
        modes.push("Powercut");
    }
    modes
}

fn wood_player_boost(location: &str, tree: &str) -> bool {
    if tree.eq_ignore_ascii_case("Regular") {
        return false;
    }
    !matches!(location, "Woodcutting Guild" | "Darkmeyer" | "Fossil Island")
}

fn wood_forestry(location: &str, tree: &str) -> bool {
    if matches!(tree, "Regular" | "Blisterwood" | "Arctic pine" | "Camphor" | "Rosewood") {
        return false;
    }
    !matches!(location, "Woodcutting Guild" | "Fossil Island")
}

fn wood_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let mut names: Vec<&str> = skillcatalog::WOOD.iter().map(|place| place.name).collect();
    names.sort_by(|left, right| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()));
    let mut location = text(values, "Location");
    if location.eq_ignore_ascii_case("Ironwood") {
        location = "Sunbleak Island".to_string();
    }
    location = snap_choice(&location, &names);
    choose(ui, "wc-location", "Location", &mut location, &names);
    values.insert("Location".into(), location.clone());
    let Some(spot) = place(skillcatalog::WOOD, &location) else {
        return;
    };
    let mut tree = snap_choice(&text(values, "Tree"), spot.methods);
    choose(ui, "wc-tree", "Tree", &mut tree, spot.methods);
    values.insert("Tree".into(), tree.clone());
    let modes = wood_modes(spot, &tree);
    let mut mode = snap_choice(&text(values, "Mode"), &modes);
    choose_tip(
        ui,
        "wc-mode",
        "Mode",
        "Bank keeps logs. Drop or burn options clear the inventory without banking. Fletch options cut and fletch on the spot.",
        &mut mode,
        &modes,
    );
    values.insert("Mode".into(), mode.clone());
    let products = wood_fletch(&tree);
    if mode.contains("Fletch") && !products.is_empty() {
        let mut item = snap_choice(&text(values, "FletchItem"), products);
        choose_tip(ui, "wc-fletch", "Fletch", "Item to fletch from the logs before banking or dropping.", &mut item, products);
        values.insert("FletchItem".into(), item);
    } else if products.is_empty() {
        values.insert("FletchItem".into(), String::new());
    } else {
        values.insert("FletchItem".into(), snap_choice(&text(values, "FletchItem"), products));
    }
    let stay_allowed = wood_player_boost(&location, &tree);
    if stay_allowed {
        checkbox_tip(
            ui,
            "Stay near other players",
            "Prefer chopping near other players when the location allows it.",
            values,
            "StayNearPlayers",
            false,
        );
    } else {
        set_flag(values, "StayNearPlayers", false);
    }
    if wood_forestry(&location, &tree) {
        ui.add_space(8.0);
        ui.label(RichText::new("Forestry events").strong());
        checkbox_tip(
            ui,
            "Prefer forestry worlds",
            "Hop only on members Forestry worlds. Timed hops, break hops, and every other hop use that list.",
            values,
            "PreferForestryWorlds",
            false,
        );
        checkbox_tip(ui, "Rising Roots", "Chop anima-infused roots when the green tile uptext is fused.", values, "ForestryRisingRoots", true);
        checkbox_tip(ui, "Beehive", "Build unfinished beehives from logs in the inventory.", values, "ForestryBeehive", true);
        checkbox_tip(ui, "Enchantment Ritual", "Stand on the green ritual tile.", values, "ForestryRitual", true);
        checkbox_tip(ui, "Woodcutting Leprechaun", "Stand on the rainbow and keep the luck buff.", values, "ForestryLeprechaun", true);
        checkbox_tip(ui, "Poachers", "Disarm fox traps until they stop spawning.", values, "ForestryPoachers", true);
    } else {
        set_flag(values, "PreferForestryWorlds", false);
        for key in ["ForestryRisingRoots", "ForestryBeehive", "ForestryRitual", "ForestryLeprechaun", "ForestryPoachers"] {
            set_flag(values, key, false);
        }
    }
    if location == "Isle of Souls" && tree == "Teak" {
        checkbox_tip(
            ui,
            "Only the two close teaks",
            "Isle of Souls teak: chop the two trees that stand next to each other.",
            values,
            "SoulsCloseTeaks",
            false,
        );
    } else {
        set_flag(values, "SoulsCloseTeaks", false);
    }
    if spot.side && !spot.returns.is_empty() {
        side_runs(ui, values, "EnableBirdhouseRuns", spot.returns);
        let mut method = text(values, "ReturnMethod");
        if method.is_empty() {
            method = text(values, "BankReturnMethod");
        }
        method = snap_choice(&method, spot.returns);
        store_return(values, &method);
    } else {
        set_flag(values, "EnableFarmRuns", false);
        set_flag(values, "EnableBirdhouseRuns", false);
        store_return(values, "(none)");
    }
}

fn cook_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = skillcatalog::COOKS.iter().map(|place| place.name).collect();
    let mut location = text(values, "Location");
    choose(ui, "cook-location", "Location", &mut location, &names);
    values.insert("Location".into(), location.clone());
    let spot = place(skillcatalog::COOKS, &location);
    choose_index_tip(
        ui,
        "cook-mode",
        "Cook",
        "Specific food cooks one raw item. Bank tab cooks everything in that tab. Jugs of wine makes wine.",
        values,
        "CookMode",
        &["Specific food", "Bank tab", "Jugs of wine"],
        0,
    );
    let mode = text(values, "CookMode");
    if mode == "1" {
        let mut tab = values.get("CookTab").and_then(|value| value.parse::<usize>().ok()).unwrap_or(1);
        if !(1..=9).contains(&tab) {
            tab = 1;
        }
        ui.label(RichText::new("Bank tab").strong());
        let shown = format!("Tab {tab}");
        ComboBox::from_id_salt("cook-tab")
            .width(360.0)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for item in 1..=9 {
                    ui.selectable_value(&mut tab, item, format!("Tab {item}"));
                }
            });
        values.insert("CookTab".into(), tab.to_string());
    } else if mode != "2" {
        let fire = spot.is_some_and(|place| place.fire);
        let foods: Vec<&str> = skillcatalog::FOODS
            .iter()
            .filter(|(_, food_fire)| !fire || *food_fire)
            .map(|(name, _)| *name)
            .collect();
        let mut food = text(values, "Food");
        if food.is_empty() {
            food = "Lobster".into();
        }
        choose(ui, "cook-food", "Food", &mut food, &foods);
        values.insert("Food".into(), food.clone());
        if food == "Custom" {
            let mut custom = text(values, "CustomFood");
            ui.label("Raw item name");
            ui.text_edit_singleline(&mut custom);
            values.insert("CustomFood".into(), custom);
        }
    }
    if let Some(spot) = spot.filter(|place| place.side) {
        side_runs(ui, values, "EnableBirdhouse", spot.returns);
    }
}

fn anvil_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = skillcatalog::ANVILS.iter().map(|place| place.name).collect();
    let mut location = text(values, "Location");
    if location.is_empty() {
        location = "Varrock West".into();
    }
    choose(ui, "anvil-location", "Location", &mut location, &names);
    values.insert("Location".into(), location.clone());
    let mut metal = text(values, "Metal");
    choose(ui, "anvil-metal", "Metal", &mut metal, &["Bronze", "Iron", "Steel", "Mithril", "Adamant", "Rune"]);
    values.insert("Metal".into(), metal);
    let mut item = text(values, "Item");
    choose(
        ui,
        "anvil-item",
        "Item",
        &mut item,
        &[
            "Dagger", "Sword", "Scimitar", "Mace", "Longsword", "Battleaxe", "Warhammer", "2h sword", "Axe", "Platebody",
            "Platelegs", "Plateskirt", "Chainbody", "Kiteshield", "Sq shield", "Full helm", "Med helm", "Nails", "Arrowtips",
            "Dart tip", "Knife", "Javelin heads", "Bolts (unf)", "Claws", "Limbs", "Wire",
        ],
    );
    values.insert("Item".into(), item);
    if let Some(spot) = place(skillcatalog::ANVILS, &location).filter(|place| place.side) {
        side_runs(ui, values, "EnableBirdhouse", spot.returns);
    }
}

fn shop_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = SHOPS.iter().map(|shop| shop.name).collect();
    let mut location = text(values, "Location");
    choose(ui, "shop-location", "Shop", &mut location, &names);
    values.insert("Location".into(), location.clone());
    number_box_tip(
        ui,
        "Minimum stock when an item has none saved",
        "Default shop stock required before buying an item that has no Min value set below.",
        values,
        "DefaultMinStock",
        "0",
    );
    let Some(shop) = SHOPS.iter().find(|shop| shop.name == location) else {
        return;
    };
    let saved = parse_items(&text(values, "ItemsCsv"));
    ui.add_space(8.0);
    ui.label(RichText::new("Buy").strong());
    ui.label("Amount 0 skips that item. Minimum stock is how many the shop must have.");
    let mut next = Vec::new();
    for item in shop.items {
        let (qty, min) = saved.get(*item).cloned().unwrap_or_else(|| ("0".into(), text(values, "DefaultMinStock")));
        let mut qty = qty;
        let mut min = min;
        ui.horizontal(|ui| {
            ui.label(*item);
            ui.label("Amount");
            ui.add(egui::TextEdit::singleline(&mut qty).desired_width(60.0));
            ui.label("Min");
            ui.add(egui::TextEdit::singleline(&mut min).desired_width(60.0));
        });
        next.push(format!("{item}:{qty}:{min}"));
    }
    values.insert("ItemsCsv".into(), next.join("|"));
}

fn parse_items(csv: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for part in csv.split('|').filter(|part| !part.is_empty()) {
        let mut pieces = part.rsplitn(3, ':');
        let min = pieces.next().unwrap_or("0");
        let qty = pieces.next().unwrap_or("0");
        let name = pieces.next().unwrap_or("");
        if !name.is_empty() {
            out.insert(name.to_string(), (qty.to_string(), min.to_string()));
        }
    }
    out
}

const SMELT_FURNACES: &[&str] = &[
    "Grimstone Dungeon",
    "Shilo Village",
    "Prifddinas",
    "Edgeville",
    "Deepfin Point",
    "The Onyx Crest",
    "Neitiznot",
    "Mount Karuulm",
    "The Summer Shore",
    "Port Phasmatys",
    "Mor Ul Rek Inner",
    "Mor Ul Rek Outer",
    "Burgh de Rott",
    "Falador",
    "Zanaris",
    "East Ardougne",
];
const SMELT_METALS: &[&str] = &["Silver", "Gold"];

fn smelt_returns(furnace: &str) -> &'static [&'static str] {
    match furnace {
        "Edgeville" => &["Amulet of Glory"],
        "Deepfin Point" => &["Sailors' Amulet"],
        "Falador" => &["Tablet", "Spell"],
        _ => &[],
    }
}

fn smelt_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    choose_sorted_index_tip(ui, "smelt-furnace", "Furnace", "Which furnace to use.", values, "Furnace", SMELT_FURNACES, 3);
    let furnace = values.get("Furnace").and_then(|value| value.parse::<usize>().ok()).unwrap_or(3);
    let name = SMELT_FURNACES.get(furnace).copied().unwrap_or("");
    if name == "Grimstone Dungeon" {
        values.insert("SmeltMode".into(), "0".into());
        locked_choice(
            ui,
            "smelt-mode",
            "Activity",
            "Grimstone Dungeon only smelts cannonballs.",
            "Cannonballs",
        );
    } else {
        choose_index_tip(
            ui,
            "smelt-mode",
            "Activity",
            "Cannonballs smelts cannonballs. Bars / Glass smelts bars or molten glass. Jewelry makes jewellery at the furnace.",
            values,
            "SmeltMode",
            &["Cannonballs", "Bars / Glass", "Jewelry"],
            0,
        );
    }
    match text(values, "SmeltMode").as_str() {
        "1" if name != "Grimstone Dungeon" => choose_index(
            ui,
            "smelt-bar",
            "Bar",
            values,
            "BarProduct",
            &["Bronze", "Iron", "Silver", "Steel", "Gold", "Mithril", "Adamantite", "Runite", "Molten glass"],
            4,
        ),
        "2" if name != "Grimstone Dungeon" => {
            choose_index(
                ui,
                "smelt-jewel",
                "Jewelry",
                values,
                "JewelType",
                &["Tiara", "Holy symbol", "Unholy symbol", "Bolts", "Ring", "Necklace", "Bracelet", "Amulet"],
                4,
            );
            let jewel = values.get("JewelType").and_then(|value| value.parse::<usize>().ok()).unwrap_or(4);
            if jewel < 4 {
                values.insert("GemType".into(), "0".into());
            } else {
                choose_index(
                    ui,
                    "smelt-gem",
                    "Gem",
                    values,
                    "GemType",
                    &["None", "Opal", "Jade", "Red topaz", "Sapphire", "Emerald", "Ruby", "Diamond", "Dragonstone", "Onyx", "Zenyte"],
                    5,
                );
            }
            let gem = values.get("GemType").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
            if jewel == 0 {
                choose_index(ui, "smelt-metal", "Metal", values, "MetalType", SMELT_METALS, 1);
            } else {
                let metal = if (1..=3).contains(&gem) || jewel < 4 { 0 } else { 1 };
                values.insert("MetalType".into(), metal.to_string());
                locked_choice(ui, "smelt-metal", "Metal", "Set from the jewellery and gem, same as the script form.", SMELT_METALS[metal]);
            }
        }
        _ => choose_index(
            ui,
            "smelt-cannon",
            "Cannonballs",
            values,
            "CannonBallType",
            &["Bronze Cannonballs", "Iron Cannonballs", "Steel Cannonballs", "Mithril Cannonballs", "Adamant Cannonballs", "Rune Cannonballs"],
            2,
        ),
    }
    let returns = smelt_returns(name);
    if !returns.is_empty() {
        side_runs(ui, values, "EnableBirdhouse", returns);
        let mut method = text(values, "ReturnMethod");
        if method.is_empty() {
            method = text(values, "BankReturnMethod");
        }
        method = snap_choice(&method, returns);
        store_return(values, &method);
    } else {
        set_flag(values, "EnableFarmRuns", false);
        set_flag(values, "EnableBirdhouse", false);
        store_return(values, "");
    }
}

fn line_edit(ui: &mut Ui, label: &str, tip: &str, values: &mut HashMap<String, String>, key: &str) {
    let mut value = text(values, key);
    field_label(ui, label, tip, false);
    ui.text_edit_singleline(&mut value);
    values.insert(key.to_string(), value);
}

fn locked_choice(ui: &mut Ui, salt: &str, label: &str, tip: &str, shown: &str) {
    field_label(ui, label, tip, true);
    ui.add_enabled_ui(false, |ui| {
        ComboBox::from_id_salt(salt).width(360.0).selected_text(shown).show_ui(ui, |_| {});
    });
}

const BANK_LOCATIONS: &[&str] = &[
    "Al Kharid", "Ardougne", "Castle Wars", "Catherby", "Crafting Guild", "Draynor Village", "Edgeville", "Falador",
    "Grand Exchange", "Lunar Isle", "Mining Guild", "Prifddinas", "Seers Village", "Varrock", "Wintertodt", "Fossil Island",
    "Deepfin Point", "Blast Furnace", "Burgh De Rott", "Canifis", "Ferox Enclave", "Hosidius", "Lumbridge", "Mor Ul Rek",
    "Motherloade Mine", "Mount Karuulm", "Myths Guild", "Nardah", "Neitiznot", "Piscarilius", "Port Phasmatys", "Rogues Den",
    "Ruins Of Unkah", "Shilo Village", "The Onyx Crest", "The Summer Shore", "Warriors Guild", "Yanille", "Zanaris", "Wyrmscraig",
    "Cooks Guild", "Port Khazard",
];

/// Farm and birdhouse tabs stay hidden for high alch and for banks that cannot return.
pub fn bank_side_runs_allowed(values: &HashMap<String, String>) -> bool {
    if values.get("Mode").is_some_and(|mode| mode == "6") {
        return false;
    }
    let index = values.get("Bank").and_then(|value| value.parse::<usize>().ok()).unwrap_or(8);
    let name = BANK_LOCATIONS.get(index).copied().unwrap_or("");
    !bank_returns(name).is_empty()
}

/// Return methods for banks that can pause for farm or birdhouse runs.
/// Same list as the script's GetReturnMethods. Other banks have none.
fn bank_returns(bank: &str) -> &'static [&'static str] {
    match bank {
        "Ardougne" | "Catherby" | "Falador" | "Varrock" => &["Tablet", "Spell"],
        "Castle Wars" => &["Ring of Dueling", "Minigame Teleport"],
        "Crafting Guild" => &["Crafting Cape"],
        "Deepfin Point" => &["Sailors' Amulet"],
        "Grand Exchange" => &["Ring of Wealth", "Grand Exchange Teleport", "Varrock Teleport"],
        "Wintertodt" => &["Games Necklace"],
        _ => &[],
    }
}

fn bank_mode_hint(mode: &str) -> &'static str {
    match mode {
        "1" => "Identifies grimy herbs and opens bird nests found in the main bank tab.",
        "2" => "Uses the primary item on the secondary item. Names must match the game exactly.",
        "3" => "Uses the tool on the material. Leave Make text blank when there is no choice.",
        "4" => "Same as Processing, but the tool can be a stack that stays across trips.",
        "5" => "Casts the chosen enchant spell on matching jewellery in the main bank tab.",
        "6" => "High alch the inventory, or a bank tab. Farm and birdhouse runs stay off.",
        _ => "Cuts uncut gems from the main bank tab, one type per inventory.",
    }
}

fn bank_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    if values.get("Bank").and_then(|value| value.parse::<usize>().ok()).is_none() {
        values.insert("Bank".into(), "8".into());
    }
    choose_index_tip(
        ui,
        "bank-mode",
        "Activity",
        "What to do at the bank. Enchanting and high alching use the spellbook. The other modes process inventory items.",
        values,
        "Mode",
        &["Cutting gems", "Cleaning", "Combining", "Processing", "Processing stacks", "Enchanting", "High alching"],
        0,
    );
    let mode = text(values, "Mode");
    let alch_tab = values.get("AlchTab").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    let inventory_alch = mode == "6" && alch_tab == 1;
    if inventory_alch {
        locked_choice(
            ui,
            "bank-where-anywhere",
            "Bank",
            "Inventory high alch runs wherever you already are.",
            "Anywhere",
        );
    } else {
        choose_index_tip(ui, "bank-where", "Bank", "Bank to stand at while processing.", values, "Bank", BANK_LOCATIONS, 8);
    }
    let bank_index = values.get("Bank").and_then(|value| value.parse::<usize>().ok()).unwrap_or(8);
    let bank_name = BANK_LOCATIONS.get(bank_index).copied().unwrap_or("");

    match mode.as_str() {
        "2" => {
            line_edit(ui, "Primary item", "Exact primary item name. Example: Vial of water or Air orb.", values, "PrimaryItem");
            line_edit(
                ui,
                "Secondary item",
                "Exact secondary item name. Example: Guam leaf or Battlestaff.",
                values,
                "SecondaryItem",
            );
        }
        "3" | "4" => {
            line_edit(ui, "Tool", "Tool used on the material. Example: Chisel, Needle, or Sacred oil.", values, "ToolItem");
            line_edit(ui, "Material", "Material the tool is used on. Example: Soft clay or Magic logs.", values, "MaterialItem");
            line_edit(ui, "Make text", "Make-interface option. Leave blank when there is no choice.", values, "MakeText");
        }
        "5" => {
            choose_index(
                ui,
                "bank-spell",
                "Spell",
                values,
                "Spell",
                &["Opal & Sapphire", "Jade & Emerald", "Ruby & Red topaz", "Diamond", "Dragonstone"],
                0,
            );
            checkbox_tip(
                ui,
                "Fast enchanting",
                "Cast enchant spells as quickly as possible. Turn off if the client struggles to keep up.",
                values,
                "FastEnchanting",
                true,
            );
        }
        "6" => {
            choose_index(
                ui,
                "bank-alch",
                "Alch from",
                values,
                "AlchTab",
                &["Select", "Inventory", "Tab 1", "Tab 2", "Tab 3", "Tab 4", "Tab 5", "Tab 6", "Tab 7", "Tab 8", "Tab 9"],
                0,
            );
            if text(values, "AlchTab") == "0" {
                ui.label(RichText::new("Choose Inventory or a bank tab before starting.").weak());
            }
        }
        _ => {}
    }
    ui.add_space(6.0);
    ui.label(RichText::new(bank_mode_hint(&mode)).weak());

    if mode == "6" {
        return;
    }
    let returns = bank_returns(bank_name);
    if returns.is_empty() {
        values.insert("EnableFarmRuns".into(), "false".into());
        values.insert("EnableBirdhouse".into(), "false".into());
        return;
    }
    side_runs(ui, values, "EnableBirdhouse", returns);
}

fn lunar_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    const SPELLS: &[(&str, &[&str])] = &[
        ("Bake Pie", &["Berry Pie", "Meat Pie", "Mud Pie", "Apple Pie", "Garden Pie", "Fish Pie", "Admiral Pie", "Wild Pie", "Summer Pie"]),
        ("Humidify", &["Bowl", "Bucket", "Clay", "Cup", "Jug", "Vial", "Waterskin"]),
        ("Spin Flax", &["Bow String"]),
        ("Superglass Make", &["Soda Ash", "Seaweed", "Giant Seaweed", "Swamp Weed"]),
        ("Tan Leather", &["Cowhide", "Snake Hide", "Green Dragonhide", "Blue Dragonhide", "Red Dragonhide", "Black Dragonhide"]),
        (
            "String Jewellery",
            &[
                "Unstrung Symbol", "Unstrung Emblem", "Gold Amulet U", "Opal Amulet U", "Jade Amulet U", "Topaz Amulet U", "Sapphire Amulet U",
                "Emerald Amulet U", "Ruby Amulet U", "Diamond Amulet U", "Dragonstone Amulet U",
            ],
        ),
        ("Plank Make", &["Logs", "Oak Logs", "Teak Logs", "Mahogany Logs", "Camphor Logs", "Ironwood Logs", "Rosewood Logs"]),
        ("Recharge Dragonstone", &["Amulet of Glory", "Combat Bracelet", "Skills Necklace"]),
    ];
    let mut spell = text(values, "CHOSENSPELL");
    if spell.is_empty() {
        spell = "Bake Pie".into();
    }
    let names: Vec<&str> = SPELLS.iter().map(|(name, _)| *name).collect();
    choose(ui, "lunar-spell", "Spell", &mut spell, &names);
    values.insert("CHOSENSPELL".into(), spell.clone());
    let items = SPELLS.iter().find(|(name, _)| *name == spell).map(|(_, items)| *items).unwrap_or(&[]);
    let mut item = text(values, "SELECTEDITEM");
    choose(ui, "lunar-item", "Item", &mut item, items);
    values.insert("SELECTEDITEM".into(), item);
    let mut speed = text(values, "SCRIPTSPEED");
    choose_tip(
        ui,
        "lunar-speed",
        "Speed",
        "How quickly spells are cast. Faster is less human-like and can miss more on slower clients.",
        &mut speed,
        &["Slow", "Normal", "Fast", "Turbo"],
    );
    values.insert("SCRIPTSPEED".into(), speed);
    number_box_tip(ui, "Quantity (0 keeps going)", "Stop after this many casts. 0 keeps going.", values, "QUANTITYINPUT", "0");
    number_box_tip(
        ui,
        "Stop at level (-1 keeps going)",
        "Stop when Magic reaches this level. -1 keeps going.",
        values,
        "STOP_AT_LEVEL",
        "-1",
    );
    checkbox_tip(ui, "Sell the noted item", "Sell the noted product at the Grand Exchange when banking.", values, "SELLNOTEDITEMCHECK", false);
    checkbox_tip(ui, "Buy runes", "Buy missing runes from the Grand Exchange when needed.", values, "BUYRUNESCHECK", false);
    checkbox_tip(ui, "Buy raw materials", "Buy missing materials from the Grand Exchange when needed.", values, "BUYRAWMATERIALS", false);
    checkbox_tip(ui, "Bank fillers", "Use bank fillers so the inventory layout stays consistent.", values, "BANKFILLERSCHECK", false);
}

const VALE_LOGS: &[&str] = &["Oak", "Willow", "Maple", "Yew", "Magic", "Redwood"];
const VALE_DECOR_STANDARD: &[&str] = &["shortbow (u)", "shortbow", "longbow (u)", "longbow", "shield", "stock"];
const VALE_DECOR_REDWOOD: &[&str] = &["hiking staff", "shield"];

fn vale_log_name(values: &HashMap<String, String>) -> &'static str {
    let index = values.get("LogsOptIndex").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    VALE_LOGS.get(index).copied().unwrap_or(VALE_LOGS[0])
}

fn vale_decor_suffixes(log: &str) -> &'static [&'static str] {
    if log.eq_ignore_ascii_case("redwood") {
        VALE_DECOR_REDWOOD
    } else {
        VALE_DECOR_STANDARD
    }
}

fn vale_decor_options(log: &str) -> Vec<String> {
    let mut options = vec!["Fletch decor from logs".to_string()];
    for suffix in vale_decor_suffixes(log) {
        options.push(format!("{log} {suffix}"));
    }
    options
}

/// Launcher saves "Longbow (u)". The editor saves "Magic longbow (u)".
/// Both mean the same decoration for the selected log.
fn vale_remap_decor(saved: &str, log: &str, options: &[String]) -> String {
    let saved = saved.trim();
    if saved.is_empty() || saved.eq_ignore_ascii_case("fletch decor from logs") {
        return "Fletch decor from logs".to_string();
    }
    let name = saved.to_ascii_lowercase();
    let mut suffixes: Vec<&str> = VALE_DECOR_STANDARD.iter().copied().chain(VALE_DECOR_REDWOOD.iter().copied()).collect();
    suffixes.sort_by_key(|suffix| std::cmp::Reverse(suffix.len()));
    suffixes.dedup();
    for suffix in suffixes {
        if name == suffix || name.ends_with(&format!(" {suffix}")) {
            let mapped = format!("{log} {suffix}");
            if let Some(option) = options.iter().find(|option| option.eq_ignore_ascii_case(&mapped)) {
                return option.clone();
            }
            break;
        }
    }
    options.first().cloned().unwrap_or_else(|| "Fletch decor from logs".to_string())
}

fn rows_page(ui: &mut Ui, rows: &[Row], values: &mut HashMap<String, String>) {
    let mut shown_section = "";
    for row in rows {
        if !row.visible {
            continue;
        }
        let farm = flag(values, "EnableFarmRuns", false)
            || flag(values, "EnableBirdhouseRuns", false)
            || flag(values, "EnableBirdhouse", false);
        if matches!(row.key, "FarmReturnMethod" | "BirdhouseReturnMethod" | "BankReturnMethod") && !farm {
            continue;
        }
        if !row.section.is_empty() && row.section != shown_section {
            if !shown_section.is_empty() {
                ui.add_space(12.0);
            }
            ui.label(RichText::new(row.section).strong());
            ui.add_space(4.0);
            shown_section = row.section;
        }
        match row.kind {
            RowKind::Choice(options) => {
                if matches!(row.key, "ReturnMethod" | "BankReturnMethod" | "FarmReturnMethod" | "BirdhouseReturnMethod") && !farm {
                    continue;
                }
                let mut current = text(values, row.key);
                if row.key == "DecorOpt" {
                    let log = vale_log_name(values);
                    let decor = vale_decor_options(log);
                    current = vale_remap_decor(&current, log, &decor);
                    let shown: Vec<&str> = decor.iter().map(String::as_str).collect();
                    choose_tip(ui, row.key, row.label, row.tip, &mut current, &shown);
                } else {
                    choose_tip(ui, row.key, row.label, row.tip, &mut current, options);
                }
                values.insert(row.key.to_string(), current);
            }
            RowKind::ChoiceAt(options, default) => {
                let mut current = text(values, row.key);
                if current.trim().is_empty() {
                    if let Some(option) = options.get(default).or_else(|| options.first()) {
                        current = (*option).to_string();
                    }
                }
                choose_tip(ui, row.key, row.label, row.tip, &mut current, options);
                values.insert(row.key.to_string(), current);
            }
            RowKind::Index(options, default) => {
                choose_index_tip(ui, row.key, row.label, row.tip, values, row.key, options, default)
            }
            RowKind::Check(default) => {
                checkbox_tip(ui, row.label, row.tip, values, row.key, default);
            }
            RowKind::Number(default) => number_box_tip(ui, row.label, row.tip, values, row.key, default),
        }
    }
}

const fn account(names: &'static [&'static str], suffix: &'static str, keys: &'static [&'static str], kind: Kind) -> Spec {
    Spec { names, suffix, file: None, keys, rows: &[], kind }
}

const fn rows(names: &'static [&'static str], suffix: &'static str, keys: &'static [&'static str], rows: &'static [Row]) -> Spec {
    Spec { names, suffix, file: None, keys, rows, kind: Kind::Rows }
}

const fn global(names: &'static [&'static str], file: &'static str, rows: &'static [Row]) -> Spec {
    Spec { names, suffix: "", file: Some(file), keys: &[], rows, kind: Kind::Rows }
}

/// Per-account section inside a file other than BASettings.ini.
const fn account_file(
    names: &'static [&'static str],
    file: &'static str,
    suffix: &'static str,
    keys: &'static [&'static str],
    rows: &'static [Row],
) -> Spec {
    Spec { names, suffix, file: Some(file), keys, rows, kind: Kind::Rows }
}

const FISH_KEYS: &[&str] = &["Location", "Method", "Mode", "EnableFarmRuns", "EnableBirdhouseRuns", "BankReturnMethod"];
const MINE_KEYS: &[&str] = &["Ore", "Location", "Mode", "EnableFarmRuns", "EnableBirdhouseRuns", "BankReturnMethod"];
const WOOD_KEYS: &[&str] = &[
    "Location", "Tree", "Mode", "FletchItem", "EnableFarmRuns", "EnableBirdhouseRuns", "BankReturnMethod", "StayNearPlayers",
    "PreferForestryWorlds", "ForestryRisingRoots", "ForestryBeehive", "ForestryRitual", "ForestryLeprechaun", "ForestryPoachers",
    "SoulsCloseTeaks",
];
const COOK_KEYS: &[&str] = &["Location", "Food", "CustomFood", "CookTab", "CookMode", "EnableFarmRuns", "EnableBirdhouse", "ReturnMethod"];
const ANVIL_KEYS: &[&str] = &["Location", "Metal", "Item", "EnableFarmRuns", "EnableBirdhouse", "ReturnMethod"];
const SHOP_KEYS: &[&str] = &["Location", "DefaultMinStock", "ItemsCsv"];
const SMELT_KEYS: &[&str] = &[
    "Furnace", "SmeltMode", "CannonBallType", "BarProduct", "JewelType", "GemType", "MetalType", "EnableFarmRuns", "EnableBirdhouse",
    "BankReturnMethod",
];
const BANK_KEYS: &[&str] = &[
    "Mode", "Bank", "Spell", "PrimaryItem", "SecondaryItem", "ToolItem", "MaterialItem", "MakeText", "FastEnchanting", "AlchTab",
    "EnableFarmRuns", "EnableBirdhouse", "FarmReturnMethod", "BirdhouseReturnMethod", "BankReturnMethod",
];

const fn choice(key: &'static str, label: &'static str, options: &'static [&'static str]) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Choice(options), visible: true }
}
const fn choice_at(options: &'static [&'static str], default: usize) -> RowKind {
    RowKind::ChoiceAt(options, default)
}
const fn index(key: &'static str, label: &'static str, options: &'static [&'static str], default: usize) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Index(options, default), visible: true }
}
const fn check(key: &'static str, label: &'static str, default: bool) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Check(default), visible: true }
}
const fn number(key: &'static str, label: &'static str, default: &'static str) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Number(default), visible: true }
}
const fn filed(section: &'static str, key: &'static str, label: &'static str, kind: RowKind) -> Row {
    Row { key, label, section, tip: "", kind, visible: true }
}
const fn tip(mut row: Row, tip: &'static str) -> Row {
    row.tip = tip;
    row
}
const fn hide(mut row: Row) -> Row {
    row.visible = false;
    row
}

pub struct ActivityChance {
    pub key: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
    pub default: i32,
    pub low: i32,
    pub high: i32,
}

const TD_ACTIVITY: &[ActivityChance] = &[
    ActivityChance {
        key: "LadderWestChance",
        label: "West ladder chance",
        tip: "Chance to take the west ladder pair into the lair. 0 is always east, 100 is always west, 50 is roughly random each trip.",
        default: 50,
        low: 20,
        high: 80,
    },
    ActivityChance {
        key: "DemonSouthChance",
        label: "South room chance",
        tip: "Chance to fight the south Tormented Demon room. 0 is always north, 100 is always south.",
        default: 0,
        low: 0,
        high: 40,
    },
    ActivityChance {
        key: "OffensiveAfterAttackChance",
        label: "Offensive after attack %",
        tip: "When swapping gear, chance to re-click the demon before turning on the offensive prayer. 0 always prays then attacks.",
        default: 0,
        low: 0,
        high: 40,
    },
    ActivityChance {
        key: "BuffTimingChance",
        label: "Buff timing %",
        tip: "Chance to cast Mark of Darkness or thralls a bit late, or to skip a refresh cycle. 0 stays on time.",
        default: 0,
        low: 0,
        high: 40,
    },
    ActivityChance {
        key: "PrepProtectFirstChance",
        label: "Protect before buffs %",
        tip: "After gear is worn, chance to turn on protection prayer before the thrall or Mark of Darkness. 0 always buffs then protects.",
        default: 40,
        low: 15,
        high: 70,
    },
];

pub fn activity_chances(script: &str) -> &'static [ActivityChance] {
    if script.eq_ignore_ascii_case("Tormented Demons") {
        TD_ACTIVITY
    } else {
        &[]
    }
}

static SPECS: &[Spec] = &[
    account(&["AIO Fishing"], " AIO Fishing Settings", FISH_KEYS, Kind::Fish),
    account(&["AIO Mining"], " AIO Mining Settings", MINE_KEYS, Kind::Mine),
    account(&["AIO Woodcutting"], " AIO Woodcutting Settings", WOOD_KEYS, Kind::Wood),
    account(&["AIO Cooker"], " BA AIO Cooker Settings", COOK_KEYS, Kind::Cook),
    account(&["AIO Smelter and Jewelry", "AIO Smelter"], " BA AIO Smelter Settings", SMELT_KEYS, Kind::Smelt),
    account(&["AIO Bankstander"], " BA AIO Bankstander Settings", BANK_KEYS, Kind::Bank),
    account(&["Anvil Smither"], " BA Anvil Smither Settings", ANVIL_KEYS, Kind::Anvil),
    account(&["AIO Shopper"], " BA AIO Shopper Settings", SHOP_KEYS, Kind::Shop),
    account(
        &["AIO Lunar Spells"],
        " AIO Lunar Settings",
        &[
            "CHOSENSPELL", "SELECTEDITEM", "SCRIPTSPEED", "SELLNOTEDITEMCHECK", "BUYRUNESCHECK", "BUYRAWMATERIALS", "BANKFILLERSCHECK",
            "QUANTITYINPUT", "STOP_AT_LEVEL",
        ],
        Kind::Lunar,
    ),
    rows(
        &["Mahogany Homes", "AIO Mahogany Homes"],
        " MH Settings",
        &["Tier", "Contact", "Count", "Contractor", "Falador", "Ardougne", "Varrock", "Hosidius", "HouseEntry"],
        &[
            tip(choice("Tier", "Contract", &["Beginner", "Novice", "Adept", "Expert"]), "Contract tier to take from the contractors."),
            tip(choice("Contact", "Contact", &["NPC Contact", "Talk to contractor"]), "How to start or continue a contract with the contractor."),
            tip(choice("Contractor", "Contractor", &["Closest", "Random", "Amy, Falador", "Marlo, Varrock", "Ellie, Ardougne", "Angelo, Hosidius"]), "Which contractor to use when asking for work."),
            tip(number("Count", "Contracts to finish", "50"), "Stop after this many contracts."),
            tip(choice("Falador", "Falador travel", &["Falador tablet", "Falador spell", "Ring of wealth", "Skills necklace", "Portal nexus", "Portal chamber"]), "How to reach Falador houses."),
            tip(choice("Ardougne", "Ardougne travel", &["Ardougne tablet", "Ardougne spell", "Portal nexus", "Portal chamber"]), "How to reach Ardougne houses."),
            tip(choice("Varrock", "Varrock travel", &["Varrock tablet", "Varrock spell", "Portal nexus", "Portal chamber"]), "How to reach Varrock houses."),
            tip(choice("Hosidius", "Hosidius travel", &["Xeric's talisman", "House teleport, outside", "House tablet, outside", "Mounted Xeric's talisman"]), "How to reach Hosidius houses."),
            tip(choice("HouseEntry", "House entry", &["Spell", "Tablet", "Construction cape"]), "How to enter your player-owned house when a travel method needs it."),
        ],
    ),
    rows(
        &["Vale Totems"],
        " Vale Totem Settings",
        &["LogsOptIndex", "DecorOpt", "UseLogBasket", "EnableFarmRuns", "EnableBirdhouseRuns", "BankReturnMethod"],
        &[
            tip(index("LogsOptIndex", "Logs", &["Oak", "Willow", "Maple", "Yew", "Magic", "Redwood"], 0), "Logs used for totem decorations."),
            tip(choice("DecorOpt", "Decoration", &["Fletch decor from logs"]), "Decoration placed on the totems. The list follows the selected logs. Redwood can only use a shield or hiking staff. Fletch decor from logs makes them on the spot."),
            tip(check("UseLogBasket", "Use a log basket", false), "Carry extra logs in a log basket when available."),
            tip(check("EnableFarmRuns", "Farm runs", false), TIP_FARM),
            tip(check("EnableBirdhouseRuns", "Birdhouse runs", false), TIP_BIRD),
            tip(choice("BankReturnMethod", "Return method", &["Runes", "Perfected quetzal whistle(i)"]), TIP_RETURN),
        ],
    ),
    rows(
        &["Tempoross"],
        " Tempoross Settings",
        &["HarpoonIndex", "Buckets", "PermitLimit", "CookCatch", "NeedRope", "NeedHammer", "SoloRound"],
        &[
            tip(index("HarpoonIndex", "Harpoon", &["Bare-handed", "Harpoon", "Barb-tail harpoon", "Dragon harpoon", "Infernal harpoon", "Crystal harpoon"], 1), "Harpoon used while fishing at Tempoross."),
            tip(number("Buckets", "Buckets", "4"), "Empty buckets brought for dousing fires."),
            tip(number("PermitLimit", "Stop after permits (0 keeps going)", "0"), "Stop after earning this many spirit angler permits. 0 keeps going."),
            tip(check("CookCatch", "Cook the catch", true), "Cook fish on the shrine before depositing."),
            tip(check("NeedRope", "Bring a rope", true), "Bring a rope for the boat and island obstacles."),
            tip(check("NeedHammer", "Bring a hammer", true), "Bring a hammer for repairing the totem and mast."),
            tip(check("SoloRound", "Solo rounds", false), "Play rounds alone instead of joining a filled boat."),
        ],
    ),
    rows(
        &["Crab Trapping"],
        " Crab Trapper Settings",
        &["CrabType", "TrapCount", "UseBankForRedCrabs", "EnablePlayerDetection", "PlayerDetectionDelay"],
        &[
            tip(index("CrabType", "Crab", &["Red", "Blue", "Rainbow"], 0), "Which crabs to trap."),
            tip(index("TrapCount", "Traps", &["Automatic", "1", "2", "3", "4", "5"], 0), "How many traps to place. Automatic uses your Hunter level."),
            tip(check("UseBankForRedCrabs", "Bank red crabs", false), "Bank raw red crabs instead of cooking or dropping them."),
            tip(check("EnablePlayerDetection", "Leave when another player is nearby", true), "Hop or leave if another player stays nearby."),
            tip(choice("PlayerDetectionDelay", "Seconds to wait before leaving", &["0", "10", "20", "30"]), "How long another player can stay nearby before leaving."),
        ],
    ),
    rows(
        &["Gemstone Crab", "GemStone Crab Slayer"],
        " Gemstone Crab Settings",
        &[
            "UseBoosts", "UseDharoks", "UseDragonBattleaxe", "RangedMode", "UseSpecialAttacks", "UseQuickPrayers", "EnableBoltEnchanting",
            "EnableFarmRuns", "EnableBirdhouseRuns", "FarmReturnMethod", "BirdhouseReturnMethod", "DharoksTargetHP", "EnablePrayerFlick",
        ],
        &[
            tip(check("UseBoosts", "Use boosts", false), "Drink combat boosts when they run out."),
            tip(check("RangedMode", "Ranged", false), "Fight with ranged instead of melee."),
            tip(check("UseSpecialAttacks", "Special attacks", false), "Use the equipped weapon special attack when it is available."),
            tip(check("UseDragonBattleaxe", "Dragon battleaxe", false), "Spec with a dragon battleaxe for the strength boost."),
            tip(check("UseQuickPrayers", "Quick prayers", false), "Turn on quick prayers while fighting."),
            tip(check("UseDharoks", "Dharok's", false), "Use Dharok's set and stay low HP for the set effect."),
            tip(number("DharoksTargetHP", "Dharok's target HP", "1"), "Hitpoints to stay around while using Dharok's."),
            tip(check("EnablePrayerFlick", "Prayer flick", false), "Flick protection or offensive prayers to save prayer points."),
            tip(check("EnableBoltEnchanting", "Enchant bolts", false), "Enchant bolts while waiting at the crab."),
            tip(check("EnableFarmRuns", "Farm runs", false), TIP_FARM),
            tip(check("EnableBirdhouseRuns", "Birdhouse runs", false), TIP_BIRD),
            tip(choice("FarmReturnMethod", "Farm return", &["Runes", "Quetzal"]), TIP_RETURN),
            tip(choice("BirdhouseReturnMethod", "Birdhouse return", &["Runes", "Quetzal"]), TIP_RETURN),
        ],
    ),
    rows(
        &["Blackjack", "Blackjackking"],
        "Blackjackkin Settings",
        &["NPCHIGHLIGHTPLUGIN", "StopAtLevel", "Npc"],
        &[
            tip(choice("Npc", "NPC", &["Menaphite Thug", "Bandit"]), "Which NPC to blackjack."),
            tip(number("StopAtLevel", "Stop at level (0 keeps going)", "0"), "Stop when Thieving reaches this level. 0 keeps going."),
            tip(check("NPCHIGHLIGHTPLUGIN", "NPC highlight plugin", false), "Use the RuneLite NPC highlight plugin to find targets."),
        ],
    ),
    rows(
        &["Chinchompa Hunter"],
        " Chinchompa Settings",
        &["StopAtLevel", "StopAtCount", "UseOptimalSpot", "EnablePlayerDetection", "LeaveTrapsOnDetection", "PlayerDetectionDelay"],
        &[
            tip(number("StopAtLevel", "Stop at level (0 keeps going)", "0"), "Stop when Hunter reaches this level. 0 keeps going."),
            tip(number("StopAtCount", "Stop at chin count (0 keeps going)", "0"), "Stop after catching this many chinchompas. 0 keeps going."),
            tip(check("UseOptimalSpot", "Use the optimal spot", true), "Prefer the best trap layout for the hunting area."),
            tip(check("EnablePlayerDetection", "Leave when another player is nearby", true), "Hop or leave if another player stays nearby."),
            tip(check("LeaveTrapsOnDetection", "Leave traps down when leaving", false), "Leave traps set when hopping away from another player."),
            tip(choice("PlayerDetectionDelay", "Seconds to wait before leaving", &["0", "10", "20", "30"]), "How long another player can stay nearby before leaving."),
        ],
    ),
    rows(
        &["Wyrmscraig Goat Hunting", "Goat Hunting"],
        " Goat Hunting Settings",
        &["HornOptIndex", "FurOptIndex", "EnablePlayerDetection", "PlayerDetectionDelay"],
        &[
            tip(index("HornOptIndex", "Horns", &["Crush", "Drop", "Keep and bank"], 0), "What to do with goat horns."),
            tip(index("FurOptIndex", "Fur", &["Keep and bank", "Drop"], 0), "What to do with goat fur."),
            tip(check("EnablePlayerDetection", "Leave when another player is nearby", true), "Hop or leave if another player stays nearby."),
            tip(choice("PlayerDetectionDelay", "Seconds to wait before leaving", &["0", "10", "20", "30"]), "How long another player can stay nearby before leaving."),
        ],
    ),
    Spec {
        names: &["Shipwreck Salvaging"],
        suffix: " ShipWreck Settings",
        file: None,
        keys: crate::salvage::SETTING_KEYS,
        rows: &[],
        kind: Kind::Salvage,
    },
    global(
        &["Vardorvis Slayer"],
        "vardorvis.ini",
        &[
            tip(
                filed("Navigation", "BankLocation", "Bank", RowKind::Choice(&["Crafting Guild (Cape)", "Castle Wars (Ring)", "Castle Wars (POH)", "Ferox Enclave (Ring)", "Ferox Enclave (POH)"])),
                "Where to bank when supplies run low.",
            ),
            tip(
                filed("Navigation", "TravelMethod", "Travel", RowKind::Choice(&["Fairy Ring (POH)", "Ring of Shadow", "Fairy Ring (Salve)"])),
                "How to return to Vardorvis after banking.",
            ),
            tip(
                filed("Navigation", "RestoreMethod", "Restore", RowKind::Choice(&["Ornate rejuvenation pool (POH)", "Pool of Refreshment (Ferox)"])),
                "Where to restore hitpoints and prayer between trips.",
            ),
            tip(
                filed("Navigation", "POHTeleport", "House teleport", RowKind::Choice(&["Construct. cape", "Construct. cape(t)", "Max cape", "Teleport to house (tablet)"])),
                "How to enter your player-owned house when Bank, Travel, or Restore needs the POH.",
            ),
        ],
    ),
    global(
        &["Tormented Demons"],
        "tormenteddemons.ini",
        &[
            tip(
                filed("Navigation", "BankLocation", "Bank", RowKind::Choice(&["Ferox Enclave (Ring)", "Ferox Enclave (POH)"])),
                "Where to bank when supplies run low. Ferox (Ring) uses a Ring of dueling. Ferox (POH) teleports to your house jewellery box first.",
            ),
            tip(
                filed("Navigation", "TravelMethod", "Return to lair", RowKind::Choice(&["Guthixian Temple Teleport", "Games Necklace (Chasm)"])),
                "How to return to the Tormented Demon lair after banking. Guthixian Temple Teleport uses the scroll. Games Necklace uses the Tears of Guthix chasm route with a sapphire lantern.",
            ),
            tip(
                filed(
                    "Navigation",
                    "POHTeleport",
                    "House teleport",
                    choice_at(&["Construct. cape", "Construct. cape(t)", "Teleport to house (tablet)"], 2),
                ),
                "How to enter your player-owned house when Bank or Travel needs the POH. The house layout is still set in the Simba House Builder tab.",
            ),
            tip(
                filed("Combat", "EatAtPercent", "Eat at HP %", RowKind::Number("60")),
                "Eat food when your Hitpoints % falls to this value or below during a fight.",
            ),
            tip(
                filed("Combat", "FoodPerKill", "Food pts / kill", RowKind::Number("200")),
                "Banking threshold based on healing points, not food item count. After topping up to full HP, this many food healing points must still remain before starting another kill. Example: sharks heal 20 each, so 10 sharks = 200 pts.",
            ),
            tip(
                filed("Combat", "DrinkAtPercent", "Drink at prayer %", RowKind::Number("40")),
                "Drink a prayer potion when your Prayer % falls to this value or below during a fight.",
            ),
            tip(
                filed("Combat", "PrayerPerKill", "Prayer pts / kill", RowKind::Number("124")),
                "Banking threshold based on prayer points restored by potions, not dose count. After restoring to full Prayer, this many prayer points must still remain before starting another kill.",
            ),
            tip(
                filed(
                    "Combat",
                    "SummonThrall",
                    "Summon thrall",
                    RowKind::Choice(&["None", "Ghost (Magic)", "Skeleton (Ranged)", "Zombie (Melee)", "Random"]),
                ),
                "Which Arceuus thrall to summon and refresh during combat. None disables thralls. Random picks Ghost, Skeleton, or Zombie each summon.",
            ),
            tip(
                filed("Combat", "OffensivePrayers", "Offensive prayers", RowKind::Check(false)),
                "Enable the strongest unlocked offensive prayer for the active attack style (Piety, Rigour, or Augury) in addition to protection prayers.",
            ),
            tip(
                filed("Combat", "MaxKills", "Max kills (0 = unlimited)", RowKind::Number("0")),
                "Stop the script after this many Tormented Demon kills. 0 means no kill limit.",
            ),
            tip(
                filed("Combat", "StopAfterSlayerTask", "Stop after the slayer task", RowKind::Check(false)),
                "Stop and logout when the Slayer task complete message appears in game chat.",
            ),
            tip(
                filed(
                    "Combat",
                    "DodgePlanMs",
                    "Dodge plan (ms)",
                    choice_at(&["1000", "2000", "3000", "4000", "5000"], 1),
                ),
                "How early (in milliseconds) to start planning and moving for bomb dodges. Default 2000ms. Fast PCs can use 1000ms; slower PCs should use about 3000-5000ms for more reliable dodges.",
            ),
            hide(tip(
                filed("Antiban", "LadderWestChance", "West ladder chance (0-100)", RowKind::Number("50")),
                "Chance to take the west ladder pair into the lair. 0 is always east, 100 is always west, 50 is roughly random each trip.",
            )),
            hide(tip(
                filed("Antiban", "DemonSouthChance", "South room chance (0-100)", RowKind::Number("0")),
                "Chance to fight the south Tormented Demon room. 0 is always north, 100 is always south.",
            )),
            hide(tip(
                filed("Antiban", "OffensiveAfterAttackChance", "Offensive after attack % (0-100)", RowKind::Number("0")),
                "When swapping gear, chance to re-click the demon before turning on the offensive prayer. 0 always prays then attacks.",
            )),
            hide(tip(
                filed("Antiban", "BuffTimingChance", "Buff timing antiban % (0-100)", RowKind::Number("0")),
                "Chance to cast Mark of Darkness or thralls a bit late, or to skip a refresh cycle. 0 stays on time.",
            )),
            hide(tip(
                filed("Antiban", "PrepProtectFirstChance", "Protect before buffs % (0-100)", RowKind::Number("40")),
                "After gear is worn, chance to turn on protection prayer before the thrall or Mark of Darkness. 0 always buffs then protects.",
            )),
        ],
    ),
    rows(
        &["Free Anvil Smither"],
        " Free Anvil Smither Settings",
        &["Metal", "Item"],
        &[
            tip(choice("Metal", "Metal", &["Bronze", "Iron", "Steel", "Mithril", "Adamant", "Rune"]), "Bar type to smith. Bars need to be in the bank."),
            tip(
                choice(
                    "Item",
                    "Item",
                    &[
                        "Dagger", "Sword", "Scimitar", "Mace", "Longsword", "Battleaxe", "Warhammer", "2h sword", "Axe", "Platebody",
                        "Platelegs", "Plateskirt", "Chainbody", "Kiteshield", "Sq shield", "Full helm", "Med helm", "Nails", "Arrowtips",
                        "Dart tip", "Knife", "Javelin heads", "Bolts (unf)", "Claws", "Limbs", "Wire",
                    ],
                ),
                "Item to smith at the Varrock west anvils.",
            ),
        ],
    ),
    rows(
        &["Free Agility"],
        " Free Agility Settings",
        &["Course"],
        &[tip(
            choice("Course", "Course", &["Draynor Village Rooftop", "Varrock Rooftop", "Falador Rooftop"]),
            "Rooftop course to run.",
        )],
    ),
    rows(
        &["Redwood Cutter"],
        " Redwood Cutter Settings",
        &["Mode", "FletchItem"],
        &[
            tip(choice("Mode", "Mode", &["Powercut", "Bank", "Fletch and drop", "Fletch and bank"]), "What to do with the logs."),
            tip(
                choice("FletchItem", "Fletch", &["Arrow shaft", "Redwood shield", "Redwood hiking staff"]),
                "Item to fletch when the mode includes fletching.",
            ),
        ],
    ),
    rows(
        &["Auburnvale Sawmill Plank Maker"],
        " Auburnvale Sawmill Settings",
        &["LogsOptIndex", "PlankGoal", "ScriptMode", "EnableFarmRuns", "EnableBirdhouseRuns", "BankReturnMethod"],
        &[
            tip(
                index("LogsOptIndex", "Logs", &["Logs", "Oak logs", "Teak logs", "Mahogany logs", "Camphor logs", "Ironwood logs", "Rosewood logs"], 1),
                "Logs to turn into planks.",
            ),
            tip(number("PlankGoal", "Planks to make (0 keeps going)", "0"), "Stop after this many planks. 0 keeps going."),
            tip(choice("ScriptMode", "Speed", &["Fast", "Normal"]), "Fast is quicker and can miss a click. Normal waits longer between actions."),
            tip(check("EnableFarmRuns", "Farm runs", false), TIP_FARM),
            tip(check("EnableBirdhouseRuns", "Birdhouse runs", false), TIP_BIRD),
            tip(
                choice(
                    "BankReturnMethod",
                    "Return method",
                    &["Runes", "Basic quetzal whistle", "Enhanced quetzal whistle", "Perfected quetzal whistle", "Perfected quetzal whistle(i)"],
                ),
                TIP_RETURN,
            ),
        ],
    ),
    rows(
        &["Cannonball Smelter"],
        " Cannonball Smelter Settings",
        &["CannonBallType"],
        &[tip(
            index(
                "CannonBallType",
                "Cannonball",
                &["Bronze Cannonballs", "Iron Cannonballs", "Steel Cannonballs", "Mithril Cannonballs", "Adamant Cannonballs", "Rune Cannonballs"],
                2,
            ),
            "Cannonball type to smelt at Edgeville.",
        )],
    ),
    rows(
        &["Salt Miner"],
        " Salt Miner Settings",
        &["BasaltOnly", "StonyBasalt", "IcyBasalt", "TeSalt", "EfhSalt", "UrtSalt"],
        &[
            tip(check("BasaltOnly", "Basalt only", false), "Mine and note basalt without crafting salts."),
            tip(number("StonyBasalt", "Stony basalt", "10"), "How much stony basalt to craft. 0 skips it."),
            tip(number("IcyBasalt", "Icy basalt", "10"), "How much icy basalt to craft. 0 skips it."),
            tip(number("TeSalt", "Te salt", "10"), "How much te salt to craft. 0 skips it."),
            tip(number("EfhSalt", "Efh salt", "1000"), "How much efh salt to craft. 0 skips it."),
            tip(number("UrtSalt", "Urt salt", "1000"), "How much urt salt to craft. 0 skips it."),
        ],
    ),
    rows(
        &["Volcanic Ash Miner"],
        " Volcanic Ash Miner Settings",
        &[
            "StopAtCount", "UseSouthSide", "UseNorthSide", "EnableSlowMode", "EnablePlayerDetection", "PlayerDetectionDelay", "EnablePreHover",
        ],
        &[
            tip(number("StopAtCount", "Stop at ash count (0 keeps going)", "0"), "Stop after mining this much volcanic ash. 0 keeps going."),
            tip(check("UseSouthSide", "South side", true), "Mine the south side of the crater."),
            tip(check("UseNorthSide", "North side", false), "Mine the north side of the crater."),
            tip(check("EnableSlowMode", "Slow mode", false), "Wait longer between clicks."),
            tip(check("EnablePlayerDetection", "Hop when another player is nearby", false), "Hop if another player stays nearby."),
            tip(choice("PlayerDetectionDelay", "Seconds before hopping", &["0", "10", "20", "30"]), "How long another player can stay nearby before hopping."),
            tip(check("EnablePreHover", "Hover the next ash", true), "Move the mouse onto the next ash while mining."),
        ],
    ),
    global(
        &["Calcified Mining"],
        "BASettings.ini",
        &[
            tip(filed("Calcified Mining Settings", "PowerMining", "Power mine", RowKind::Check(false)), "Drop the deposits instead of banking them."),
            tip(
                filed("Calcified Mining Settings", "IgnoreWaterfalls", "Ignore waterfalls", RowKind::Check(false)),
                "Keep mining when a waterfall is in the way.",
            ),
        ],
    ),
    rows(
        &["Trouble Brewing Minigame"],
        " BATrouble Brewing",
        &["TeamSelection", "Method", "PowerFletch", "StopAtPointsLimit"],
        &[
            tip(choice("TeamSelection", "Team", &["Red", "Blue", "Random"]), "Which team to join."),
            tip(choice("Method", "Method", &["Scrapy Bark", "Buckets"]), "How to earn points."),
            tip(check("PowerFletch", "Power fletch", false), "Fletch bowls without waiting on the minigame."),
            tip(check("StopAtPointsLimit", "Stop at the points cap", false), "Stop when the bucket points cap is reached."),
        ],
    ),
    account_file(
        &["Ironwood WoodCutter"],
        "BAIronwoodSettings.ini",
        " Ironwood Script Settings",
        &["PreferPopulatedTrees", "PowerCut"],
        &[
            tip(check("PreferPopulatedTrees", "Prefer populated trees", true), "Chop trees that already have other players nearby."),
            tip(check("PowerCut", "Power cut", false), "Drop the logs instead of banking them."),
        ],
    ),
    rows(&["Ardy Superheat Iron"], "", &[], &[]),
    rows(&["Barronite Miner"], "", &[], &[]),
    rows(&["Free High Alchemy"], "", &[], &[]),
    rows(&["Infected Roots Chopper"], "", &[], &[]),
    rows(
        &["Standalone Farm and Birdhouse Runs"],
        "",
        &[],
        &[],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tormented_demons_lists_the_script_form_options() {
        let spec = spec_for("Tormented Demons").expect("tormented demons spec");
        let visible: Vec<&str> = spec.rows.iter().filter(|row| row.visible).map(|row| row.key).collect();
        assert_eq!(
            visible,
            vec![
                "BankLocation",
                "TravelMethod",
                "POHTeleport",
                "EatAtPercent",
                "FoodPerKill",
                "DrinkAtPercent",
                "PrayerPerKill",
                "SummonThrall",
                "OffensivePrayers",
                "MaxKills",
                "StopAfterSlayerTask",
                "DodgePlanMs",
            ]
        );
        for chance in activity_chances("Tormented Demons") {
            let row = spec.rows.iter().find(|row| row.key == chance.key).expect(chance.key);
            assert!(!row.visible);
            assert_eq!(row.section, "Antiban");
        }
        assert!(spec.rows.iter().all(|row| row.key != "MarkOfDarkness" && row.key != "DebugMode"));
        assert!(spec.rows.iter().all(|row| row.key != "PrayerHotkey" && row.key != "InventoryHotkey"));
    }

    #[test]
    fn smelter_follows_the_script_form() {
        assert_eq!(SMELT_FURNACES[3], "Edgeville");
        assert_eq!(smelt_returns("Grimstone Dungeon"), &[] as &[&str]);
        assert_eq!(smelt_returns("Edgeville"), &["Amulet of Glory"]);
        assert_eq!(smelt_returns("Deepfin Point"), &["Sailors' Amulet"]);
        assert_eq!(smelt_returns("Falador"), &["Tablet", "Spell"]);
        assert!(smelt_returns("Shilo Village").is_empty());
        let mut names: Vec<&str> = SMELT_FURNACES.to_vec();
        names.sort_by(|left, right| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()));
        assert_eq!(names.first().copied(), Some("Burgh de Rott"));
        assert_eq!(names[names.len() - 1], "Zanaris");
        assert_ne!(names[0], SMELT_FURNACES[0]);
    }

    #[test]
    fn wood_options_follow_the_location_and_tree() {
        let fossil = place(skillcatalog::WOOD, "Fossil Island").unwrap();
        assert_eq!(fossil.returns, &["Digsite pendant"]);
        assert!(wood_modes(fossil, "Camphor").iter().all(|mode| !mode.to_ascii_lowercase().contains("fletch")));
        assert_eq!(wood_fletch("Teak"), &["Teak stock"]);
        assert!(wood_fletch("Yew").contains(&"Yew shortbow"));
        assert!(!wood_fletch("Yew").contains(&"Oak shortbow"));
        assert!(!wood_player_boost("Fossil Island", "Teak"));
        assert!(wood_player_boost("Seers' Village", "Yew"));
        assert!(!wood_player_boost("Varrock east", "Regular"));
        assert!(!wood_forestry("Fossil Island", "Teak"));
        assert!(!wood_forestry("Woodcutting Guild", "Yew"));
        assert!(!wood_forestry("Neitiznot", "Arctic pine"));
        assert!(wood_forestry("Seers' Village", "Maple"));
        assert!(!place(skillcatalog::WOOD, "Darkmeyer").unwrap().side);
        assert_eq!(snap_choice("Yew shortbow", wood_fletch("Oak")), "Arrow shaft");
        assert_eq!(snap_choice("Digsite pendant", fossil.returns), "Digsite pendant");
    }

    #[test]
    fn bank_side_runs_follow_the_script() {
        let mut values = HashMap::new();
        values.insert("Mode".into(), "2".into());
        values.insert("Bank".into(), "8".into());
        assert!(bank_side_runs_allowed(&values));
        assert_eq!(bank_returns("Grand Exchange"), &["Ring of Wealth", "Grand Exchange Teleport", "Varrock Teleport"]);
        values.insert("Bank".into(), "5".into());
        assert!(!bank_side_runs_allowed(&values));
        values.insert("Bank".into(), "8".into());
        values.insert("Mode".into(), "6".into());
        assert!(!bank_side_runs_allowed(&values));
    }

    #[test]
    fn redwood_decorations_are_only_shield_and_hiking_staff() {
        let options = vale_decor_options("Redwood");
        assert_eq!(
            options,
            vec![
                "Fletch decor from logs".to_string(),
                "Redwood hiking staff".to_string(),
                "Redwood shield".to_string(),
            ]
        );
    }

    #[test]
    fn other_logs_keep_bows_and_drop_hiking_staff() {
        let options = vale_decor_options("Magic");
        assert_eq!(
            options,
            vec![
                "Fletch decor from logs".to_string(),
                "Magic shortbow (u)".to_string(),
                "Magic shortbow".to_string(),
                "Magic longbow (u)".to_string(),
                "Magic longbow".to_string(),
                "Magic shield".to_string(),
                "Magic stock".to_string(),
            ]
        );
    }

    #[test]
    fn switching_to_redwood_drops_a_bow() {
        let options = vale_decor_options("Redwood");
        assert_eq!(vale_remap_decor("Longbow (u)", "Redwood", &options), "Fletch decor from logs");
        assert_eq!(vale_remap_decor("Magic longbow (u)", "Redwood", &options), "Fletch decor from logs");
        assert_eq!(vale_remap_decor("Shield", "Redwood", &options), "Redwood shield");
        assert_eq!(vale_remap_decor("Hiking staff", "Redwood", &options), "Redwood hiking staff");
    }

    #[test]
    fn switching_logs_keeps_the_same_decoration() {
        let options = vale_decor_options("Oak");
        assert_eq!(vale_remap_decor("Magic longbow (u)", "Oak", &options), "Oak longbow (u)");
        assert_eq!(vale_remap_decor("Longbow (u)", "Oak", &options), "Oak longbow (u)");
        assert_eq!(vale_remap_decor("Redwood hiking staff", "Oak", &options), "Fletch decor from logs");
    }
}

