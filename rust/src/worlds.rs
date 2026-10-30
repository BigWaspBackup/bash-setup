//! OSRS world list, the same source the Simba Generate Worlds dialog uses.

use std::collections::HashSet;

use eframe::egui::{self, Button, RichText, ScrollArea};

const WORLD_LIST_URL: &str = "https://oldschool.runescape.com/slu";
const LOCATIONS: [&str; 8] = [
    "Germany",
    "United Kingdom",
    "United States",
    "Australia",
    "Brazil",
    "Singapore",
    "Japan",
    "South Africa",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Activity {
    Normal,
    Dangerous,
    Restricted,
    Skill,
}

struct World {
    number: i32,
    players: i32,
    location: String,
    members: bool,
    activity: Activity,
    activity_text: String,
}

pub struct Picker {
    pub open: bool,
    worlds: Vec<World>,
    chosen: HashSet<i32>,
    pending: Option<String>,
    pub target: usize,
    error: String,
    members: bool,
    free: bool,
    locations: [bool; 8],
    normal: bool,
    dangerous: bool,
    restricted: bool,
    skill: bool,
    min_players: String,
    max_players: String,
    exclude: String,
}

impl Default for Picker {
    fn default() -> Self {
        Self {
            open: false,
            worlds: Vec::new(),
            chosen: HashSet::new(),
            pending: None,
            target: 0,
            error: String::new(),
            members: true,
            free: false,
            locations: [true; 8],
            normal: true,
            dangerous: false,
            restricted: false,
            skill: false,
            min_players: "0".into(),
            max_players: "2000".into(),
            exclude: String::new(),
        }
    }
}

impl Picker {
    pub fn open_with(&mut self, current: &str) {
        self.open = true;
        self.pending = Some(current.to_string());
        self.error.clear();
    }

    fn apply_pending(&mut self) {
        let Some(current) = self.pending.take() else {
            return;
        };
        let have = parse_worlds(&current);
        self.members = true;
        self.free = false;
        self.locations = [true; 8];
        self.normal = true;
        self.dangerous = false;
        self.restricted = false;
        self.skill = false;
        if have.is_empty() {
            self.chosen = self.visible_numbers();
            return;
        }
        for world in &self.worlds {
            if !have.contains(&world.number) {
                continue;
            }
            if world.members {
                self.members = true;
            } else {
                self.free = true;
            }
            if let Some(index) = LOCATIONS.iter().position(|name| name.eq_ignore_ascii_case(&world.location)) {
                self.locations[index] = true;
            }
            match world.activity {
                Activity::Normal => self.normal = true,
                Activity::Dangerous => self.dangerous = true,
                Activity::Restricted => self.restricted = true,
                Activity::Skill => self.skill = true,
            }
        }
        self.chosen = have;
    }

    fn visible_numbers(&self) -> HashSet<i32> {
        self.worlds.iter().filter(|world| self.matches(world)).map(|world| world.number).collect()
    }

    fn matches(&self, world: &World) -> bool {
        if world.members && !self.members {
            return false;
        }
        if !world.members && !self.free {
            return false;
        }
        if let Some(index) = LOCATIONS.iter().position(|name| name.eq_ignore_ascii_case(&world.location)) {
            if !self.locations[index] {
                return false;
            }
        }
        let activity_on = match world.activity {
            Activity::Normal => self.normal,
            Activity::Dangerous => self.dangerous,
            Activity::Restricted => self.restricted,
            Activity::Skill => self.skill,
        };
        if !activity_on {
            return false;
        }
        let min = self.min_players.trim().parse::<i32>().unwrap_or(0);
        let max = self.max_players.trim().parse::<i32>().unwrap_or(2000);
        if world.players < min || world.players > max {
            return false;
        }
        let excluded = parse_worlds(&self.exclude);
        !excluded.contains(&world.number)
    }
}

/// Draws the generator. `Some` is the comma-separated list to store.
pub fn show(ctx: &egui::Context, picker: &mut Picker) -> Option<String> {
    if !picker.open {
        return None;
    }
    if picker.worlds.is_empty() && picker.error.is_empty() {
        load_into(picker);
    }
    if picker.pending.is_some() && !picker.worlds.is_empty() {
        picker.apply_pending();
    }

    let mut done = None;
    let mut refresh = false;
    egui::Window::new("Generate worlds")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(760.0);
            ui.label("Tick the worlds you want. Done replaces this account's worlds with the ticked ones in the list.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut picker.members, "Members");
                ui.checkbox(&mut picker.free, "Free-to-play");
            });
            ui.label(RichText::new("Location").strong());
            ui.horizontal_wrapped(|ui| {
                for (index, name) in LOCATIONS.iter().enumerate() {
                    ui.checkbox(&mut picker.locations[index], *name);
                }
            });
            ui.label(RichText::new("Activity").strong());
            ui.horizontal(|ui| {
                ui.checkbox(&mut picker.normal, "Normal");
                ui.checkbox(&mut picker.dangerous, "Dangerous");
                ui.checkbox(&mut picker.restricted, "Restricted");
                ui.checkbox(&mut picker.skill, "Skill total");
            });
            ui.horizontal(|ui| {
                ui.label("Players");
                ui.add(egui::TextEdit::singleline(&mut picker.min_players).desired_width(52.0));
                ui.label("to");
                ui.add(egui::TextEdit::singleline(&mut picker.max_players).desired_width(52.0));
                ui.label("Exclude");
                ui.add(egui::TextEdit::singleline(&mut picker.exclude).desired_width(180.0).hint_text("world numbers"));
                if ui.button("Refresh").clicked() {
                    refresh = true;
                }
            });
            if !picker.error.is_empty() {
                ui.label(RichText::new(&picker.error).color(egui::Color32::from_rgb(220, 120, 110)));
            }
            let visible: Vec<usize> = picker
                .worlds
                .iter()
                .enumerate()
                .filter(|(_, world)| picker.matches(world))
                .map(|(index, _)| index)
                .collect();
            let ticked = visible.iter().filter(|index| picker.chosen.contains(&picker.worlds[**index].number)).count();
            ui.label(RichText::new(format!("{ticked} ticked, {} shown", visible.len())).weak());
            ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                egui::Grid::new("world-rows").num_columns(5).spacing([18.0, 3.0]).striped(true).show(ui, |ui| {
                    ui.label(RichText::new("World").strong());
                    ui.label(RichText::new("Location").strong());
                    ui.label(RichText::new("Type").strong());
                    ui.label(RichText::new("Activity").strong());
                    ui.label(RichText::new("Players").strong());
                    ui.end_row();
                    for index in &visible {
                        let (number, location, members, activity, players) = {
                            let world = &picker.worlds[*index];
                            (world.number, world.location.clone(), world.members, activity_label(world), world.players)
                        };
                        let mut on = picker.chosen.contains(&number);
                        if ui.checkbox(&mut on, number.to_string()).changed() {
                            if on {
                                picker.chosen.insert(number);
                            } else {
                                picker.chosen.remove(&number);
                            }
                        }
                        ui.label(location);
                        ui.label(if members { "Members" } else { "Free" });
                        ui.label(activity);
                        ui.label(players.to_string());
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Select shown").clicked() {
                    for index in &visible {
                        picker.chosen.insert(picker.worlds[*index].number);
                    }
                }
                if ui.button("Clear shown").clicked() {
                    for index in &visible {
                        picker.chosen.remove(&picker.worlds[*index].number);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        picker.open = false;
                        picker.pending = None;
                    }
                    if ui.add_enabled(ticked > 0, Button::new("Done")).clicked() {
                        let mut numbers: Vec<i32> = visible
                            .iter()
                            .map(|index| picker.worlds[*index].number)
                            .filter(|number| picker.chosen.contains(number))
                            .collect();
                        numbers.sort_unstable();
                        done = Some(numbers.iter().map(|number| number.to_string()).collect::<Vec<_>>().join(", "));
                        picker.open = false;
                    }
                });
            });
        });
    if refresh {
        picker.worlds.clear();
        picker.error.clear();
        load_into(picker);
        if picker.pending.is_some() && !picker.worlds.is_empty() {
            picker.apply_pending();
        }
    }
    done
}

