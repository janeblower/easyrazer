//! The gamepad drawn as controls the keys can be bound to, moving with the keys' live depth.

use std::collections::BTreeMap;

use easyrazer_backend::gamepad::{ANALOG_CONTROLS, Config};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use super::shape;
use crate::theme;

/// Every control a key can drive, in the order the editor lists them.
pub const BINDINGS: [&str; 24] = [
    "left_up", "left_down", "left_left", "left_right", "left_click", "right_up", "right_down", "right_left", "right_right", "right_click", "a", "b", "x", "y", "lb", "rb", "lt", "rt",
    "back", "start", "dpad_up", "dpad_down", "dpad_left", "dpad_right",
];

pub fn analog(id: &str) -> bool {
    ANALOG_CONTROLS.contains(&id)
}

/// Equal for the window: a missing maximum depth is 100%.
pub fn same(a: &Config, b: &Config) -> bool {
    let depth = |c: &Config, id: &str| c.max_depth.get(id).copied().unwrap_or(100);
    a.enabled == b.enabled && BINDINGS.iter().all(|id| a.bindings.get(*id) == b.bindings.get(*id)) && ANALOG_CONTROLS.iter().all(|id| depth(a, id) == depth(b, id))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stick {
    pub x: f32,
    pub y: f32,
}

pub struct State {
    pub buttons: BTreeMap<&'static str, bool>,
    pub left: Stick,
    pub right: Stick,
    pub lt: f32,
    pub rt: f32,
}

/// What the pad reports for the keys' depth: analog controls reach full at their maximum depth.
pub fn state(c: &Config, depth: &[u8]) -> State {
    let value = |id: &str| {
        let raw = c.bindings.get(id).and_then(|k| depth.get(*k as usize)).copied().unwrap_or(0);
        let maximum = if analog(id) { (255.0 * f32::from(c.max_depth.get(id).copied().unwrap_or(100).clamp(1, 100)) / 100.0).round() } else { 255.0 };
        (f32::from(raw) * 255.0 / maximum).round().min(255.0) / 255.0
    };
    State {
        buttons: BINDINGS.iter().map(|id| (*id, value(id) >= if analog(id) { 1.0 } else { 128.0 / 255.0 })).collect(),
        left: Stick { x: value("left_right") - value("left_left"), y: value("left_down") - value("left_up") },
        right: Stick { x: value("right_right") - value("right_left"), y: value("right_down") - value("right_up") },
        lt: value("lt"),
        rt: value("rt"),
    }
}

pub struct StickView {
    pub x: f32,
    pub y: f32,
    pub magnitude: f32,
    pub angle: i32,
    pub opacity: f32,
}

pub fn stick_view(s: Stick) -> StickView {
    let length = s.x.hypot(s.y);
    let magnitude = length.min(1.0);
    StickView {
        x: s.x / length.max(1.0),
        y: s.y / length.max(1.0),
        magnitude,
        angle: if length > 0.0 { ((-s.y).atan2(s.x).to_degrees().round() as i32 + 360) % 360 } else { 0 },
        opacity: if magnitude <= 0.01 { 0.0 } else { ((magnitude - 0.01) / 0.15).min(1.0) },
    }
}

/// The first key pressed past half its travel since the last look.
pub fn captured_key(previous: &[u8], depth: &[u8], keys: &[u8]) -> Option<u8> {
    let at = |d: &[u8], k: u8| d.get(k as usize).copied().unwrap_or(0);
    keys.iter().copied().find(|k| at(previous, *k) < 128 && at(depth, *k) >= 128)
}

const VIEW: Vec2 = vec2(800.0, 690.0);
const SHELL: &str = "M267 252 Q207 256 170 294 Q132 333 108 409 Q73 518 92 613 Q99 655 126 654 Q142 654 165 632 L251 585 Q270 567 303 567 L497 567 Q530 567 549 585 L635 632 Q658 654 674 654 Q701 655 708 613 Q727 518 692 409 Q668 333 630 294 Q593 256 533 252 Q400 240 267 252 Z";
const SHOULDER: &str = "M158 142 C193 140 225 136 251 128 C264 140 279 148 299 146 V211 H265 C210 211 170 188 158 142 Z M164 299 C166 276 180 260 204 250 C231 237 250 236 267 247 C278 254 287 254 301 254 V340 H164 Z";
const TRIGGER: &str = "M159 136 L159 92 Q160 47 193 44 Q239 40 249 124 Z";
const FACES: [(&str, f32, f32); 4] = [("y", 570.0, 302.0), ("x", 522.0, 350.0), ("b", 618.0, 350.0), ("a", 570.0, 398.0)];
const DPAD: [(&str, f32, f32, f32); 4] = [("dpad_up", 310.0, 466.0, 0.0), ("dpad_down", 310.0, 534.0, 180.0), ("dpad_left", 276.0, 500.0, 270.0), ("dpad_right", 344.0, 500.0, 90.0)];
const STICKS: [(&str, f32, f32, &str); 2] = [("left", 230.0, 350.0, "L"), ("right", 490.0, 500.0, "R")];

/// Maps the drawing's 800 × 690 view box into a rect, keeping its proportions.
struct View {
    origin: Pos2,
    scale: f32,
}

impl View {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    fn all(&self, pts: &[Pos2], mirror: bool) -> Vec<Pos2> {
        pts.iter().map(|q| self.p(if mirror { VIEW.x - q.x } else { q.x }, q.y)).collect()
    }
}

fn look(selected: bool, pressed: bool, hovered: bool) -> (Color32, Stroke) {
    let fill = if pressed { theme::ACCENT.gamma_multiply(0.28).blend(theme::KEY) } else { theme::KEY };
    let stroke = if selected || pressed {
        Stroke::new(2.5, theme::ACCENT)
    } else if hovered {
        Stroke::new(1.6, theme::TEXT)
    } else {
        Stroke::new(1.6, Color32::from_gray(0x66))
    };
    (fill, stroke)
}

/// Draws the controller; returns the control clicked this frame.
pub fn show(ui: &mut Ui, config: &Config, depth: &[u8], selected: Option<&str>, enabled: bool) -> Option<&'static str> {
    let avail = ui.available_size();
    let scale = (avail.x / VIEW.x).min((avail.y.max(360.0)) / VIEW.y);
    let (rect, response) = ui.allocate_exact_size(VIEW * scale, if enabled { Sense::click() } else { Sense::hover() });
    let v = View { origin: rect.min, scale };
    let p = ui.painter_at(rect.expand(8.0));
    let s = state(config, depth);
    let hover = response.hover_pos();
    let mut hits: Vec<(&'static str, Vec<Pos2>)> = Vec::new();
    let is = |id: &str| selected == Some(id);
    let font = |size: f32| FontId::proportional(size * scale.max(0.6));

    // Shoulders and triggers, the right ones mirrored.
    for (bumper, trigger, mirror, x_label, value) in [("lb", "lt", false, 229.0, s.lt), ("rb", "rt", true, 571.0, s.rt)] {
        let paths: Vec<Vec<Pos2>> = shape::parse(SHOULDER).iter().map(|pts| v.all(pts, mirror)).collect();
        let hovered = hover.is_some_and(|h| paths.iter().any(|pts| shape::contains(pts, h)));
        let (fill, stroke) = look(is(bumper), s.buttons[bumper], hovered);
        for pts in &paths {
            p.extend(shape::filled(pts, fill, stroke));
            hits.push((static_id(bumper), pts.clone()));
        }
        p.text(v.p(x_label, 181.0), Align2::CENTER_CENTER, bumper.to_uppercase(), font(14.0), theme::TEXT);
        let tri = v.all(&shape::parse(TRIGGER)[0], mirror);
        let hovered = hover.is_some_and(|h| shape::contains(&tri, h));
        let (fill, stroke) = look(is(trigger), s.buttons[trigger], hovered);
        p.extend(shape::filled(&tri, fill, stroke));
        if value > 0.0 {
            let top = v.p(0.0, 140.0 - value * 100.0).y;
            let clip = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), rect.bottom()));
            for s in shape::filled(&tri, theme::ACCENT.gamma_multiply(0.6), Stroke::NONE) {
                p.with_clip_rect(clip).add(s);
            }
        }
        hits.push((static_id(trigger), tri));
        let tx = if mirror { VIEW.x - 202.0 } else { 202.0 };
        p.text(v.p(tx, 99.0), Align2::CENTER_CENTER, trigger.to_uppercase(), font(14.0), theme::TEXT);
        // The ruler beside the trigger: its depth in percent.
        let rx = if mirror { VIEW.x - 147.0 } else { 147.0 };
        let track = Rect::from_min_max(v.p(rx - 2.0, 61.0), v.p(rx + 2.0, 133.0));
        p.rect_filled(track, 0.0, theme::LINE);
        p.rect_filled(Rect::from_min_max(v.p(rx - 2.0, 133.0 - value * 72.0), v.p(rx + 2.0, 133.0)), 0.0, theme::ACCENT);
        let lx = if mirror { VIEW.x - 139.0 } else { 139.0 };
        p.text(v.p(lx, 44.0), Align2::CENTER_CENTER, format!("{}%", (value * 100.0).round()), font(12.0), theme::TEXT);
    }

    let shell = v.all(&shape::parse(SHELL)[0], false);
    p.extend(shape::filled(&shell, theme::PANEL, Stroke::new(1.8, theme::LINE)));

    for (id, x, y) in FACES {
        let c = v.p(x, y);
        let r = 22.0 * scale;
        let hovered = hover.is_some_and(|h| h.distance(c) <= r);
        let (fill, stroke) = look(is(id), s.buttons[id], hovered);
        p.circle(c, r, fill, stroke);
        p.text(c, Align2::CENTER_CENTER, id.to_uppercase(), font(14.0), theme::TEXT);
        hits.push((static_id(id), circle(c, r)));
    }
    for (i, id) in ["back", "start"].into_iter().enumerate() {
        let c = v.p(350.0 + i as f32 * 100.0, 364.0);
        let r = 16.0 * scale;
        let hovered = hover.is_some_and(|h| h.distance(c) <= r);
        let (fill, stroke) = look(is(id), s.buttons[id], hovered);
        p.circle(c, r, fill, stroke);
        let glyph = Stroke::new(1.5, theme::MUTED);
        for dy in [-5.0, 0.0, 5.0] {
            p.line_segment([c + vec2(-7.0, dy) * scale, c + vec2(7.0, dy) * scale], glyph);
        }
        hits.push((static_id(id), circle(c, r)));
    }
    for (id, x, y, angle) in DPAD {
        let r = Rect::from_center_size(v.p(x, y), Vec2::splat(30.0 * scale));
        let hovered = hover.is_some_and(|h| r.contains(h));
        let (fill, stroke) = look(is(id), s.buttons[id], hovered);
        p.rect(r, 3.0, fill, stroke, egui::StrokeKind::Middle);
        let rot = egui::emath::Rot2::from_angle(angle.to_radians());
        let chevron: Vec<Pos2> = [vec2(-6.0, 3.0), vec2(0.0, -3.0), vec2(6.0, 3.0)].iter().map(|d| r.center() + rot * (*d * scale)).collect();
        p.add(Shape::line(chevron, Stroke::new(1.5, theme::MUTED)));
        hits.push((static_id(id), vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]));
    }
    for (id, cx, cy, label) in STICKS {
        let axes = if id == "left" { s.left } else { s.right };
        let sv = stick_view(axes);
        let c = v.p(cx, cy);
        let chosen = selected.is_some_and(|sel| sel == id || sel.starts_with(&format!("{id}_")));
        let pressed = s.buttons[if id == "left" { "left_click" } else { "right_click" }];
        let hovered = hover.is_some_and(|h| h.distance(c) <= 46.0 * scale);
        let (fill, stroke) = look(chosen, pressed, hovered);
        p.circle(c, 46.0 * scale, theme::BG, stroke);
        p.circle_stroke(c, 37.0 * scale, Stroke::new(1.6, Color32::from_gray(0x66)));
        if sv.opacity > 0.0 {
            telemetry(&p, c, scale, axes, &sv);
        }
        let knob = c + vec2(sv.x, sv.y) * 29.0 * scale;
        p.circle(knob, 25.0 * scale, fill, stroke);
        p.circle_stroke(knob, 19.0 * scale, Stroke::new(1.0, theme::LINE));
        p.text(knob, Align2::CENTER_CENTER, label, font(14.0), theme::TEXT);
        hits.push((static_id(id), circle(c, 46.0 * scale)));
    }
    if !response.clicked() {
        return None;
    }
    let at = response.interact_pointer_pos()?;
    hits.iter().rev().find(|(_, pts)| shape::contains(pts, at)).map(|(id, _)| *id)
}

