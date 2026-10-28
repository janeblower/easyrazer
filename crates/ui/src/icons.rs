//! The window's icons, the same SVGs as `app/src/assets/icons`.

use egui::{Color32, Image, ImageSource};

macro_rules! icons {
    ($($name:ident = $file:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        #[allow(non_camel_case_types, dead_code)]
        pub enum Icon { $($name),* }

        const ALL: &[(Icon, &str, &[u8])] = &[
            $((Icon::$name, concat!("bytes://icon/", $file, ".svg"),
               include_bytes!(concat!("../../../app/src/assets/icons/", $file, ".svg")))),*
        ];
    };
}

icons!(
    ban = "ban",
    brightness_down = "brightness_down",
    brightness_up = "brightness_up",
    chip = "chip",
    cog = "cog",
    down = "down",
    game_mode = "game_mode",
    gamepad_toggle = "gamepad_toggle",
    keyboard = "keyboard",
    macro_ = "macro",
    macro_led = "macro_led",
    mouse = "mouse",
    music = "music",
    next_profile = "next_profile",
    plug = "plug",
    plug_off = "plug_off",
    random = "random",
    remove = "remove",
    revert = "revert",
    rust = "rust",
    shield = "shield",
    sleep = "sleep",
    snap_tap = "snap_tap",
    up = "up",
    write = "write",
);

/// Registers every icon; drawn white, so a tint gives it any color.
pub fn install(ctx: &egui::Context) {
    for (_, uri, svg) in ALL {
        let white = String::from_utf8_lossy(svg).replace("currentColor", "#ffffff");
        ctx.include_bytes(*uri, white.into_bytes());
    }
}

pub fn image(icon: Icon, size: f32, color: Color32) -> Image<'static> {
    let (_, uri, _) = ALL[icon as usize];
    Image::new(ImageSource::Uri(uri.into())).fit_to_exact_size(egui::vec2(size, size)).tint(color)
}

/// Draws an icon straight with a painter, for widgets that paint their own content.
pub fn paint(painter: &egui::Painter, icon: Icon, rect: egui::Rect, color: Color32) {
    let img = image(icon, rect.width(), color);
    if let Ok(egui::load::TexturePoll::Ready { texture }) = img.load_for_size(painter.ctx(), rect.size()) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture.id, rect, uv, color);
    }
}