fn activity_label(world: &World) -> String {
    if world.activity_text.is_empty() || world.activity_text == "-" {
        "Normal".into()
    } else {
        world.activity_text.clone()
    }
}

fn load_into(picker: &mut Picker) {
    match fetch_worlds() {
        Ok(worlds) => picker.worlds = worlds,
        Err(()) => picker.error = "Could not load the world list. Check your connection and press Refresh.".into(),
    }
}

fn fetch_worlds() -> Result<Vec<World>, ()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| ())?;
    let page = client
        .get(WORLD_LIST_URL)
        .header("User-Agent", "BASH launcher")
        .send()
        .map_err(|_| ())?
        .text()
        .map_err(|_| ())?;
    let worlds = parse_world_page(&page);
    if worlds.is_empty() { Err(()) } else { Ok(worlds) }
}

fn parse_worlds(text: &str) -> HashSet<i32> {
    text.split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<i32>().ok())
        .filter(|world| *world > 0)
        .collect()
}

fn parse_world_page(html: &str) -> Vec<World> {
    let mut worlds = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<tr") {
        rest = &rest[start..];
        let Some(end) = rest.find("</tr>") else {
            break;
        };
        let row = &rest[..end];
        rest = &rest[end + 5..];
        if row.contains("world=") {
            if let Some(world) = parse_row(row) {
                worlds.push(world);
            }
        }
    }
    worlds.sort_by_key(|world| world.number);
    worlds.dedup_by_key(|world| world.number);
    worlds
}

