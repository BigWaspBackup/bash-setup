//! Shipwreck Salvaging launcher page. Writes the same INI keys as the script form.

use std::collections::HashMap;

use egui::{Align2, Color32, ComboBox, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::salvage_data::{self, ShipLoot};

pub const SETTING_KEYS: &[&str] = &[
    "SelectedShipwreck",
    "UseDoubleSalvageSpots",
    "UseCrewmateSalvaging",
    "SelectedCrewmate",
    "UseSalvageStation",
    "UseSailingBoost",
    "UseCrystalExtractor",
    "ItemsToKeep_Smallshipwreck",
    "ItemsToKeep_Fishermansshipwreck",
    "ItemsToKeep_Barracudashipwreck",
    "ItemsToKeep_Largeshipwreck",
    "ItemsToKeep_Pirateshipwreck",
    "ItemsToKeep_Mercenaryshipwreck",
    "ItemsToKeep_Fremennikshipwreck",
    "ItemsToKeep_Merchantshipwreck",
];

pub const BOAT_KEYS: &[&str] = &[
    "SmallBoat_IsConfigured",
    "SmallBoat_KegPos",
    "SmallBoat_KegPos_Grid",
    "SmallBoat_CrystalExtractorPos",
    "SmallBoat_CrystalExtractorPos_Grid",
    "SmallBoat_SalvageTablePos",
    "SmallBoat_SalvageTablePos_Grid",
    "MediumBoat_IsConfigured",
    "MediumBoat_KegPos",
    "MediumBoat_KegPos_Grid",
    "MediumBoat_CrystalExtractorPos",
    "MediumBoat_CrystalExtractorPos_Grid",
    "MediumBoat_SalvageTablePos",
    "MediumBoat_SalvageTablePos_Grid",
];

const CREW: &[&str] = &[
    "None",
    "Adventurer Ada",
    "Bosun Zarah",
    "Cabin Boy Jenkins",
    "Ex-Captain Siad",
    "Jittery Jim",
    "Jobless Jim",
    "Jolly Jim",
    "Oarswoman Olga",
    "Sailor Jakob",
    "Spotter Virginia",
];

const DESTROY_ITEMS: &[&str] = &[
    "Mouldy block",
    "Mouldy doll",
    "Broken compass",
    "Broken sextant",
    "Smashed mirror",
    "Rusty locket",
    "Dull knife",
    "Rusty coin",
];

struct Spot {
    x: i32,
    y: i32,
    extractor_only: bool,
}

const SMALL_SPOTS: &[Spot] = &[
    Spot { x: 0, y: 1, extractor_only: false },
    Spot { x: 0, y: 2, extractor_only: false },
    Spot { x: 0, y: 3, extractor_only: true },
];

const MEDIUM_ROWS: &[&[Spot]] = &[
    &[
        Spot { x: -1, y: 1, extractor_only: false },
        Spot { x: 1, y: 1, extractor_only: false },
    ],
    &[
        Spot { x: -1, y: 2, extractor_only: false },
        Spot { x: 1, y: 2, extractor_only: false },
    ],
    &[
        Spot { x: -1, y: 4, extractor_only: false },
        Spot { x: 1, y: 4, extractor_only: false },
    ],
    &[Spot { x: 1, y: 5, extractor_only: true }],
];

pub fn is_script(name: &str) -> bool {
    name.eq_ignore_ascii_case("Shipwreck Salvaging")
}

pub fn station_enabled(values: &HashMap<String, String>) -> bool {
    flag(values, "UseSalvageStation", false)
}

pub fn take_page(values: &mut HashMap<String, String>) -> Option<String> {
    values.remove("_OpenPage")
}

pub fn show(ui: &mut Ui, values: &mut HashMap<String, String>) {
    ensure_ship(values);
    normalize_crew(values);
    action_tab(ui, values);
    sync_current_items(values);
}

pub fn show_boat(ui: &mut Ui, values: &mut HashMap<String, String>) {
    ensure_ship(values);
    normalize_crew(values);
    boat_tab(ui, values);
    sync_current_items(values);
}

pub fn show_items(ui: &mut Ui, values: &mut HashMap<String, String>) {
    ensure_ship(values);
    normalize_crew(values);
    let ship = current_ship(values);
    let extractor = flag(values, "UseCrystalExtractor", false);
    ui.label("Choose what happens to each item from this shipwreck. Value is the high alch price.");
    item_list(ui, values, ship, extractor);
}

fn action_tab(ui: &mut Ui, values: &mut HashMap<String, String>) {
    let names: Vec<&str> = salvage_data::SHIPS.iter().map(|ship| ship.name).collect();
    let mut ship = text(values, "SelectedShipwreck");
    choose(ui, "salvage-ship", "Shipwreck", "Which shipwreck to salvage.", &mut ship, &names);
    values.insert("SelectedShipwreck".into(), ship);

    check(
        ui,
        "Park between two wrecks",
        "Stand between two wrecks so both can be salvaged without moving as far.",
        values,
        "UseDoubleSalvageSpots",
    );

    let mut crew = text(values, "SelectedCrewmate");
    choose(
        ui,
        "salvage-crew",
        "Crewmate",
        "None leaves the second hook empty. Pick a crewmate to send them to that hook.",
        &mut crew,
        CREW,
    );
    values.insert("SelectedCrewmate".into(), crew.clone());
    values.insert("UseCrewmateSalvaging".into(), if crew == "None" { "false" } else { "true" }.into());

    ui.add_space(16.0);
    if ui.button(RichText::new("Click here to setup your boat").strong()).clicked() {
        values.insert("_OpenPage".into(), "Boat".into());
    }
}

fn boat_tab(ui: &mut Ui, values: &mut HashMap<String, String>) {
    ui.horizontal(|ui| {
        check(ui, "Salvage station", "Sort salvage at the station on the boat.", values, "UseSalvageStation");
        ui.add_space(12.0);
        check(ui, "Sailing boost (keg)", "Drink from the keg when the sailing boost runs out.", values, "UseSailingBoost");
        ui.add_space(12.0);
        check(
            ui,
            "Crystal extractor",
            "Charge and harvest the crystal extractor. Crystal shards are added to this shipwreck's item list.",
            values,
            "UseCrystalExtractor",
        );
    });
    let station = flag(values, "UseSalvageStation", false);
    let keg = flag(values, "UseSailingBoost", false);
    let extractor = flag(values, "UseCrystalExtractor", false);

    let mut hull = text(values, "_BoatHull");
    if hull != "MEDIUM" {
        hull = "SMALL".into();
    }
    ui.horizontal(|ui| {
        if ui.selectable_label(hull == "SMALL", "Small/Skiff Boat").clicked() {
            hull = "SMALL".into();
            values.insert("_BoatMessage".into(), String::new());
        }
        if ui.selectable_label(hull == "MEDIUM", "Medium/Sloop Boat").clicked() {
            hull = "MEDIUM".into();
            values.insert("_BoatMessage".into(), String::new());
        }
        if ui.button("Clear deck").clicked() {
            for facility in ["table", "keg", "extractor"] {
                clear_facility(values, &hull, facility);
            }
            values.insert("_BoatMessage".into(), String::new());
        }
        if station && ui.button(RichText::new("Click here to configure items").strong()).clicked() {
            values.insert("_OpenPage".into(), "Items".into());
        }
    });
    values.insert("_BoatHull".into(), hull.clone());

    ui.label(RichText::new(place_prompt(values, &hull, station, keg, extractor)).strong());
    let message = text(values, "_BoatMessage");
    if !message.is_empty() {
        ui.label(RichText::new(message).color(Color32::from_rgb(220, 150, 90)));
    }
    ui.add_space(4.0);
    paint_boat(ui, values, &hull, station, keg, extractor);
    refresh_configured(values, "SMALL", station, keg, extractor);
    refresh_configured(values, "MEDIUM", station, keg, extractor);
}

fn ensure_ship(values: &mut HashMap<String, String>) {
    let ship = text(values, "SelectedShipwreck");
    if !salvage_data::SHIPS.iter().any(|item| item.name == ship) {
        values.insert("SelectedShipwreck".into(), salvage_data::SHIPS[0].name.into());
    }
}

fn current_ship(values: &HashMap<String, String>) -> &'static ShipLoot {
    let ship = text(values, "SelectedShipwreck");
    salvage_data::SHIPS.iter().find(|item| item.name == ship).unwrap_or(&salvage_data::SHIPS[0])
}

