//! The Lighting tab: effects, colors and the custom per-key layout.

mod color;
mod model;

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use easyrazer_backend::api::{self, KeyView};
use easyrazer_backend::device::LightingState;
use egui::{Color32, RichText, Stroke, Ui};
use razer_core::lighting::{EffectInfo, Look, Rgb};

use crate::app::App;
use crate::icons::Icon;
use crate::jobs::Done;
use crate::keys::map::{self, MapState};
use crate::{theme, widgets};
use model::{Controls, Group};

/// At most one preview this often; the newest draft wins.
const PREVIEW_EVERY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct Lighting {
    effects: Vec<EffectInfo>,
    groups: Vec<Group>,
    applied: Option<Look>,
    saved: Option<Look>,
    controls: Option<Controls>,
    dynamic_lighting: bool,
    delegated: bool,
    busy: bool,
    message: String,
    layout: Vec<KeyView>,
    /// Last applied custom layout, offered again when the effect is picked.
    custom: Option<BTreeMap<u8, Rgb>>,
    selection: BTreeSet<u8>,
    map: MapState,
    /// The keyboard and profile the effects were read for.
    loaded_for: Option<(Option<String>, Option<u32>)>,
    previewed: Option<Look>,
    pending: Option<Look>,
    sending: bool,
    sent_at: Option<Instant>,
}

impl Lighting {
    fn draft(&self) -> Option<Look> {
        self.controls.as_ref().and_then(|c| model::resolve(&self.groups, c))
    }

    fn is_custom(&self) -> bool {
        self.controls.as_ref().is_some_and(|c| c.group == "custom")
    }
}

fn blocked(app: &App) -> bool {
    app.status.as_ref().is_none_or(|s| s.synapse || !s.device)
}

fn dirty(app: &App) -> bool {
    let l = &app.lighting;
    l.draft().is_some_and(|d| Some(&d) != l.applied.as_ref().or(l.saved.as_ref()))
}

pub fn pending(app: &App) -> usize {
    usize::from(dirty(app))
}

pub fn can_apply(app: &App) -> bool {
    dirty(app) && !app.lighting.busy && !blocked(app)
}

pub fn can_write(app: &App) -> bool {
    let l = &app.lighting;
    !blocked(app) && !l.busy && !l.is_custom() && l.draft().is_some_and(|d| Some(&d) != l.saved.as_ref())
}

/// A custom layout cannot go into the flash and stays in the app.
pub fn skips_write(app: &App) -> bool {
    app.lighting.is_custom()
}

pub fn apply(app: &mut App, then: Done) {
    send(app, false, then);
}

pub fn write(app: &mut App, then: Done) {
    send(app, true, then);
}

pub fn revert(app: &mut App) {
    let l = &mut app.lighting;
    let stored = l.applied.clone().or(l.saved.clone());
    l.controls = model::controls_from(&l.effects, stored.as_ref(), l.custom.as_ref(), l.layout.iter().map(|k| k.key.key));
}

fn send(app: &mut App, save: bool, then: Done) {
    let (Some(id), Some(look)) = (app.keyboard.clone(), app.lighting.draft()) else {
        then(app);
        return;
    };
    app.lighting.busy = true;
    let worker_id = id.clone();
    let sent = look.clone();
    app.jobs.run(move |b| if save { b.lighting_write(&worker_id, sent) } else { b.lighting_apply(&worker_id, sent) }, move |app, r| {
        app.lighting.busy = false;
        match r {
            Ok(()) if app.ours(Some(&id)) => {
                let l = &mut app.lighting;
                l.applied = Some(look.clone());
                if save {
                    l.saved = Some(look);
                } else if let Some(c) = look.effect.colors {
                    l.custom = Some(c);
                }
                app.lighting.message = app.t(if save { "lighting.saved" } else { "lighting.applied" });
            }
            Ok(()) => {}
            Err(e) => app.lighting.message = e,
        }
        then(app);
    });
}

