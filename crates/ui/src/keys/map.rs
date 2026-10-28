//! The key map: the board's caps drawn from its layout, picked by click, Ctrl+click, a box
//! or the arrows.

use std::collections::{BTreeMap, BTreeSet};

use easyrazer_backend::api::KeyView;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use razer_core::binding::System;
use razer_core::layout::{LayoutKey, Shape};

use super::nav::{self, Direction};
use crate::app::App;
use crate::icons::{self, Icon};
use crate::theme;

/// One key unit, in points.
const U: f32 = 50.0;
/// Space between key caps; the map ends at the last cap, not at the space after it.
const GAP: f32 = 4.0;
/// Width of a ring's frame, in key units.
const RING: f32 = 0.3;
/// A press that moves less than this is a click, not a box.
const DRAG: f32 = 4.0;

/// Zones are named by the window; key caps keep the labels from the layout.
const ZONES: [(u8, &str); 4] = [(200, "zones.media"), (201, "zones.dial"), (202, "zones.edge"), (203, "zones.wrist")];

/// A Hypershift binding unlike the normal one: system actions show as icons.
pub struct HsBind {
    pub text: String,
    pub icon: Option<System>,
}

#[derive(Default)]
pub struct Props {
    /// Text under the label, and the keys whose value is not applied yet.
    pub values: BTreeMap<u8, String>,
    /// Show `values` in place of the label.
    pub inline: bool,
    /// Hypershift press point, on keys where it differs from `values`; shown above it.
    pub hs_values: BTreeMap<u8, String>,
    pub hs_edited: BTreeSet<u8>,
    pub hs_binds: BTreeMap<u8, HsBind>,
    pub edited: BTreeSet<u8>,
    pub errors: BTreeMap<u8, String>,
    pub unsaved: BTreeSet<u8>,
    /// The keys are painted with these instead of showing `values`.
    pub colors: Option<BTreeMap<u8, [u8; 3]>>,
    /// Rapid Trigger label of the keys with it on.
    pub rapid: BTreeMap<u8, &'static str>,
    pub rapid_edited: BTreeSet<u8>,
    /// Snap Tap group color per key, shown as a bar along the top edge.
    pub snap: BTreeMap<u8, Color32>,
    /// Shrink the whole map to the width instead of scrolling.
    pub fit: bool,
    /// One key at a time: no Ctrl+click and no box.
    pub single: bool,
    /// Editable keys shown as read, in layout order; `None` when the map is not being read.
    pub loaded: Option<usize>,
    /// The keyboard is away: the map is shown dimmed.
    pub dim: bool,
}

/// What lives between frames: the box being dragged and the arrow cursor.
#[derive(Default)]
pub struct MapState {
    start: Option<Pos2>,
    band: Option<Rect>,
    additive: bool,
    cursor: Option<u8>,
    /// The ring frames the whole board; leaving it returns to the key it was entered from.
    entered: Option<u8>,
}

fn bounds(layout: &[KeyView]) -> Vec2 {
    let w = layout.iter().map(|k| k.key.x + k.key.w).fold(0.0, f32::max) * U - GAP;
    let h = layout.iter().map(|k| k.key.y + k.key.h).fold(0.0, f32::max) * U - GAP;
    vec2(w.max(0.0), h.max(0.0))
}

fn cap_rect(k: &LayoutKey, origin: Pos2, scale: f32) -> Rect {
    Rect::from_min_size(origin + vec2(k.x * U, k.y * U) * scale, vec2(k.w * U - GAP, k.h * U - GAP) * scale)
}

fn ring_rect(k: &LayoutKey, origin: Pos2, scale: f32) -> Rect {
    Rect::from_min_size(origin + vec2(k.x * U, k.y * U) * scale, vec2(k.w * U, k.h * U) * scale)
}

/// A ring is hit on its frame only; the keys inside stay reachable.
fn on_ring(k: &LayoutKey, r: Rect, origin: Pos2, scale: f32) -> bool {
    let outer = ring_rect(k, origin, scale);
    let inner = outer.shrink(RING * U * scale);
    outer.intersects(r) && !inner.contains_rect(r)
}

/// The editable keys light up in layout order as the whole read goes on.
fn is_read(layout: &[KeyView], key: u8, loaded: Option<usize>) -> bool {
    let Some(n) = loaded else { return true };
    layout.iter().filter(|k| k.key.editable).position(|k| k.key.key == key).is_none_or(|i| i < n)
}