fn normalize_crew(values: &mut HashMap<String, String>) {
    let crew = text(values, "SelectedCrewmate");
    let using_crew = flag(values, "UseCrewmateSalvaging", false);
    if !using_crew || crew.is_empty() || crew.eq_ignore_ascii_case("none") || !CREW.contains(&crew.as_str()) {
        values.insert("SelectedCrewmate".into(), "None".into());
        values.insert("UseCrewmateSalvaging".into(), "false".into());
    }
}

fn sync_current_items(values: &mut HashMap<String, String>) {
    let ship = current_ship(values);
    let extractor = flag(values, "UseCrystalExtractor", false);
    let key = format!("ItemsToKeep_{}", ship.key);
    let original = values.get(&key).cloned().unwrap_or_default();
    let actions = load_actions(ship, &original, extractor);
    let baseline = store_actions(&actions);
    remember_items(values, &key, &original, &baseline, &actions);
}

fn place_prompt(values: &HashMap<String, String>, hull: &str, station: bool, keg: bool, extractor: bool) -> String {
    match next_unplaced(values, hull, station, keg, extractor) {
        Some("keg") => "Click a deck tile to place the keg.".into(),
        Some("extractor") => "Click a deck tile to place the crystal extractor.".into(),
        Some("table") => "Click a deck tile to place the salvage table.".into(),
        _ if !station && !keg && !extractor => "Turn on the facilities your boat has, then click the deck to mark them.".into(),
        _ => "Deck saved. Click a placed facility to move it.".into(),
    }
}