/// Every frame: reads the effects when a keyboard connects or loads another profile, and sends
/// the preview of what the controls show.
pub fn tick(app: &mut App) {
    let connected = !blocked(app);
    let key = (app.keyboard.clone(), app.status.as_ref().and_then(|s| s.profile));
    if connected && app.lighting.loaded_for.as_ref() != Some(&key) {
        app.lighting.loaded_for = Some(key);
        load(app);
    }
    if !connected {
        app.lighting.loaded_for = None;
        return;
    }
    let draft = app.lighting.draft();
    if draft.is_some() && draft != app.lighting.previewed {
        app.lighting.previewed = draft.clone();
        app.lighting.pending = draft;
    }
    preview(app);
}

/// One preview in flight: parallel calls would race for the device lock and could land out of order.
fn preview(app: &mut App) {
    let l = &mut app.lighting;
    if l.sending || l.pending.is_none() {
        return;
    }
    if let Some(t) = l.sent_at
        && t.elapsed() < PREVIEW_EVERY
    {
        app.jobs.wake_after(PREVIEW_EVERY - t.elapsed());
        return;
    }
    let (Some(id), Some(look)) = (app.keyboard.clone(), l.pending.take()) else { return };
    l.sending = true;
    l.sent_at = Some(Instant::now());
    let worker_id = id.clone();
    app.jobs.run(move |b| b.lighting_preview(&worker_id, &look), move |app, r| {
        app.lighting.sending = false;
        if let Err(e) = r
            && app.ours(Some(&id))
        {
            app.lighting.message = e;
        }
        app.jobs.wake_after(PREVIEW_EVERY);
    });
}

fn load(app: &mut App) {
    let Some(id) = app.keyboard.clone() else { return };
    if app.lighting.layout.is_empty() {
        app.lighting.layout = api::lighting_layout();
    }
    let worker_id = id.clone();
    app.jobs.run(move |b| b.lighting_state(&worker_id), move |app, r: Result<LightingState, String>| match r {
        Ok(s) if app.ours(Some(&id)) => {
            let l = &mut app.lighting;
            l.groups = model::groups(&s.effects);
            l.effects = s.effects;
            l.custom = s.custom;
            l.applied = s.applied;
            l.saved = s.saved;
            l.dynamic_lighting = s.dynamic_lighting;
            l.delegated = s.delegated;
            revert(app);
            // What the keyboard shows already needs no preview.
            app.lighting.previewed = app.lighting.draft();
            app.lighting.message.clear();
        }
        Ok(_) => {}
        Err(e) => app.lighting.message = e,
    });
}

fn set_delegated(app: &mut App, on: bool) {
    let Some(id) = app.keyboard.clone() else { return };
    let worker_id = id.clone();
    app.jobs.run(move |b| b.set_lighting_delegated(&worker_id, on), move |app, r| match r {
        Ok(()) if app.ours(Some(&id)) => {
            app.lighting.delegated = on;
            app.lighting.message = app.t(if on { "lighting.dlDelegated" } else { "lighting.dlTaken" });
        }
        Ok(()) => {}
        Err(e) => app.lighting.message = e,
    });
}

fn open_settings(app: &mut App) {
    if let Err(e) = api::open_dynamic_lighting_settings() {
        app.lighting.message = e;
    }
}

