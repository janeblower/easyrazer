//! Colors, fonts and widget looks; the colors are those of `app/src/style.css`.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
pub const PANEL: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
pub const KEY: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
pub const LINE: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
pub const DRIVER: Color32 = Color32::from_rgb(0xd7, 0x54, 0x11);
pub const INK: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0b);
pub const KEY_OFF: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const HS: Color32 = Color32::from_rgb(0x2c, 0xb8, 0xe8);
pub const EDITED: Color32 = Color32::from_rgb(0xd6, 0xa0, 0x2c);
pub const WRITE: Color32 = Color32::from_rgb(0xff, 0xb0, 0x70);
pub const WRITE_LINE: Color32 = Color32::from_rgb(0x8a, 0x5a, 0x20);
pub const DEV: Color32 = Color32::from_rgb(0xa8, 0x7c, 0xf0);
pub const ACCENT: Color32 = Color32::from_rgb(0x44, 0xd6, 0x2c);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xcf, 0x7a);
pub const WARN_BG: Color32 = Color32::from_rgb(0x3a, 0x2a, 0x10);
pub const ERROR: Color32 = Color32::from_rgb(0xe0, 0x44, 0x3c);

pub const RADIUS: u8 = 6;

/// Bundled, so the look does not depend on the fonts a system has.
const REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
/// Subsets of Noto Sans Symbols 2 and Noto Sans Math: the symbols the window uses that Inter lacks.
const SYMBOLS: &[u8] = include_bytes!("../assets/fonts/NotoSansSymbols2-Subset.ttf");
const MATH: &[u8] = include_bytes!("../assets/fonts/NotoSansMath-Subset.ttf");

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    egui_extras::install_image_loaders(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, style);
}

fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    f.font_data.insert("inter".into(), Arc::new(FontData::from_static(REGULAR)));
    f.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(SEMIBOLD)));
    f.font_data.insert("symbols".into(), Arc::new(FontData::from_static(SYMBOLS)));
    f.font_data.insert("math".into(), Arc::new(FontData::from_static(MATH)));
    // egui's own fonts stay behind for the emoji.
    let proportional = f.families.entry(FontFamily::Proportional).or_default();
    for (i, name) in ["inter", "symbols", "math"].into_iter().enumerate() {
        proportional.insert(i, name.into());
    }
    let monospace = f.families.entry(FontFamily::Monospace).or_default();
    monospace.extend(["symbols".to_string(), "math".to_string()]);
    let mut bold = vec!["inter-semibold".to_string()];
    bold.extend(f.families[&FontFamily::Proportional].iter().skip(1).cloned());
    f.families.insert(self::bold(), bold);
    f
}

fn style(s: &mut egui::Style) {
    s.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, FontId::new(15.0, bold())),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    s.spacing.item_spacing = egui::vec2(8.0, 8.0);
    s.spacing.button_padding = egui::vec2(12.0, 6.0);
    s.spacing.interact_size.y = 32.0;
    let v = &mut s.visuals;
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = KEY;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.error_fg_color = ERROR;
    v.warn_fg_color = WARN;
    let radius = CornerRadius::same(RADIUS);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = radius;
        w.bg_stroke = Stroke::new(1.0, LINE);
        w.fg_stroke = Stroke::new(1.0, TEXT);
    }
    v.widgets.inactive.weak_bg_fill = KEY;
    v.widgets.inactive.bg_fill = KEY;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x33, 0x33, 0x33);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x33, 0x33, 0x33);
    v.widgets.active.weak_bg_fill = LINE;
    v.widgets.active.bg_fill = LINE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
}