fn next_unplaced(values: &HashMap<String, String>, hull: &str, station: bool, keg: bool, extractor: bool) -> Option<&'static str> {
    for (on, id) in [(keg, "keg"), (extractor, "extractor"), (station, "table")] {
        if on && read_grid(values, hull, id).is_none() {
            return Some(id);
        }
    }
    None
}

fn paint_boat(ui: &mut Ui, values: &mut HashMap<String, String>, hull: &str, station: bool, keg: bool, extractor: bool) {
    let ts = 56.0;
    let medium = hull == "MEDIUM";
    let (min_x, max_x, rows) = if medium { (-1, 1, 6) } else { (0, 0, 4) };
    let bow = 36.0;
    let stern = 28.0;
    let side = 36.0;
    let legend_w = 196.0;
    let grid_h = rows as f32 * ts;
    let height = bow + grid_h + stern;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let origin = Pos2::new(rect.left() + side - min_x as f32 * ts, rect.top() + bow);
    let hover = response.hover_pos().and_then(|pos| tile_at(origin, ts, pos));
    if hover.is_some_and(|(x, y)| spot_at(hull, x, y).is_some()) {
        response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((x, y)) = tile_at(origin, ts, pos) {
                mark_tile(values, hull, x, y, station, keg, extractor);
            }
        }
    }

    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, Color32::from_rgb(43, 43, 43));
    let deck_left = origin.x + min_x as f32 * ts;
    let deck_right = origin.x + (max_x as f32 + 1.0) * ts;
    let deck_top = origin.y;
    let deck_bottom = origin.y + rows as f32 * ts;
    let mid_x = (deck_left + deck_right) * 0.5;
    let hull_pts = vec![
        Pos2::new(mid_x, deck_top - 26.0),
        Pos2::new(deck_left + 16.0, deck_top + 6.0),
        Pos2::new(deck_left - 8.0, deck_top + ts * 0.8),
        Pos2::new(deck_left - 14.0, deck_bottom + 6.0),
        Pos2::new(deck_right + 14.0, deck_bottom + 6.0),
        Pos2::new(deck_right + 8.0, deck_top + ts * 0.8),
        Pos2::new(deck_right - 16.0, deck_top + 6.0),
    ];
    let shadow: Vec<Pos2> = hull_pts.iter().map(|point| *point + Vec2::new(3.0, 3.0)).collect();
    painter.add(Shape::convex_polygon(shadow, Color32::from_rgb(26, 26, 26), Stroke::NONE));
    painter.add(Shape::convex_polygon(
        hull_pts,
        Color32::from_rgb(90, 64, 32),
        Stroke::new(2.0_f32, Color32::from_rgb(16, 32, 48)),
    ));
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(deck_left, deck_top), Pos2::new(deck_right, deck_bottom)),
        2.0,
        Color32::from_rgb(48, 96, 120),
    );
    painter.text(Pos2::new(mid_x, deck_top - 30.0), Align2::CENTER_BOTTOM, "BOW", FontId::proportional(13.0), Color32::WHITE);
    painter.text(
        Pos2::new(mid_x, deck_bottom + 8.0),
        Align2::CENTER_TOP,
        "STERN",
        FontId::proportional(12.0),
        Color32::from_rgb(180, 180, 180),
    );
    if medium {
        painter.text(Pos2::new(deck_left - 8.0, (deck_top + deck_bottom) * 0.5), Align2::RIGHT_CENTER, "Port", FontId::proportional(11.0), Color32::from_rgb(160, 160, 160));
        painter.text(Pos2::new(deck_right + 8.0, (deck_top + deck_bottom) * 0.5), Align2::LEFT_CENTER, "Starb.", FontId::proportional(11.0), Color32::from_rgb(160, 160, 160));
        let mast_y = origin.y + 3.5 * ts;
        painter.line_segment(
            [Pos2::new(deck_left + 8.0, mast_y), Pos2::new(deck_right - 8.0, mast_y)],
            Stroke::new(1.0_f32, Color32::from_rgb(96, 96, 96)),
        );
        painter.text(Pos2::new(mid_x, mast_y), Align2::CENTER_CENTER, "~ mast ~", FontId::proportional(11.0), Color32::from_rgb(140, 140, 140));
        paint_hook(painter, cell_rect(origin, ts, -1, 0), "Hook (P)");
        paint_hook(painter, cell_rect(origin, ts, 1, 0), "Hook (S)");
    } else {
        paint_hook(painter, cell_rect(origin, ts, 0, 0), "Hook");
    }

    for spot in hull_spots(hull) {
        let cell = cell_rect(origin, ts, spot.x, spot.y).shrink(3.0);
        let here = facility_at(values, hull, spot.x, spot.y);
        let (fill, label) = match here {
            Some("keg") => (Color32::from_rgb(0, 104, 184), "Keg"),
            Some("extractor") => (Color32::from_rgb(176, 80, 0), "Extr"),
            Some("table") => (Color32::from_rgb(0, 104, 32), "Salv"),
            _ => (Color32::from_rgb(40, 80, 106), ""),
        };
        painter.rect_filled(cell, 2.0, fill);
        let hovered = hover == Some((spot.x, spot.y));
        painter.rect_stroke(
            cell,
            2.0,
            Stroke::new(if hovered { 2.0_f32 } else { 1.0_f32 }, if here.is_some() { Color32::from_rgb(224, 224, 224) } else { Color32::from_rgb(96, 128, 144) }),
            StrokeKind::Inside,
        );
        if !label.is_empty() {
            painter.text(cell.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0), Color32::WHITE);
        }
    }

    let legend = Rect::from_min_size(Pos2::new(deck_right + 48.0, deck_top), Vec2::new(legend_w, 168.0));
    painter.rect_filled(legend, 4.0, Color32::from_rgb(54, 54, 54));
    painter.rect_stroke(legend, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(80, 80, 80)), StrokeKind::Inside);
    painter.text(legend.left_top() + Vec2::new(10.0, 8.0), Align2::LEFT_TOP, "Legend", FontId::proportional(14.0), Color32::WHITE);
    let swatches = [
        (Color32::from_rgb(80, 96, 128), "Hook (fixed)"),
        (Color32::from_rgb(0, 104, 184), "Keg"),
        (Color32::from_rgb(176, 80, 0), "Crystal extractor"),
        (Color32::from_rgb(0, 104, 32), "Salvage table"),
    ];
    for (index, (color, label)) in swatches.iter().enumerate() {
        let y = legend.top() + 36.0 + index as f32 * 26.0;
        let swatch = Rect::from_min_size(Pos2::new(legend.left() + 10.0, y), Vec2::new(14.0, 14.0));
        painter.rect_filled(swatch, 2.0, *color);
        painter.text(Pos2::new(swatch.right() + 8.0, swatch.center().y), Align2::LEFT_CENTER, *label, FontId::proportional(12.0), Color32::from_rgb(208, 208, 208));
    }
    painter.text(
        legend.left_bottom() + Vec2::new(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        if medium { "Medium / Sloop" } else { "Small / Skiff" },
        FontId::proportional(13.0),
        Color32::WHITE,
    );
}

