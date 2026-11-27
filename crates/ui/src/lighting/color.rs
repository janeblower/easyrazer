//! Picking a color: a swatch button opening a saturation/brightness square, a hue strip, presets
//! and a hex field.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use razer_core::lighting::Rgb;

use crate::theme;

/// Hue in degrees, saturation and value from 0 to 1.
pub type Hsv = [f32; 3];

const PRESETS: [Rgb; 12] = [
    [0xff, 0x00, 0x00],
    [0xff, 0x80, 0x00],
    [0xff, 0xff, 0x00],
    [0x44, 0xd6, 0x2c],
    [0x00, 0xff, 0x80],
    [0x00, 0xff, 0xff],
    [0x00, 0x40, 0xff],
    [0x80, 0x00, 0xff],
    [0xff, 0x00, 0xff],
    [0xff, 0x40, 0x80],
    [0xff, 0xff, 0xff],
    [0xff, 0xc0, 0x80],
];
pub const DEFAULT: Rgb = [0x44, 0xd6, 0x2c];
/// Arrow keys move the point across the square by this much.
const STEP: f32 = 0.02;
const AREA: f32 = 200.0;

pub fn hex_of(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub fn rgb_of_hex(text: &str) -> Option<Rgb> {
    let t = text.trim().trim_start_matches('#');
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

pub fn hsv_of([r, g, b]: Rgb) -> Hsv {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let mut h = 0.0;
    if d > 0.0 {
        h = if max == r {
            ((g - b) / d + 6.0) % 6.0
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
    }
    [h * 60.0, if max == 0.0 { 0.0 } else { d / max }, max / 255.0]
}

pub fn rgb_of_hsv([h, s, v]: Hsv) -> Rgb {
    let f = |n: f32| {
        let k = (n + h / 60.0) % 6.0;
        (255.0 * (v - v * s * k.min(4.0 - k).clamp(0.0, 1.0))).round() as u8
    };
    [f(5.0), f(3.0), f(1.0)]
}

pub fn color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The picker's own state, kept by id: grey and black carry no hue, and the hue must not jump to red.
#[derive(Clone, Copy)]
struct Memory {
    hsv: Hsv,
    /// Color to come back to after "no color" is switched off again.
    last: Rgb,
}

/// A color button; `None` is "no color". Returns the color picked this frame.
pub fn picker(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: Option<Rgb>, enabled: bool, t: &dyn Fn(&str) -> String) -> Option<Option<Rgb>> {
    let id = ui.make_persistent_id(id);
    let mut mem = ui.data(|d| d.get_temp::<Memory>(id)).unwrap_or(Memory { hsv: hsv_of(value.unwrap_or(DEFAULT)), last: value.unwrap_or(DEFAULT) });
    if let Some(v) = value {
        mem.last = v;
        if rgb_of_hsv(mem.hsv) != v {
            mem.hsv = hsv_of(v);
        }
    }
    let label = value.map_or(t("picker.none"), hex_of);
    let button = ui.add_enabled_ui(enabled, |ui| {
        let r = ui.add(egui::Button::new(egui::RichText::new(format!("      {label}")).monospace().small()).min_size(vec2(100.0, 32.0)));
        let sw = Rect::from_min_size(r.rect.left_center() + vec2(5.0, -12.0), vec2(24.0, 24.0));
        ui.painter().rect_filled(sw, 4.0, color32(mem.last));
        if value.is_none() {
            ui.painter().line_segment([sw.left_bottom(), sw.right_top()], Stroke::new(2.0, theme::ERROR));
        }
        r.on_hover_text(if value.is_some() { t("picker.color") } else { t("picker.none") })
    });
    let mut out = None;
    egui::Popup::from_toggle_button_response(&button.inner)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(AREA)
        .show(|ui| {
            if let Some(hsv) = area(ui, mem.hsv) {
                mem.hsv = hsv;
                out = Some(Some(rgb_of_hsv(hsv)));
            }
            if let Some(h) = hue(ui, mem.hsv[0]) {
                mem.hsv = [h, if mem.hsv[1] == 0.0 { 1.0 } else { mem.hsv[1] }, if mem.hsv[2] == 0.0 { 1.0 } else { mem.hsv[2] }];
                out = Some(Some(rgb_of_hsv(mem.hsv)));
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for c in PRESETS {
                    let (rect, r) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    ui.painter().rect_filled(rect, 4.0, color32(c));
                    if value == Some(c) {
                        ui.painter().rect_stroke(rect.expand(1.0), 5.0, Stroke::new(2.0, theme::TEXT), egui::StrokeKind::Outside);
                    }
                    if r.on_hover_text(hex_of(c)).clicked() {
                        mem.hsv = hsv_of(c);
                        out = Some(Some(c));
                    }
                }
            });
            ui.horizontal(|ui| {
                let key = id.with("hex");
                let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| hex_of(value.unwrap_or(mem.last)));
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).font(egui::TextStyle::Monospace)).on_hover_text(t("picker.hex"));
                if r.lost_focus() {
                    if let Some(c) = rgb_of_hex(&text) {
                        mem.hsv = hsv_of(c);
                        out = Some(Some(c));
                    }
                    ui.data_mut(|d| d.remove::<String>(key));
                } else if r.has_focus() {
                    ui.data_mut(|d| d.insert_temp(key, text));
                }
                if ui.button(t("picker.noColor")).clicked() {
                    out = Some(if value.is_some() { None } else { Some(mem.last) });
                }
            });
        });
    ui.data_mut(|d| d.insert_temp(id, mem));
    out
}