/// The stick's dial with its axes, angle and displacement, shown while it moves.
fn telemetry(p: &egui::Painter, c: Pos2, scale: f32, axes: Stick, sv: &StickView) {
    let a = sv.opacity;
    let line = Stroke::new(1.0, theme::MUTED.gamma_multiply(a));
    p.circle_stroke(c, 73.0 * scale, Stroke::new(1.0, theme::LINE.gamma_multiply(a)));
    for n in 0..48 {
        let angle = (n as f32 * 7.5).to_radians();
        let dir = vec2(angle.sin(), -angle.cos());
        let inner = if n % 4 == 0 { 65.0 } else { 69.0 };
        p.line_segment([c + dir * inner * scale, c + dir * 73.0 * scale], line);
    }
    p.line_segment([c + vec2(-82.0, 0.0) * scale, c + vec2(82.0, 0.0) * scale], line);
    p.line_segment([c + vec2(0.0, -82.0) * scale, c + vec2(0.0, 82.0) * scale], line);
    let text = theme::TEXT.gamma_multiply(a);
    let small = FontId::proportional(11.0 * scale.max(0.7));
    p.text(c + vec2(-85.0, -88.0) * scale, Align2::CENTER_CENTER, format!("{:.2}", axes.y), small.clone(), text);
    p.text(c + vec2(86.0, 90.0) * scale, Align2::CENTER_CENTER, format!("{:.2}", axes.x), small.clone(), text);
    p.text(c + vec2(-67.0, 90.0) * scale, Align2::CENTER_CENTER, format!("{}°", sv.angle), small.clone(), text);
    p.text(c + vec2(68.0, -80.0) * scale, Align2::CENTER_CENTER, format!("{}%", (sv.magnitude * 100.0).round()), small, text);
    let tip = c + vec2(sv.x, sv.y) * 40.0 * scale;
    p.line_segment([c, tip], Stroke::new(2.0, theme::ACCENT.gamma_multiply(a)));
    p.line_segment([tip - vec2(9.0, 0.0) * scale, tip + vec2(9.0, 0.0) * scale], Stroke::new(1.5, theme::ACCENT.gamma_multiply(a)));
    p.line_segment([tip - vec2(0.0, 9.0) * scale, tip + vec2(0.0, 9.0) * scale], Stroke::new(1.5, theme::ACCENT.gamma_multiply(a)));
}