/// Names the description adds later show as they are until translated.
fn label(lang: &str, section: &str, name: &str) -> String {
    let key = format!("lighting.{section}.{name}");
    let t = easyrazer_backend::i18n::t(lang, &key);
    if t == key { name.to_string() } else { t }
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let blocked = blocked(app);
    if app.lighting.dynamic_lighting && !app.lighting.delegated {
        egui::Frame::new().fill(theme::WARN_BG).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(app.t("lighting.dlWarning")).color(theme::WARN));
                if widgets::button(ui, app.t("lighting.dlTakeBack"), !blocked).clicked() {
                    set_delegated(app, false);
                }
                if ui.button(app.t("lighting.dlSettings")).clicked() {
                    open_settings(app);
                }
            });
        });
    }
    if !app.lighting.message.is_empty() {
        ui.label(RichText::new(&app.lighting.message).color(theme::MUTED));
    }
    if app.lighting.draft().is_none() {
        ui.label(RichText::new(app.t("lighting.connect")).color(theme::MUTED));
    } else {
        ui.add_enabled_ui(!blocked, |ui| {
            if blocked {
                ui.multiply_opacity(0.4);
            }
            ui.horizontal_top(|ui| {
                groups(app, ui);
                ui.vertical(|ui| widgets::card(ui, |ui| controls(app, ui)));
            });
        });
    }
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            let mut on = app.lighting.delegated;
            if widgets::toggle(ui, &mut on, &app.t("lighting.dynamic"), !blocked).changed() {
                set_delegated(app, on);
            }
            if ui.button(app.t("lighting.dlSettings")).clicked() {
                open_settings(app);
            }
        });
    });
    crate::dev::lighting_log(app, ui);
}

fn groups(app: &mut App, ui: &mut Ui) {
    let names: Vec<(String, String)> = app.lighting.groups.iter().map(|g| (g.group.clone(), label(&app.lang, "groups", &g.group))).collect();
    let current = app.lighting.controls.as_ref().map(|c| c.group.clone());
    egui::Frame::new().fill(theme::PANEL).corner_radius(theme::RADIUS).inner_margin(10).show(ui, |ui| {
        ui.vertical(|ui| groups_list(app, ui, names, current));
    });
}

fn groups_list(app: &mut App, ui: &mut Ui, names: Vec<(String, String)>, current: Option<String>) {
    ui.set_width(190.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    for (group, name) in names {
        let on = current.as_deref() == Some(group.as_str());
        let (rect, r) = ui.allocate_exact_size(egui::vec2(190.0, 36.0), egui::Sense::click());
        ui.painter().rect_filled(rect, theme::RADIUS, theme::KEY);
        if on || r.hovered() {
            swatch(ui, &group, rect.shrink(2.0));
        }
        if on {
            ui.painter().rect_stroke(rect, theme::RADIUS, Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
        }
        ui.painter().text(rect.left_center() + egui::vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(14.0), theme::TEXT);
        if r.clicked() {
            pick_group(app, &group);
        }
    }
}

/// A hint of the effect behind the group's name, drawn while it is hovered or picked.
fn swatch(ui: &Ui, group: &str, rect: egui::Rect) {
    let t = ui.input(|i| i.time) as f32;
    let alpha = 0.45;
    let p = ui.painter().with_clip_rect(rect);
    let hsv = |h: f32| color::color32(color::rgb_of_hsv([h.rem_euclid(360.0), 1.0, 1.0])).gamma_multiply(alpha);
    match group {
        "static" => {
            p.rect_filled(rect, theme::RADIUS, theme::ACCENT.gamma_multiply(alpha));
        }
        "breathing" | "fire" => {
            let period = if group == "fire" { 1.2 } else { 3.0 };
            let k = 0.5 - 0.5 * (t / period * std::f32::consts::TAU).cos();
            let c = if group == "fire" { Color32::from_rgb(0xff, 0x80, 0x00) } else { theme::ACCENT };
            p.rect_filled(rect, theme::RADIUS, c.gamma_multiply(alpha * k));
        }
        "spectrum" => {
            p.rect_filled(rect, theme::RADIUS, hsv(t / 4.0 * 360.0));
        }
        "wave" => {
            let n = 24;
            let w = rect.width() / n as f32;
            for i in 0..n {
                let x = rect.left() + i as f32 * w;
                let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w + 0.5, rect.height()));
                p.rect_filled(r, 0.0, hsv(i as f32 / n as f32 * 720.0 + t / 3.0 * 360.0));
            }
        }
        "reactive" => {
            let r = (t / 1.6).fract() * 220.0;
            let center = egui::pos2(rect.left() + rect.width() * 0.2, rect.center().y);
            p.circle_stroke(center, r, Stroke::new(6.0, theme::ACCENT.gamma_multiply(alpha * (1.0 - r / 220.0))));
        }
        "starlight" => {
            let stars = [(0.12, 0.3, 0.0), (0.48, 0.7, 0.0), (0.8, 0.35, 0.0), (0.3, 0.6, 0.5), (0.66, 0.25, 0.5), (0.92, 0.7, 0.5)];
            for (x, y, phase) in stars {
                let a = 0.5 - 0.5 * ((t / 1.8 + phase) * std::f32::consts::TAU).cos();
                p.circle_filled(egui::pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height()), 2.5, Color32::WHITE.gamma_multiply(a));
            }
        }
        _ => return,
    }
    ui.ctx().request_repaint_after(Duration::from_millis(33));
}

