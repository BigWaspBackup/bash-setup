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
                let section_name = if spec.suffix.is_empty() { row.section } else { account_section.as_str() };
                let section = ini_section(&text, section_name);
                if let Some(value) = ini_get(&section, row.key) {
                    values.insert(row.key.to_string(), value);
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
                    .map(|row| (row.key, values.get(row.key).cloned().unwrap_or_default()))
                    .collect();
                next = write_section(&next, section, &pairs);
            }
        } else {
            let pairs: Vec<(&str, String)> = spec
                .rows
                .iter()
                .map(|row| (row.key, values.get(row.key).cloned().unwrap_or_default()))
                .collect();
            next = write_section(&next, &format!("{account}{}", spec.suffix), &pairs);
        }
        return std::fs::write(path, next).map_err(|error| format!("Could not save the script settings: {error}"));
    }
    let path = configs.join("BASettings.ini");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let pairs: Vec<(&str, String)> = spec
        .keys
        .iter()
        .map(|key| (*key, values.get(*key).cloned().unwrap_or_default()))
        .collect();
    let mut next = write_section(&existing, &format!("{account}{}", spec.suffix), &pairs);
    if spec.kind == Kind::Salvage {
        let boat_pairs: Vec<(&str, String)> = crate::salvage::BOAT_KEYS
            .iter()
            .map(|key| (*key, values.get(*key).cloned().unwrap_or_default()))
            .collect();
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
}

enum RowKind {
    Choice(&'static [&'static str]),
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
    if !options.iter().any(|option| *option == current.as_str()) {
        *current = options[0].to_string();
    }
    field_label(ui, label, tip, true);
    let shown = current.clone();
    ComboBox::from_id_salt(salt)
        .width(360.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(current, (*option).to_string(), *option);
            }
        });
}