fn paint_hook(painter: &egui::Painter, cell: Rect, label: &str) {
    let tile = cell.shrink(4.0);
    painter.rect_filled(tile, 2.0, Color32::from_rgb(128, 96, 80));
    painter.rect_stroke(tile, 2.0, Stroke::new(2.0_f32, Color32::from_rgb(90, 58, 58)), StrokeKind::Inside);
    painter.text(tile.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::WHITE);
}

fn mark_tile(values: &mut HashMap<String, String>, hull: &str, x: i32, y: i32, station: bool, keg: bool, extractor: bool) {
    let Some(spot) = spot_at(hull, x, y) else {
        return;
    };
    if let Some(existing) = facility_at(values, hull, x, y) {
        clear_facility(values, hull, existing);
        values.insert("_BoatMessage".into(), String::new());
        return;
    }
    let Some(next) = next_unplaced(values, hull, station, keg, extractor) else {
        values.insert("_BoatMessage".into(), "Everything is placed. Click a facility to move it.".into());
        return;
    };
    if spot.extractor_only && next != "extractor" {
        values.insert("_BoatMessage".into(), "That tile is only for the crystal extractor.".into());
        return;
    }
    for other in ["table", "keg", "extractor"] {
        if facility_at(values, hull, x, y) == Some(other) {
            clear_facility(values, hull, other);
        }
    }
    place_facility(values, hull, next, x, y);
    values.insert("_BoatMessage".into(), String::new());
}