fn pick_group(app: &mut App, group: &str) {
    let types = app.lighting.groups.iter().find(|g| g.group == group).map(|g| g.types.clone()).unwrap_or_default();
    if let Some(c) = &mut app.lighting.controls {
        c.group = group.to_string();
        if c.kind.as_ref().is_none_or(|k| !types.contains(k)) {
            c.kind = types.first().cloned();
        }
    }
}

fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([100.0, 32.0], egui::Label::new(RichText::new(label).color(theme::MUTED)));
        add(ui);
    });
}

fn controls(app: &mut App, ui: &mut Ui) {
    let Some(c) = app.lighting.controls.clone() else { return };
    let lang = app.lang.clone();
    let t = |k: &str| easyrazer_backend::i18n::t(&lang, k);
    let types = app.lighting.groups.iter().find(|g| g.group == c.group).map(|g| g.types.clone()).unwrap_or_default();
    let cands = model::candidates(&app.lighting.groups, &c);
    let slots = cands.iter().map(|(e, _)| e.colors).max().unwrap_or(0);
    let has_random = cands.iter().any(|(_, p)| p.variant == Some(model::Variant::Random));
    // Taken from the group, not the resolved effect: with no colors the effect is "off" but the controls stay.
    let dirs = cands.iter().find(|(e, _)| !e.dirs.is_empty()).map(|(e, _)| e.dirs.clone());
    let speed = cands.iter().find_map(|(e, _)| e.speed.map(|s| (s, e.fast_low)));
    let random_on = c.random && has_random;
    let mut next = c.clone();
    if !types.is_empty() {
        field(ui, &t("lighting.type"), |ui| {
            let items: Vec<(Option<String>, String)> = types.iter().map(|k| (Some(k.clone()), label(&lang, "types", k))).collect();
            if let Some(k) = widgets::segments(ui, &items, c.kind.clone(), true) {
                next.kind = k;
            }
        });
    }
    if slots > 0 {
        field(ui, &t(if slots > 1 { "lighting.colors" } else { "lighting.color" }), |ui| {
            for i in 0..slots {
                if let Some(v) = color::picker(ui, ("fx-color", i), c.colors.get(i).copied().flatten(), !random_on, &t) {
                    if next.colors.len() <= i {
                        next.colors.resize(i + 1, None);
                    }
                    next.colors[i] = v;
                }
            }
            if has_random {
                let b = egui::Button::image(crate::icons::image(Icon::random, 18.0, if c.random { theme::ACCENT } else { theme::TEXT }))
                    .stroke(Stroke::new(1.0, if c.random { theme::ACCENT } else { theme::LINE }))
                    .min_size(egui::vec2(34.0, 32.0));
                if ui.add(b).on_hover_text(t("lighting.random")).clicked() {
                    next.random = !c.random;
                }
            }
        });
    }
    if let Some(dirs) = dirs {
        field(ui, &t("lighting.direction"), |ui| {
            let current = c.dir.clone().filter(|d| dirs.contains(d)).unwrap_or_else(|| dirs[0].clone());
            let items: Vec<(String, String)> = dirs.iter().map(|d| (d.clone(), label(&lang, "dirs", d))).collect();
            if let Some(d) = widgets::segments(ui, &items, current, true) {
                next.dir = Some(d);
            }
        });
    }
    if let Some(([lo, hi], fast_low)) = speed {
        // Some firmware speeds are "lower is faster"; the slider always reads slow → fast.
        let flip = |v: u8| if fast_low { lo + hi - v } else { v };
        field(ui, &t("lighting.speed"), |ui| {
            ui.vertical(|ui| {
                ui.set_width(240.0);
                ui.spacing_mut().slider_width = 240.0;
                let cur = c.speed.filter(|s| (lo..=hi).contains(s)).unwrap_or(((u16::from(lo) + u16::from(hi)) / 2) as u8);
                let mut v = flip(cur);
                if ui.add(egui::Slider::new(&mut v, lo..=hi).show_value(false)).changed() {
                    next.speed = Some(flip(v));
                }
                ui.horizontal(|ui| {
                    widgets::hint(ui, t("lighting.slow"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| widgets::hint(ui, t("lighting.fast")));
                });
            });
        });
    }
    if c.group == "custom" {
        custom(app, ui, &mut next);
    }
    if c.group == "off" {
        ui.label(RichText::new(t("lighting.isOff")).color(theme::MUTED));
    } else {
        field(ui, &t("lighting.brightness"), |ui| {
            let mut percent = (f32::from(c.brightness) / 255.0 * 100.0).round() as u8;
            ui.spacing_mut().slider_width = 240.0;
            if ui.add(egui::Slider::new(&mut percent, 0..=100).show_value(false)).changed() {
                next.brightness = (f32::from(percent) * 255.0 / 100.0).round() as u8;
            }
            ui.label(format!("{percent}%"));
        });
    }
    if next != c {
        app.lighting.controls = Some(next);
    }
}

fn custom(app: &mut App, ui: &mut Ui, next: &mut Controls) {
    let lang = app.lang.clone();
    let t = |k: &str| easyrazer_backend::i18n::t(&lang, k);
    let props = map::Props { colors: Some(next.paint.clone()), fit: true, ..Default::default() };
    let layout = std::mem::take(&mut app.lighting.layout);
    let mut selection = std::mem::take(&mut app.lighting.selection);
    let mut state = std::mem::take(&mut app.lighting.map);
    map::show(app, ui, &layout, &props, &mut selection, &mut state);
    // The color all selected keys share, or none when they differ.
    let shared = crate::keys::model::common(selection.iter().map(|k| next.paint.get(k).copied().unwrap_or(model::BLACK)));
    field(ui, &t("lighting.color"), |ui| {
        if let Some(v) = color::picker(ui, "paint", shared, !selection.is_empty(), &t) {
            for k in &selection {
                next.paint.insert(*k, v.unwrap_or(model::BLACK));
            }
        }
        let count = selection.len();
        if ui.button(t("common.selectAll")).clicked() {
            selection = layout.iter().map(|k| k.key.key).collect();
        }
        if widgets::button(ui, t("common.clearSelection"), count > 0).clicked() {
            selection.clear();
        }
        let hint = if count > 0 { easyrazer_backend::i18n::tf(&lang, "common.selected", &[("n", &count.to_string())]) } else { t("common.selectHint") };
        ui.label(RichText::new(hint).color(theme::MUTED));
    });
    ui.label(RichText::new(t("lighting.customWarning")).color(theme::WARN));
    app.lighting.layout = layout;
    app.lighting.selection = selection;
    app.lighting.map = state;
}