fn choose_index(ui: &mut Ui, salt: &str, label: &str, values: &mut HashMap<String, String>, key: &str, options: &[&str], default: usize) {
    choose_index_tip(ui, salt, label, "", values, key, options, default);
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
        choose_tip(ui, "script-return", "Return method", TIP_RETURN, &mut method, returns);
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

fn wood_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = skillcatalog::WOOD.iter().map(|place| place.name).collect();
    let mut location = text(values, "Location");
    choose(ui, "wc-location", "Location", &mut location, &names);
    values.insert("Location".into(), location.clone());
    let Some(spot) = place(skillcatalog::WOOD, &location) else {
        return;
    };
    let mut tree = text(values, "Tree");
    choose(ui, "wc-tree", "Tree", &mut tree, spot.methods);
    values.insert("Tree".into(), tree);
    let mut mode = text(values, "Mode");
    choose_tip(
        ui,
        "wc-mode",
        "Mode",
        "Bank keeps logs. Drop or burn options clear the inventory without banking. Fletch options cut and fletch on the spot.",
        &mut mode,
        spot.modes,
    );
    values.insert("Mode".into(), mode.clone());
    if mode.contains("Fletch") && !spot.extras.is_empty() {
        let mut item = text(values, "FletchItem");
        choose_tip(ui, "wc-fletch", "Fletch", "Item to fletch from the logs before banking or dropping.", &mut item, spot.extras);
        values.insert("FletchItem".into(), item);
    }
    checkbox_tip(
        ui,
        "Stay near other players",
        "Prefer chopping near other players when the location allows it.",
        values,
        "StayNearPlayers",
        false,
    );
    checkbox_tip(
        ui,
        "Prefer forestry worlds",
        "Prefer worlds where forestry events are more common.",
        values,
        "PreferForestryWorlds",
        false,
    );
    if spot.side {
        side_runs(ui, values, "EnableBirdhouseRuns", spot.returns);
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

fn smelt_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    const FURNACES: &[&str] = &[
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
    choose_index_tip(ui, "smelt-furnace", "Furnace", "Which furnace to use.", values, "Furnace", FURNACES, 3);
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
    match text(values, "SmeltMode").as_str() {
        "1" => choose_index(
            ui,
            "smelt-bar",
            "Bar",
            values,
            "BarProduct",
            &["Bronze", "Iron", "Silver", "Steel", "Gold", "Mithril", "Adamantite", "Runite", "Molten glass"],
            4,
        ),
        "2" => {
            choose_index(
                ui,
                "smelt-jewel",
                "Jewelry",
                values,
                "JewelType",
                &["Tiara", "Holy symbol", "Unholy symbol", "Bolts", "Ring", "Necklace", "Bracelet", "Amulet"],
                4,
            );
            choose_index(
                ui,
                "smelt-gem",
                "Gem",
                values,
                "GemType",
                &["None", "Opal", "Jade", "Red topaz", "Sapphire", "Emerald", "Ruby", "Diamond", "Dragonstone", "Onyx", "Zenyte"],
                5,
            );
            choose_index(ui, "smelt-metal", "Metal", values, "MetalType", &["Silver", "Gold"], 1);
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
    let furnace = values.get("Furnace").and_then(|value| value.parse::<usize>().ok()).unwrap_or(3);
    let name = FURNACES.get(furnace).copied().unwrap_or("");
    let returns: &[&str] = match name {
        "Edgeville" => &["Amulet of Glory"],
        "Deepfin Point" => &["Sailors' Amulet"],
        "Falador" => &["Tablet", "Spell"],
        _ => &[],
    };
    if !returns.is_empty() {
        side_runs(ui, values, "EnableBirdhouse", returns);
    }
}

fn bank_page(ui: &mut Ui, values: &mut HashMap<String, String>) {
    const BANKS: &[&str] = &[
        "Al Kharid", "Ardougne", "Castle Wars", "Catherby", "Crafting Guild", "Draynor Village", "Edgeville", "Falador",
        "Grand Exchange", "Lunar Isle", "Mining Guild", "Prifddinas", "Seers Village", "Varrock", "Wintertodt", "Fossil Island",
        "Deepfin Point", "Blast Furnace", "Burgh De Rott", "Canifis", "Ferox Enclave", "Hosidius", "Lumbridge", "Mor Ul Rek",
        "Motherloade Mine", "Mount Karuulm", "Myths Guild", "Nardah", "Neitiznot", "Piscarilius", "Port Phasmatys", "Rogues Den",
        "Ruins Of Unkah", "Shilo Village", "The Onyx Crest", "The Summer Shore", "Warriors Guild", "Yanille", "Zanaris", "Wyrmscraig",
        "Cooks Guild", "Port Khazard",
    ];
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
    choose_index_tip(ui, "bank-where", "Bank", "Bank to stand at while processing.", values, "Bank", BANKS, 8);
    let mode = text(values, "Mode");
    if mode == "5" {
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
            false,
        );
    }
    if mode == "6" {
        choose_index(
            ui,
            "bank-alch",
            "Alch from",
            values,
            "AlchTab",
            &["Select", "Inventory", "Tab 1", "Tab 2", "Tab 3", "Tab 4", "Tab 5", "Tab 6", "Tab 7", "Tab 8", "Tab 9"],
            1,
        );
    } else {
        let mut primary = text(values, "PrimaryItem");
        let mut secondary = text(values, "SecondaryItem");
        let mut tool = text(values, "ToolItem");
        let mut material = text(values, "MaterialItem");
        let mut make = text(values, "MakeText");
        ui.label("Primary item");
        ui.text_edit_singleline(&mut primary);
        ui.label("Secondary item");
        ui.text_edit_singleline(&mut secondary);
        ui.label("Tool");
        ui.text_edit_singleline(&mut tool);
        ui.label("Material");
        ui.text_edit_singleline(&mut material);
        ui.label("Make text");
        ui.text_edit_singleline(&mut make);
        values.insert("PrimaryItem".into(), primary);
        values.insert("SecondaryItem".into(), secondary);
        values.insert("ToolItem".into(), tool);
        values.insert("MaterialItem".into(), material);
        values.insert("MakeText".into(), make);
    }
    side_runs(ui, values, "EnableBirdhouse", &["Amulet of Glory", "Ring of Dueling", "Skills Necklace"]);
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
    for row in rows {
        let farm = flag(values, "EnableFarmRuns", false)
            || flag(values, "EnableBirdhouseRuns", false)
            || flag(values, "EnableBirdhouse", false);
        if matches!(row.key, "FarmReturnMethod" | "BirdhouseReturnMethod" | "BankReturnMethod") && !farm {
            continue;
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
    "PreferForestryWorlds",
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
    Row { key, label, section: "", tip: "", kind: RowKind::Choice(options) }
}
const fn index(key: &'static str, label: &'static str, options: &'static [&'static str], default: usize) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Index(options, default) }
}
const fn check(key: &'static str, label: &'static str, default: bool) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Check(default) }
}
const fn number(key: &'static str, label: &'static str, default: &'static str) -> Row {
    Row { key, label, section: "", tip: "", kind: RowKind::Number(default) }
}
const fn filed(section: &'static str, key: &'static str, label: &'static str, kind: RowKind) -> Row {
    Row { key, label, section, tip: "", kind }
}
const fn tip(mut row: Row, tip: &'static str) -> Row {
    row.tip = tip;
    row
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
        &["AIO Mahogany Homes"],
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
            tip(choice("Hosidius", "Hosidius travel", &["Xeric's talisman", "House teleport outside", "House tablet outside", "Mounted Xeric's"]), "How to reach Hosidius houses."),
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
                filed("Combat", "EatAtPercent", "Eat at HP %", RowKind::Number("60")),
                "Eat food when your Hitpoints % falls to this value or below during a fight.",
            ),
            tip(
                filed("Combat", "DrinkAtPercent", "Drink at prayer %", RowKind::Number("40")),
                "Drink a prayer potion when your Prayer % falls to this value or below during a fight.",
            ),
            tip(
                filed("Combat", "DodgePlanMs", "Dodge plan (ms)", RowKind::Choice(&["1000", "2000", "3000", "4000", "5000"])),
                "How early (in milliseconds) to start planning and moving for bomb dodges. Default 2000ms. Fast PCs can use 1000ms; slower PCs should use about 3000-5000ms for more reliable dodges.",
            ),
            tip(
                filed("Combat", "FoodPerKill", "Food per kill", RowKind::Number("200")),
                "Banking threshold based on healing points, not food item count. After topping up to full HP, this many food healing points must still remain before starting another kill. Example: sharks heal 20 each, so 10 sharks = 200 pts.",
            ),
            tip(
                filed("Combat", "PrayerPerKill", "Prayer per kill", RowKind::Number("124")),
                "Banking threshold based on prayer points restored by potions, not dose count. After restoring to full Prayer, this many prayer points must still remain before starting another kill.",
            ),
            tip(
                filed("Combat", "StopAfterSlayerTask", "Stop after the slayer task", RowKind::Check(false)),
                "Stop and logout when the Slayer task complete message appears in game chat.",
            ),
            tip(
                filed("Navigation", "BankLocation", "Bank", RowKind::Choice(&["Ferox Enclave (Ring)", "Ferox Enclave (POH)"])),
                "Where to bank when supplies run low. Ferox (Ring) uses a Ring of dueling. Ferox (POH) teleports to your house jewellery box first.",
            ),
            tip(
                filed("Navigation", "TravelMethod", "Travel", RowKind::Choice(&["Guthixian Temple Teleport", "Games Necklace (Chasm)"])),
                "How to return to the Tormented Demon lair after banking. Guthixian Temple Teleport uses the scroll. Games Necklace uses the Tears of Guthix chasm route with a sapphire lantern.",
            ),
            tip(
                filed("Navigation", "POHTeleport", "House teleport", RowKind::Choice(&["Construct. cape", "Construct. cape(t)", "Teleport to house (tablet)"])),
                "How to enter your player-owned house when Bank or Travel needs the POH.",
            ),
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

