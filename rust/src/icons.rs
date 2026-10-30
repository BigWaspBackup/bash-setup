//! Skill icons from the old launcher (`BashLib/utils/assets/activities`).
//! Embedded so the window does not depend on that folder being present.

use std::{collections::HashMap, io::Cursor};

use egui::TextureHandle;
use image::ImageReader;

const ACTIVITIES: &[(&str, &[u8])] = &[
    ("agility", include_bytes!("../assets/activities/agility.png")),
    ("all", include_bytes!("../assets/activities/all.png")),
    ("bossing", include_bytes!("../assets/activities/bossing.png")),
    ("combat", include_bytes!("../assets/activities/combat.png")),
    ("construction", include_bytes!("../assets/activities/construction.png")),
    ("cooking", include_bytes!("../assets/activities/cooking.png")),
    ("crafting", include_bytes!("../assets/activities/crafting.png")),
    ("farming", include_bytes!("../assets/activities/farming.png")),
    ("firemaking", include_bytes!("../assets/activities/firemaking.png")),
    ("fishing", include_bytes!("../assets/activities/fishing.png")),
    ("fletching", include_bytes!("../assets/activities/fletching.png")),
    ("herblore", include_bytes!("../assets/activities/herblore.png")),
    ("hunter", include_bytes!("../assets/activities/hunter.png")),
    ("magic", include_bytes!("../assets/activities/magic.png")),
    ("minigame", include_bytes!("../assets/activities/minigame.png")),
    ("mining", include_bytes!("../assets/activities/mining.png")),
    ("prayer", include_bytes!("../assets/activities/prayer.png")),
    ("ranged", include_bytes!("../assets/activities/ranged.png")),
    ("runecraft", include_bytes!("../assets/activities/runecraft.png")),
    ("sailing", include_bytes!("../assets/activities/sailing.png")),
    ("slayer", include_bytes!("../assets/activities/slayer.png")),
    ("smithing", include_bytes!("../assets/activities/smithing.png")),
    ("thieving", include_bytes!("../assets/activities/thieving.png")),
    ("utility", include_bytes!("../assets/activities/utility.png")),
    ("woodcutting", include_bytes!("../assets/activities/woodcutting.png")),
];

pub fn load(ctx: &egui::Context) -> HashMap<String, TextureHandle> {
    let options = egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: None,
    };
    let mut icons = HashMap::new();
    for (id, bytes) in ACTIVITIES {
        let Ok(img) = ImageReader::new(Cursor::new(*bytes)).with_guessed_format() else {
            continue;
        };
        let Ok(img) = img.decode() else { continue };
        let rgba = img.into_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        icons.insert((*id).to_string(), ctx.load_texture(*id, color, options));
    }
    icons
}