fn hull_spots(hull: &str) -> Vec<&'static Spot> {
    if hull == "MEDIUM" {
        MEDIUM_ROWS.iter().flat_map(|row| row.iter()).collect()
    } else {
        SMALL_SPOTS.iter().collect()
    }
}

fn spot_at(hull: &str, x: i32, y: i32) -> Option<&'static Spot> {
    hull_spots(hull).into_iter().find(|spot| spot.x == x && spot.y == y)
}

fn cell_rect(origin: Pos2, ts: f32, x: i32, y: i32) -> Rect {
    Rect::from_min_size(origin + Vec2::new(x as f32 * ts, y as f32 * ts), Vec2::splat(ts))
}

fn tile_at(origin: Pos2, ts: f32, pos: Pos2) -> Option<(i32, i32)> {
    let x = ((pos.x - origin.x) / ts).floor() as i32;
    let y = ((pos.y - origin.y) / ts).floor() as i32;
    if cell_rect(origin, ts, x, y).shrink(3.0).contains(pos) {
        Some((x, y))
    } else {
        None
    }
}

fn item_list(ui: &mut Ui, values: &mut HashMap<String, String>, ship: &ShipLoot, extractor: bool) {
    let key = format!("ItemsToKeep_{}", ship.key);
    let original = values.get(&key).cloned().unwrap_or_default();
    let baseline = store_actions(&load_actions(ship, &original, extractor));
    let mut actions = load_actions(ship, &original, extractor);
    ui.horizontal(|ui| {
        if ui.button("Drop All").clicked() {
            set_visible(&mut actions, "Drop");
        }
        if ui.button("Keep All").clicked() {
            set_visible(&mut actions, "Keep");
        }
        if ui.button("High Alc All").clicked() {
            set_visible(&mut actions, "HighAlc");
        }
        if ui.button("Restore Defaults").clicked() {
            actions = load_actions(ship, "", extractor);
        }
    });
    ui.add_space(8.0);
    let indexes: Vec<usize> = actions.iter().enumerate().filter(|(_, action)| action.visible).map(|(index, _)| index).collect();
    let columns = 5;
    let column_width = (ui.available_width() / columns as f32) - 8.0;
    for row in indexes.chunks(columns) {
        ui.horizontal_top(|ui| {
            for &index in row {
                let name = actions[index].name.clone();
                let choices = action_choices(&name);
                let mut shown = action_label(&name, &actions[index].action);
                if !choices.iter().any(|choice| choice == &shown) {
                    shown = choices[0].clone();
                    actions[index].action = action_code(&shown);
                }
                let value = salvage_data::high_alch_value(&name);
                let value_text = if value > 0 { format!("{value} gp") } else { " ".to_string() };
                ui.vertical(|ui| {
                    ui.set_width(column_width);
                    ui.label(RichText::new(display_name(&name)).strong());
                    ui.label(RichText::new(value_text).weak().size(11.0));
                    ComboBox::from_id_salt(format!("salvage-act-{}-{name}", ship.key))
                        .width(column_width - 4.0)
                        .selected_text(&shown)
                        .show_ui(ui, |ui| {
                            for choice in &choices {
                                ui.selectable_value(&mut shown, choice.clone(), choice);
                            }
                        });
                });
                actions[index].action = action_code(&shown);
            }
        });
        ui.add_space(6.0);
    }
    remember_items(values, &key, &original, &baseline, &actions);
}