pub fn show(app: &App, ui: &mut Ui, layout: &[KeyView], props: &Props, selection: &mut BTreeSet<u8>, state: &mut MapState) -> Response {
    let size = bounds(layout);
    let scale = if props.fit && size.x > 0.0 { (ui.available_width() / size.x).min(1.0) } else { 1.0 };
    let (outer, response) = ui.allocate_exact_size(vec2(ui.available_width().max(size.x * scale), size.y * scale), Sense::click_and_drag());
    let origin = pos2(outer.center().x - size.x * scale / 2.0, outer.top());
    let painter = ui.painter_at(outer.expand(4.0));
    let opacity = if props.dim { 0.6 } else { 1.0 };
    let keys: Vec<&KeyView> = layout.iter().filter(|k| k.key.shape != Shape::Ring).collect();
    let rings: Vec<&KeyView> = layout.iter().filter(|k| k.key.shape == Shape::Ring).collect();

    pointer(layout, ui, &response, props, selection, state, origin, scale);
    if response.has_focus() {
        keyboard(layout, ui, props, selection, state);
    }

    for r in &rings {
        let rect = ring_rect(&r.key, origin, scale);
        let frame = RING * U * scale;
        painter.rect_stroke(rect.shrink(frame / 2.0), 8.0, Stroke::new(frame, theme::LINE.gamma_multiply(opacity)), StrokeKind::Middle);
        if let Some(c) = props.colors.as_ref().and_then(|c| c.get(&r.key.key)) {
            painter.rect_stroke(rect.shrink(frame), 6.0, Stroke::new(4.0, rgb(*c)), StrokeKind::Inside);
        }
        if selection.contains(&r.key.key) {
            painter.rect_stroke(rect.expand(2.0), 10.0, Stroke::new(2.0, theme::TEXT), StrokeKind::Outside);
        }
    }
    let focused = response.has_focus().then(|| focusable(layout, selection, state)).flatten();
    for k in &keys {
        paint_key(app, layout, &painter, k, props, selection, origin, scale, opacity, focused == Some(k.key.key));
    }
    if let Some(r) = rings.iter().find(|r| Some(r.key.key) == focused) {
        painter.rect_stroke(ring_rect(&r.key, origin, scale).expand(4.0), 10.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Outside);
    }
    if let Some(b) = state.band {
        painter.rect(b, 0.0, theme::ACCENT.gamma_multiply(0.08), Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
    }
    if let Some(pos) = response.hover_pos()
        && let Some(k) = keys.iter().find(|k| cap_rect(&k.key, origin, scale).contains(pos))
        && let Some(tip) = title(app, k.key.key, props)
    {
        response.clone().on_hover_text_at_pointer(tip);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, app.t("common.keyMap")));
    response
}