fn parse_row(row: &str) -> Option<World> {
    let number = world_number(row)?;
    if number == 400 || number == 401 {
        return None;
    }
    let cells = table_cells(row);
    if cells.len() < 4 {
        return None;
    }
    let players = first_number(&cells[1])?;
    let location = cells[2].clone();
    let members = !cells[3].to_ascii_lowercase().contains("free");
    let activity_text = cells.get(4).cloned().unwrap_or_default();
    Some(World {
        number,
        players,
        location,
        members,
        activity: classify_activity(&activity_text),
        activity_text,
    })
}

fn world_number(row: &str) -> Option<i32> {
    let start = row.find("world=")? + "world=".len();
    let digits: String = row[start..].chars().take_while(|ch| ch.is_ascii_digit()).collect();
    let number = digits.parse::<i32>().ok()?;
    if number > 0 { Some(number) } else { None }
}

fn first_number(text: &str) -> Option<i32> {
    let digits: String = text.chars().skip_while(|ch| !ch.is_ascii_digit()).take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn table_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut rest = row;
    while let Some(start) = rest.find("<td") {
        rest = &rest[start..];
        let Some(end) = rest.find("</td>") else {
            break;
        };
        cells.push(strip_tags(&rest[..end]));
        rest = &rest[end + 5..];
    }
    cells
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for ch in text.chars() {
        if ch == '<' {
            inside = true;
            continue;
        }
        if ch == '>' {
            inside = false;
            continue;
        }
        if !inside {
            out.push(ch);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn classify_activity(text: &str) -> Activity {
    let upper = text.to_ascii_uppercase();
    if ["PVP", "DEADMAN", "RISK"].iter().any(|word| upper.contains(word)) {
        return Activity::Dangerous;
    }
    if ["BOUNTY", "LEAGUE", "BETA", "SPEEDRUNNING", "FRESH", "TOURNAMENT", "LOCKED", "DEVELOPER"]
        .iter()
        .any(|word| upper.contains(word))
    {
        return Activity::Restricted;
    }
    if upper.contains("SKILL TOTAL") {
        return Activity::Skill;
    }
    Activity::Normal
}

#[cfg(test)]
mod tests {
    use super::parse_world_page;

    #[test]
    fn parses_a_world_row() {
        let html = r#"
            <tr class='server-list__row'>
                <td class='server-list__row-cell'><a href='https://oldschool.runescape.com/game?world=302'>Old School 2</a></td>
                <td class='server-list__row-cell'>100 players</td>
                <td class='server-list__row-cell server-list__row-cell--GB'>United Kingdom</td>
                <td class='server-list__row-cell'>Members</td>
                <td class='server-list__row-cell'>-</td>
            </tr>
            <tr class='server-list__row server-list__row--pvp'>
                <td class='server-list__row-cell'><a href='https://oldschool.runescape.com/game?world=577'>Old School 277</a></td>
                <td class='server-list__row-cell'>78 players</td>
                <td class='server-list__row-cell'>United States</td>
                <td class='server-list__row-cell'>Free</td>
                <td class='server-list__row-cell'>PvP World</td>
            </tr>
        "#;
        let worlds = parse_world_page(html);
        assert_eq!(worlds.len(), 2);
        assert_eq!(worlds[0].number, 302);
        assert!(worlds[0].members);
        assert_eq!(worlds[0].location, "United Kingdom");
        assert_eq!(worlds[0].players, 100);
        assert_eq!(worlds[1].number, 577);
        assert!(!worlds[1].members);
    }
}