fn remember_items(values: &mut HashMap<String, String>, key: &str, original: &str, baseline: &str, actions: &[ItemAction]) {
    let stored = store_actions(actions);
    // An untouched empty list stays empty so the script keeps applying its own defaults.
    if original.is_empty() && stored == baseline {
        values.insert(key.to_string(), String::new());
    } else {
        values.insert(key.to_string(), stored);
    }
}

fn display_name(name: &str) -> &str {
    match name {
        "Sawmill coupon (wood plank)" => "Sawmill coupon(wood)",
        "Sawmill coupon (oak plank)" => "Sawmill coupon (oak)",
        other => other,
    }
}

struct ItemAction {
    name: String,
    action: String,
    visible: bool,
}

fn load_actions(ship: &ShipLoot, saved: &str, extractor: bool) -> Vec<ItemAction> {
    let saved_map = parse_actions(saved);
    let mut names: Vec<String> = ship.items.iter().map(|item| (*item).to_string()).collect();
    if extractor && !names.iter().any(|name| name == "Crystal shards") {
        names.push("Crystal shards".into());
    }
    names
        .into_iter()
        .map(|name| {
            let action = saved_map
                .iter()
                .find(|(item, _)| item == &name)
                .map(|(_, action)| action.clone())
                .unwrap_or_else(|| default_action(&name));
            ItemAction { visible: !hidden(&ship.name, &name), name, action }
        })
        .collect()
}

fn set_visible(actions: &mut [ItemAction], action: &str) {
    for item in actions.iter_mut().filter(|item| item.visible) {
        if action_choices(&item.name).iter().any(|choice| action_code(choice) == action) {
            item.action = action.to_string();
        }
    }
}

fn parse_actions(saved: &str) -> Vec<(String, String)> {
    saved
        .split(',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (name, action) = part.split_once(':')?;
            Some((name.trim().to_string(), action_code(action.trim())))
        })
        .collect()
}

fn store_actions(actions: &[ItemAction]) -> String {
    actions.iter().map(|item| format!("{}:{}", item.name, item.action)).collect::<Vec<_>>().join(",")
}

fn action_code(label: &str) -> String {
    let lower: String = label.to_ascii_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect();
    if lower.starts_with("highalc") {
        "HighAlc".into()
    } else if lower == "keep" {
        "Keep".into()
    } else if lower == "destroy" {
        "Destroy".into()
    } else {
        "Drop".into()
    }
}

fn action_choices(name: &str) -> Vec<String> {
    if name == "Casket" {
        return vec!["Drop".into()];
    }
    if DESTROY_ITEMS.contains(&name) {
        return vec!["Keep".into(), "Destroy".into()];
    }
    let mut choices = vec!["Drop".into(), "Keep".into()];
    if name != "Coins" && name != "Platinum token" && name != "Salvor's paint" {
        let value = salvage_data::high_alch_value(name);
        if value > 0 {
            choices.push(format!("High Alc ({value}gp)"));
        } else {
            choices.push("High Alc".into());
        }
    }
    choices
}

fn action_label(name: &str, action: &str) -> String {
    if action == "HighAlc" {
        return action_choices(name)
            .into_iter()
            .find(|choice| choice.to_ascii_lowercase().starts_with("high alc"))
            .unwrap_or_else(|| "High Alc".into());
    }
    action.to_string()
}