#[allow(clippy::too_many_arguments)]
fn paint_key(app: &App, layout: &[KeyView], painter: &egui::Painter, k: &KeyView, props: &Props, selection: &BTreeSet<u8>, origin: Pos2, scale: f32, opacity: f32, focused: bool) {
    let key = k.key.key;
    let rect = cap_rect(&k.key, origin, scale);
    let selected = selection.contains(&key);
    let read = is_read(layout, key, props.loaded);
    let painted = props.colors.is_some();
    let live = painted || (k.key.editable && read);
    let radius = if k.key.shape == Shape::Round { CornerRadius::same((rect.height().min(rect.width()) / 2.0) as u8) } else { CornerRadius::same(theme::RADIUS) };
    let fill = if live { theme::KEY } else { theme::KEY_OFF };
    painter.rect_filled(rect, radius, fill.gamma_multiply(opacity));
    let error = props.errors.contains_key(&key);
    let border = if error { Some(theme::ERROR) } else if selected && !painted { Some(theme::ACCENT) } else { None };
    if let Some(c) = border {
        painter.rect_stroke(rect, radius, Stroke::new(2.0, c), StrokeKind::Inside);
    }
    if painted && selected {
        painter.rect_stroke(rect.expand(1.0), radius, Stroke::new(2.0, theme::TEXT), StrokeKind::Outside);
    }
    if focused {
        painter.rect_stroke(rect.expand(3.0), radius, Stroke::new(2.0, theme::ACCENT), StrokeKind::Outside);
    }
    let clip = painter.with_clip_rect(rect.shrink(2.0));
    // A painted key stays a dark cap with its color as a strip along the bottom: filled caps glared.
    if let Some(c) = props.colors.as_ref().and_then(|c| c.get(&key)) {
        let strip = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 5.0), rect.right_bottom());
        clip.rect_filled(strip, 0.0, rgb(*c));
    } else if let Some(c) = props.snap.get(&key) {
        let bar = Rect::from_min_max(rect.left_top(), pos2(rect.right(), rect.top() + 3.0));
        clip.rect_filled(bar, 0.0, *c);
    }
    let text_color = |c: Color32| c.gamma_multiply(opacity);
    let inner = rect.shrink2(vec2(5.0, 3.0) * scale.max(0.5));
    let small = 12.0 * scale.max(0.75);
    let tiny = 11.0 * scale.max(0.75);
    let round = k.key.shape == Shape::Round;
    let (anchor, at) = if round { (Align2::CENTER_CENTER, rect.center()) } else { (Align2::LEFT_TOP, inner.left_top()) };
    match props.values.get(&key).filter(|_| props.inline) {
        Some(v) => {
            let edited = props.edited.contains(&key);
            clip.text(at, anchor, v, font(small, edited), text_color(if edited { theme::EDITED } else { theme::ACCENT }));
        }
        None => {
            let label = if key == 203 { app.t("zones.wrist") } else { k.key.label.to_string() };
            let color = if live { theme::TEXT } else { theme::MUTED };
            clip.text(at, anchor, label, FontId::proportional(small), text_color(color));
        }
    }
    if painted || !read {
        return;
    }
    let mut x = inner.left();
    let bottom = inner.bottom();
    if let Some(r) = props.rapid.get(&key) {
        let edited = props.rapid_edited.contains(&key);
        let size = if *r == "CRT" { 9.0 * scale.max(0.75) } else { tiny };
        let g = clip.text(pos2(x, bottom), Align2::LEFT_BOTTOM, *r, font(size, edited), text_color(if edited { theme::EDITED } else { theme::ACCENT }));
        x = g.right() + 4.0;
    }
    if let Some(h) = props.hs_binds.get(&key) {
        let edited = props.hs_edited.contains(&key);
        let color = text_color(if edited { theme::EDITED } else { theme::HS });
        match h.icon.and_then(system_icon) {
            Some(icon) => {
                icons::paint(&clip, icon, Rect::from_min_size(pos2(x, bottom - 13.0), vec2(13.0, 13.0)), color);
                x += 17.0;
            }
            None => {
                let g = clip.text(pos2(x, bottom), Align2::LEFT_BOTTOM, &h.text, font(tiny, edited), color);
                x = g.right() + 4.0;
            }
        }
    }
    let _ = x;
    if k.key.editable && !props.inline
        && let Some(v) = props.values.get(&key)
    {
        let edited = props.edited.contains(&key);
        let unsaved = props.unsaved.contains(&key);
        let color = if edited { theme::EDITED } else if unsaved { theme::ACCENT } else { theme::MUTED };
        match props.hs_values.get(&key) {
            Some(hs) => {
                let hs_edited = props.hs_edited.contains(&key);
                let size = 9.0 * scale.max(0.75);
                clip.text(pos2(inner.right(), bottom - 10.0), Align2::RIGHT_BOTTOM, hs, font(size, hs_edited), text_color(if hs_edited { theme::EDITED } else { theme::HS }));
                clip.text(pos2(inner.right(), bottom), Align2::RIGHT_BOTTOM, v, font(size, edited), text_color(color));
            }
            None => {
                clip.text(pos2(inner.right(), bottom), Align2::RIGHT_BOTTOM, v, font(tiny, edited), text_color(color));
            }
        }
    }
}