fn circle(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..24).map(|i| c + Vec2::angled(i as f32 / 24.0 * std::f32::consts::TAU) * r).collect()
}

/// The names the drawing hands out are the fixed control names.
fn static_id(id: &str) -> &'static str {
    BINDINGS.iter().chain(["left", "right"].iter()).find(|b| **b == id).copied().unwrap_or("a")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(bindings: &[(&str, u8)], max_depth: &[(&str, u8)]) -> Config {
        Config { enabled: true, bindings: bindings.iter().map(|(k, v)| (k.to_string(), *v)).collect(), max_depth: max_depth.iter().map(|(k, v)| (k.to_string(), *v)).collect() }
    }

    #[test]
    fn half_travel_presses_a_button_and_a_stick_reaches_full_at_its_maximum() {
        let mut depth = vec![0u8; 256];
        depth[4] = 128;
        depth[5] = 128;
        let s = state(&config(&[("a", 4), ("left_right", 5)], &[("left_right", 50)]), &depth);
        assert!(s.buttons["a"]);
        assert!((s.left.x - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_missing_maximum_is_full_depth() {
        assert!(same(&config(&[], &[("lt", 100)]), &config(&[], &[])));
        assert!(!same(&config(&[], &[("lt", 50)]), &config(&[], &[])));
    }

    #[test]
    fn a_stick_view_is_clamped_to_the_circle() {
        let v = stick_view(Stick { x: 1.0, y: -1.0 });
        assert!((v.x.hypot(v.y) - 1.0).abs() < 1e-5);
        assert_eq!(v.angle, 45);
        assert_eq!(stick_view(Stick::default()).opacity, 0.0);
    }

    #[test]
    fn a_capture_takes_the_first_key_crossing_half_travel() {
        let previous = vec![0u8; 256];
        let mut depth = vec![0u8; 256];
        depth[9] = 200;
        assert_eq!(captured_key(&previous, &depth, &[3, 9]), Some(9));
        assert_eq!(captured_key(&depth, &depth, &[3, 9]), None, "a key already held is not a press");
    }
}