/// Saturation across, brightness up.
fn area(ui: &mut Ui, hsv: Hsv) -> Option<Hsv> {
    let (rect, r) = ui.allocate_exact_size(vec2(AREA, 150.0), Sense::click_and_drag());
    let corner = |s: f32, v: f32| color32(rgb_of_hsv([hsv[0], s, v]));
    let mut mesh = Mesh::default();
    for (pos, c) in [(rect.left_top(), corner(0.0, 1.0)), (rect.right_top(), corner(1.0, 1.0)), (rect.right_bottom(), corner(1.0, 0.0)), (rect.left_bottom(), corner(0.0, 0.0))] {
        mesh.vertices.push(Vertex { pos, uv: Pos2::ZERO, color: c });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    ui.painter().add(mesh);
    let dot = pos2(rect.left() + hsv[1] * rect.width(), rect.top() + (1.0 - hsv[2]) * rect.height());
    ui.painter().circle_stroke(dot, 6.0, Stroke::new(2.0, Color32::WHITE));
    if (r.dragged() || r.clicked())
        && let Some(p) = r.interact_pointer_pos()
    {
        r.request_focus();
        return Some([hsv[0], ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0), (1.0 - (p.y - rect.top()) / rect.height()).clamp(0.0, 1.0)]);
    }
    if r.has_focus() {
        let (dx, dy) = ui.input(|i| {
            let k = |key| f32::from(i.key_pressed(key));
            (k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft), k(egui::Key::ArrowUp) - k(egui::Key::ArrowDown))
        });
        if dx != 0.0 || dy != 0.0 {
            return Some([hsv[0], (hsv[1] + dx * STEP).clamp(0.0, 1.0), (hsv[2] + dy * STEP).clamp(0.0, 1.0)]);
        }
    }
    None
}

fn hue(ui: &mut Ui, h: f32) -> Option<f32> {
    let (rect, r) = ui.allocate_exact_size(vec2(AREA, 14.0), Sense::click_and_drag());
    let mut mesh = Mesh::default();
    let steps = 12;
    for i in 0..=steps {
        let f = i as f32 / steps as f32;
        let c = color32(rgb_of_hsv([f * 360.0, 1.0, 1.0]));
        let x = rect.left() + f * rect.width();
        mesh.vertices.push(Vertex { pos: pos2(x, rect.top()), uv: Pos2::ZERO, color: c });
        mesh.vertices.push(Vertex { pos: pos2(x, rect.bottom()), uv: Pos2::ZERO, color: c });
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.indices.extend([b - 2, b - 1, b + 1, b - 2, b + 1, b]);
        }
    }
    ui.painter().add(mesh);
    let x = rect.left() + h / 360.0 * rect.width();
    ui.painter().rect_stroke(Rect::from_center_size(pos2(x, rect.center().y), vec2(6.0, 18.0)), 2.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Middle);
    if (r.dragged() || r.clicked())
        && let Some(p) = r.interact_pointer_pos()
    {
        return Some(((p.x - rect.left()) / rect.width()).clamp(0.0, 359.0 / 360.0) * 360.0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(rgb_of_hex("#44d62c"), Some([0x44, 0xd6, 0x2c]));
        assert_eq!(rgb_of_hex("44D62C"), Some([0x44, 0xd6, 0x2c]));
        assert_eq!(rgb_of_hex("#12345"), None);
        assert_eq!(hex_of([0x44, 0xd6, 0x2c]), "#44D62C");
    }

    #[test]
    fn hsv_round_trips_the_presets() {
        for c in PRESETS {
            assert_eq!(rgb_of_hsv(hsv_of(c)), c, "{}", hex_of(c));
        }
    }

    #[test]
    fn grey_has_no_saturation() {
        let [_, s, v] = hsv_of([0x80, 0x80, 0x80]);
        assert_eq!(s, 0.0);
        assert!((v - 0x80 as f32 / 255.0).abs() < 1e-6);
    }
}