fn default_action(name: &str) -> String {
    const KEEP: &[&str] = &[
        "Coins",
        "Platinum token",
        "Rune Pouch",
        "Divine Rune Pouch",
        "Nature rune",
        "Snape grass seed",
        "Ironwood seed",
        "Rosewood seed",
        "Camphor seed",
        "Hemp seed",
        "Bronze nails",
        "Iron nails",
        "Steel nails",
        "Mithril nails",
        "Adamantite nails",
        "Rune nails",
        "Dragon nails",
        "Astral rune",
        "Sawmill coupon (wood plank)",
        "Sawmill coupon (oak plank)",
        "Bronze cannonball",
        "Iron cannonball",
        "Steel cannonball",
        "Mithril cannonball",
        "Adamant cannonball",
        "Rune cannonball",
        "Dragon cannonball",
        "Dragon spear",
        "Shield left half",
        "Dragon cannon barrel",
        "Salvor's paint",
        "Boat bottle (empty)",
        "Mouldy block",
        "Mouldy doll",
        "Broken compass",
        "Broken sextant",
        "Smashed mirror",
        "Rusty locket",
        "Dull knife",
        "Rusty coin",
    ];
    if KEEP.contains(&name) {
        "Keep".into()
    } else {
        "Drop".into()
    }
}

fn hidden(ship: &str, name: &str) -> bool {
    if ship != "Merchant shipwreck" {
        return false;
    }
    const HIDDEN: &[&str] = &[
        "Avantoe seed",
        "Kwuarm seed",
        "Snapdragon seed",
        "Cadantine seed",
        "Lantadyme seed",
        "Dwarf weed seed",
        "Torstol seed",
        "Toadflax seed",
        "Irit seed",
        "Belladonna seed",
        "Poison ivy seed",
        "Cactus seed",
        "Potato cactus seed",
        "Ironwood seed",
        "Camphor seed",
        "Uncut opal",
        "Uncut jade",
        "Uncut red topaz",
        "Uncut sapphire",
        "Uncut emerald",
        "Uncut ruby",
        "Silver bar",
        "Tiara",
        "Jug of wine",
        "Spice",
        "Grey wolf fur",
        "Silk",
    ];
    HIDDEN.contains(&name)
}

fn facility_at(values: &HashMap<String, String>, hull: &str, x: i32, y: i32) -> Option<&'static str> {
    for facility in ["table", "keg", "extractor"] {
        if let Some((px, py)) = read_grid(values, hull, facility) {
            if px == x && py == y {
                return Some(facility);
            }
        }
    }
    None
}

fn read_grid(values: &HashMap<String, String>, hull: &str, facility: &str) -> Option<(i32, i32)> {
    let raw = values.get(grid_key(hull, facility))?;
    let (x, y) = parse_pair(raw)?;
    if (x, y) == (0, 0) || x <= -9999 || y <= -9999 {
        return None;
    }
    Some((x, y))
}

fn place_facility(values: &mut HashMap<String, String>, hull: &str, facility: &str, x: i32, y: i32) {
    let (gx, gy) = game_pos(hull, x, y);
    values.insert(grid_key(hull, facility).to_string(), format!("{x},{y}"));
    values.insert(pos_key(hull, facility).to_string(), format!("{gx},{gy}"));
}

fn clear_facility(values: &mut HashMap<String, String>, hull: &str, facility: &str) {
    values.insert(grid_key(hull, facility).to_string(), "0,0".into());
    values.insert(pos_key(hull, facility).to_string(), "0,0".into());
}

fn refresh_configured(values: &mut HashMap<String, String>, hull: &str, station: bool, keg: bool, extractor: bool) {
    let needed = [station, keg, extractor];
    let ids = ["table", "keg", "extractor"];
    let ready = needed.iter().zip(ids).all(|(on, id)| !on || read_grid(values, hull, id).is_some());
    let any = needed.iter().any(|on| *on);
    let key = if hull == "MEDIUM" { "MediumBoat_IsConfigured" } else { "SmallBoat_IsConfigured" };
    values.insert(key.to_string(), if any && ready { "true" } else { "false" }.into());
}