fn font(size: f32, bold: bool) -> FontId {
    if bold { FontId::new(size, theme::bold()) } else { FontId::proportional(size) }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn system_icon(s: System) -> Option<Icon> {
    Some(match s {
        System::BrightnessDown => Icon::brightness_down,
        System::BrightnessUp => Icon::brightness_up,
        System::Sleep => Icon::sleep,
        System::GameMode => Icon::game_mode,
        System::MacroLed => Icon::macro_led,
        System::NextProfile => Icon::next_profile,
        System::SnapTap => Icon::snap_tap,
        System::GamepadToggle => Icon::gamepad_toggle,
    })
}

fn title(app: &App, key: u8, props: &Props) -> Option<String> {
    if let Some((_, zone)) = ZONES.iter().find(|(k, _)| *k == key) {
        return Some(app.t(zone));
    }
    if let Some(e) = props.errors.get(&key) {
        return Some(e.clone());
    }
    props.unsaved.contains(&key).then(|| app.t("actuation.unsavedKey"))
}

fn editable_at(layout: &[KeyView], pos: Pos2, origin: Pos2, scale: f32) -> Option<u8> {
    let hit = layout.iter().find(|k| k.key.shape != Shape::Ring && cap_rect(&k.key, origin, scale).contains(pos));
    let hit = hit.or_else(|| layout.iter().find(|k| k.key.shape == Shape::Ring && on_ring(&k.key, Rect::from_center_size(pos, Vec2::ZERO), origin, scale)));
    hit.filter(|k| k.key.editable).map(|k| k.key.key)
}

#[allow(clippy::too_many_arguments)]
fn pointer(layout: &[KeyView], ui: &Ui, response: &Response, props: &Props, selection: &mut BTreeSet<u8>, state: &mut MapState, origin: Pos2, scale: f32) {
    let ctrl = ui.input(|i| i.modifiers.command);
    if response.drag_started() {
        state.start = response.interact_pointer_pos();
        state.additive = ctrl && !props.single;
    }
    if response.dragged()
        && !props.single
        && let (Some(start), Some(now)) = (state.start, response.interact_pointer_pos())
        && (state.band.is_some() || start.distance(now) > DRAG)
    {
        state.band = Some(Rect::from_two_pos(start, now));
    }
    if response.drag_stopped() {
        if let Some(b) = state.band.take() {
            let mut next = if state.additive { selection.clone() } else { BTreeSet::new() };
            for k in layout.iter().filter(|k| k.key.editable) {
                let hit = if k.key.shape == Shape::Ring { on_ring(&k.key, b, origin, scale) } else { cap_rect(&k.key, origin, scale).intersects(b) };
                if hit {
                    next.insert(k.key.key);
                }
            }
            *selection = next;
        }
        state.start = None;
    }
    if response.clicked() {
        response.request_focus();
        let additive = ctrl && !props.single;
        let mut next = if additive { selection.clone() } else { BTreeSet::new() };
        if let Some(k) = response.interact_pointer_pos().and_then(|p| editable_at(layout, p, origin, scale)) {
            state.cursor = Some(k);
            if additive && next.contains(&k) {
                next.remove(&k);
            } else {
                next.insert(k);
            }
        }
        *selection = next;
    }
}

/// One tab stop for the whole map; the arrows move between the keys.
fn focusable(layout: &[KeyView], selection: &BTreeSet<u8>, state: &MapState) -> Option<u8> {
    let all: Vec<u8> = layout.iter().filter(|k| k.key.editable).map(|k| k.key.key).collect();
    if let Some(c) = state.cursor.filter(|c| all.contains(c)) {
        return Some(c);
    }
    all.iter().copied().find(|k| selection.contains(k)).or(all.first().copied())
}

fn keyboard(layout: &[KeyView], ui: &Ui, props: &Props, selection: &mut BTreeSet<u8>, state: &mut MapState) {
    let Some(from) = focusable(layout, selection, state).and_then(|k| layout.iter().find(|v| v.key.key == k)) else { return };
    let multi = !props.single;
    let nav: Vec<&LayoutKey> = layout.iter().filter(|k| k.key.editable && k.key.shape != Shape::Ring).map(|k| &k.key).collect();
    let ring = layout.iter().find(|k| k.key.editable && k.key.shape == Shape::Ring).map(|k| &k.key);
    let (pressed, mods) = ui.input(|i| {
        let keys = [egui::Key::ArrowLeft, egui::Key::ArrowRight, egui::Key::ArrowUp, egui::Key::ArrowDown, egui::Key::Home, egui::Key::End, egui::Key::Space, egui::Key::Enter, egui::Key::A];
        (keys.into_iter().filter(|k| i.key_pressed(*k)).collect::<Vec<_>>(), i.modifiers)
    });
    for key in pressed {
        let dir = match key {
            egui::Key::ArrowLeft => Some(Direction::Left),
            egui::Key::ArrowRight => Some(Direction::Right),
            egui::Key::ArrowUp => Some(Direction::Up),
            egui::Key::ArrowDown => Some(Direction::Down),
            _ => None,
        };
        let to = match key {
            egui::Key::Space | egui::Key::Enter => {
                let toggle = mods.command && multi;
                let mut next = if toggle { selection.clone() } else { BTreeSet::new() };
                if toggle && next.contains(&from.key.key) {
                    next.remove(&from.key.key);
                } else {
                    next.insert(from.key.key);
                }
                *selection = next;
                continue;
            }
            egui::Key::A if mods.command && multi => {
                *selection = layout.iter().filter(|k| k.key.editable).map(|k| k.key.key).collect();
                continue;
            }
            egui::Key::Home => nav.first().copied(),
            egui::Key::End => nav.last().copied(),
            _ => match dir {
                Some(_) if from.key.shape == Shape::Ring => state.entered.and_then(|e| nav.iter().copied().find(|k| k.key == e)).or(nav.first().copied()),
                Some(d) => {
                    let next = nav::step(&nav, &from.key, d);
                    if next.is_none() && ring.is_some() {
                        state.entered = Some(from.key.key);
                    }
                    next.or(ring)
                }
                None => None,
            },
        };
        if let Some(to) = to {
            if mods.shift && multi {
                selection.insert(from.key.key);
                selection.insert(to.key);
            }
            state.cursor = Some(to.key);
        }
    }
}