fn grid_key(hull: &str, facility: &str) -> &'static str {
    match (hull, facility) {
        ("MEDIUM", "table") => "MediumBoat_SalvageTablePos_Grid",
        ("MEDIUM", "keg") => "MediumBoat_KegPos_Grid",
        ("MEDIUM", "extractor") => "MediumBoat_CrystalExtractorPos_Grid",
        (_, "keg") => "SmallBoat_KegPos_Grid",
        (_, "extractor") => "SmallBoat_CrystalExtractorPos_Grid",
        _ => "SmallBoat_SalvageTablePos_Grid",
    }
}

fn pos_key(hull: &str, facility: &str) -> &'static str {
    match (hull, facility) {
        ("MEDIUM", "table") => "MediumBoat_SalvageTablePos",
        ("MEDIUM", "keg") => "MediumBoat_KegPos",
        ("MEDIUM", "extractor") => "MediumBoat_CrystalExtractorPos",
        (_, "keg") => "SmallBoat_KegPos",
        (_, "extractor") => "SmallBoat_CrystalExtractorPos",
        _ => "SmallBoat_SalvageTablePos",
    }
}

fn game_pos(hull: &str, x: i32, y: i32) -> (i32, i32) {
    match (hull, x, y) {
        ("SMALL", 0, 1) => (2, 14),
        ("SMALL", 0, 2) => (2, 18),
        ("SMALL", 0, 3) => (2, 20),
        ("MEDIUM", -1, 1) => (-2, 12),
        ("MEDIUM", 1, 1) => (4, 10),
        ("MEDIUM", -1, 2) => (-2, 14),
        ("MEDIUM", 1, 2) => (4, 14),
        ("MEDIUM", -1, 4) => (-2, 20),
        ("MEDIUM", 1, 4) => (4, 20),
        ("MEDIUM", 1, 5) => (4, 24),
        _ => (0, 0),
    }
}

fn parse_pair(raw: &str) -> Option<(i32, i32)> {
    let (x, y) = raw.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn text(values: &HashMap<String, String>, key: &str) -> String {
    values.get(key).cloned().unwrap_or_default()
}

fn flag(values: &HashMap<String, String>, key: &str, default: bool) -> bool {
    match values.get(key).map(String::as_str) {
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => true,
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => false,
        _ => default,
    }
}

fn check(ui: &mut Ui, label: &str, tip: &str, values: &mut HashMap<String, String>, key: &str) -> bool {
    let mut on = flag(values, key, false);
    ui.horizontal(|ui| {
        ui.checkbox(&mut on, label);
        crate::ui::help_tip(ui, tip);
    });
    values.insert(key.to_string(), if on { "true" } else { "false" }.into());
    on
}

fn choose(ui: &mut Ui, salt: &str, label: &str, tip: &str, current: &mut String, options: &[&str]) {
    if options.is_empty() {
        return;
    }
    if !options.contains(&current.as_str()) {
        *current = options[0].to_string();
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong());
        crate::ui::help_tip(ui, tip);
    });
    let shown = current.clone();
    ComboBox::from_id_salt(salt).width(360.0).selected_text(shown).show_ui(ui, |ui| {
        for option in options {
            ui.selectable_value(current, (*option).to_string(), *option);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_not_a_crewmate() {
        assert_eq!(CREW[0], "None");
    }

    #[test]
    fn the_keg_is_placed_before_the_table() {
        let values = HashMap::new();
        assert_eq!(next_unplaced(&values, "SMALL", true, true, false), Some("keg"));
    }

    #[test]
    fn coins_are_kept_and_caskets_are_dropped() {
        assert_eq!(default_action("Coins"), "Keep");
        assert_eq!(default_action("Casket"), "Drop");
        assert_eq!(default_action("Bronze bar"), "Drop");
    }

    #[test]
    fn stern_tile_converts_like_the_script() {
        assert_eq!(game_pos("SMALL", 0, 1), (2, 14));
        assert_eq!(game_pos("MEDIUM", 1, 5), (4, 24));
        assert_eq!(game_pos("MEDIUM", -1, 1), (-2, 12));
    }

    #[test]
    fn saved_high_alc_round_trips() {
        let ship = &salvage_data::SHIPS[0];
        let actions = load_actions(ship, "Bronze bar:HighAlc,Coins:Keep", false);
        let stored = store_actions(&actions);
        assert!(stored.contains("Bronze bar:HighAlc"));
        assert!(stored.contains("Coins:Keep"));
        assert!(stored.contains("Logs:Drop"));
    }
}
