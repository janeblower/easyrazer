//! Developer mode: the keyboard traffic log, the command console and the bug-report text.

mod console;
mod traffic;

use std::collections::{BTreeMap, BTreeSet};

use easyrazer_backend::api;
use easyrazer_backend::dev::{DevKey, DeviceInfo};
use easyrazer_backend::settings::{DevPanel, DevWindow, Dock};
use egui::{RichText, Stroke, Ui};
use razer_core::logged::Entry;

use crate::app::App;
use crate::theme;
use console::{Console, Registry};
use traffic::Traffic;

/// Lines of raw key data under the map before the rest is summed up.
const KEYS_SHOWN: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Traffic,
    Device,
}

#[derive(Default)]
pub struct Dev {
    log: Vec<Entry>,
    registry: Registry,
    /// The exchange whose details are open, by `seq`; the console opens it too.
    selected: Option<u64>,
    /// Exchanges this window's console sent.
    from_console: BTreeSet<u64>,
    tab: Tab,
    drawer: bool,
    uncensored: bool,
    note: String,
    info: Option<Option<DeviceInfo>>,
    traffic: Traffic,
    console: Console,
    /// Raw slot data of the keys under the map, and the keys it was read for.
    keys: Option<(Vec<u8>, BTreeMap<u8, DevKey>)>,
    macro_bytes: Option<(u16, Option<Vec<u8>>)>,
    /// The firmware mode, the startup slot and the loaded profile change with these, so the
    /// device tab is read again when they do.
    info_for: Option<(bool, bool, Option<u32>)>,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// Getters have bit 7 of the id set.
pub fn is_write(cmd: &str) -> bool {
    cmd.get(3..5).and_then(|id| u8::from_str_radix(id, 16).ok()).is_some_and(|id| id & 0x80 == 0)
}

pub fn is_error(e: &Entry) -> bool {
    e.error.is_some() || e.status.as_deref().is_some_and(|s| s != "Ok")
}

pub fn name_of<'a>(registry: &'a Registry, cmd: &str) -> Option<&'a str> {
    registry.iter().find(|(_, c)| c.cmd.to_string() == cmd).map(|(n, _)| n.as_str())
}

pub fn mode(app: &App) -> bool {
    app.settings_view.as_ref().map_or_else(|| app.jobs.backend().settings().dev_mode, |s| s.dev_mode)
}

fn panel_settings(app: &App) -> Option<DevPanel> {
    app.settings_view.as_ref().map_or_else(|| app.jobs.backend().settings().dev_panel.clone(), |s| s.dev_panel.clone())
}

pub fn panel_open(app: &App) -> bool {
    panel_settings(app).is_some_and(|p| p.open)
}

fn set_panel(app: &mut App, change: impl FnOnce(&mut DevPanel)) {
    let Some(mut p) = panel_settings(app) else { return };
    change(&mut p);
    if let Err(e) = app.jobs.backend().set_dev_panel(p) {
        app.show_error(e);
    }
    crate::settings::reload(app);
}

pub fn toggle_panel(app: &mut App) {
    set_panel(app, |p| p.open = !p.open);
}

pub fn on_log(app: &mut App, id: String, entry: Entry) {
    if !app.ours(Some(&id)) || app.dev.log.last().is_some_and(|e| e.seq >= entry.seq) {
        return;
    }
    app.dev.log.push(entry);
    let size = app.settings_view.as_ref().map_or(10_000, |s| s.dev_log_size);
    if app.dev.log.len() > size {
        let over = app.dev.log.len() - size;
        app.dev.log.drain(..over);
    }
}

/// Settings changed elsewhere: the registry is read on the first turn into developer mode.
pub fn on_settings(app: &mut App) {
    if mode(app) && app.dev.registry.is_empty() {
        app.dev.registry = api::dev_commands();
        let s = app.jobs.backend().app_settings();
        app.dev.console.restore(s.dev_recent);
        if let Some(v) = &s.dev_traffic {
            app.dev.traffic.restore(v);
        }
        reload_log(app);
    }
}

/// Refetches the log of the shown keyboard.
pub fn reload_log(app: &mut App) {
    app.dev.log.clear();
    app.dev.info = None;
    app.dev.keys = None;
    app.dev.macro_bytes = None;
    let Some(id) = app.keyboard.clone() else { return };
    let worker_id = id.clone();
    app.jobs.run(move |b| b.dev_log(&worker_id), move |app, r| {
        let Ok(fetched) = r else { return };
        if !app.ours(Some(&id)) {
            return;
        }
        // Entries that arrived while the log was fetched.
        let last = fetched.last().map(|e| e.seq);
        let newer: Vec<Entry> = app.dev.log.drain(..).filter(|e| last.is_none_or(|l| e.seq > l)).collect();
        app.dev.log = fetched;
        app.dev.log.extend(newer);
    });
}

/// The panel docked to an edge or in its own window, while developer mode has it open.
pub fn panel(app: &mut App, ui: &mut Ui) {
    if !mode(app) {
        return;
    }
    if app.dev.registry.is_empty() {
        on_settings(app);
    }
    let Some(p) = panel_settings(app).filter(|p| p.open) else { return };
    let size = p.size as f32;
    let frame = egui::Frame::new().fill(theme::PANEL).inner_margin(6).stroke(Stroke::new(2.0, theme::LINE));
    let shown = match p.dock {
        Dock::Left => Some(egui::Panel::left("dev").resizable(true).default_size(size).frame(frame).show(ui, |ui| content(app, ui, false)).response),
        Dock::Right => Some(egui::Panel::right("dev").resizable(true).default_size(size).frame(frame).show(ui, |ui| content(app, ui, false)).response),
        Dock::Bottom => Some(egui::Panel::bottom("dev").resizable(true).default_size(size).frame(frame).show(ui, |ui| content(app, ui, false)).response),
        Dock::Window => {
            window(app, ui.ctx());
            None
        }
    };
    if let Some(r) = shown {
        let now = if p.dock == Dock::Bottom { r.rect.height() } else { r.rect.width() };
        if ui.input(|i| i.pointer.any_released()) && (now - size).abs() > 1.0 {
            set_panel(app, |p| p.size = now.round() as u32);
        }
    }
}

fn window(app: &mut App, ctx: &egui::Context) {
    let geom = app.settings_view.as_ref().and_then(|s| s.dev_window);
    let mut builder = egui::ViewportBuilder::default().with_title(app.t("dev.title")).with_min_inner_size([480.0, 320.0]);
    builder = match geom {
        Some(g) => builder.with_position([g.x as f32, g.y as f32]).with_inner_size([g.w as f32, g.h as f32]),
        None => builder.with_inner_size([900.0, 560.0]),
    };
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("dev-window"), builder, |ui, _| {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::PANEL).inner_margin(6)).show(ui, |ui| content(app, ui, true));
        let (close, rect) = ui.input(|i| (i.viewport().close_requested(), i.viewport().outer_rect));
        if let Some(r) = rect {
            let g = DevWindow { x: f64::from(r.left()), y: f64::from(r.top()), w: f64::from(r.width()), h: f64::from(r.height()) };
            let moved = app.settings_view.as_ref().and_then(|s| s.dev_window) != Some(g);
            if moved && ui.input(|i| i.pointer.any_released() || close) {
                let _ = app.jobs.backend().set_dev_window(g);
                crate::settings::reload(app);
            }
        }
        if close {
            set_panel(app, |p| p.open = false);
        }
    });
}

const DOCKS: [(Dock, &str, &str); 4] = [(Dock::Left, "◧", "left"), (Dock::Bottom, "⬓", "bottom"), (Dock::Right, "◨", "right"), (Dock::Window, "⎘", "window")];

/// `detached` is the panel's own window, where every Esc is the panel's.
fn content(app: &mut App, ui: &mut Ui, detached: bool) {
    let area = ui.max_rect();
    // An Esc that closes a menu is the menu's.
    let menu_open = egui::Popup::is_any_open(ui.ctx());
    ui.horizontal(|ui| {
        for (tab, key) in [(Tab::Traffic, "dev.tabs.traffic"), (Tab::Device, "dev.tabs.device")] {
            let on = app.dev.tab == tab;
            if ui.selectable_label(on, RichText::new(app.t(key)).color(if on { theme::TEXT } else { theme::MUTED })).clicked() {
                app.dev.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✕").on_hover_text(app.t("dev.close")).clicked() {
                set_panel(app, |p| p.open = false);
            }
            ui.menu_button("⋮", |ui| menu(app, ui)).response.on_hover_text(app.t("dev.menu.open"));
            let drawer = app.dev.drawer;
            if ui.selectable_label(drawer, RichText::new("›_").monospace()).on_hover_text(app.t("dev.drawer.toggle")).clicked() {
                app.dev.drawer = !drawer;
            }
        });
    });
    ui.separator();
    // Fixed rects: content sized from what is left would push the panel taller every frame.
    let full = ui.available_rect_before_wrap();
    let console_height = if app.dev.drawer { (full.height() * 0.4).clamp(120.0, 320.0).min(full.height() - 80.0).max(0.0) } else { 0.0 };
    let split = full.max.y - console_height;
    let main = egui::Rect::from_min_max(full.min, egui::pos2(full.max.x, split));
    ui.scope_builder(egui::UiBuilder::new().max_rect(main).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(main.intersect(ui.clip_rect()));
        match app.dev.tab {
            Tab::Traffic => traffic::ui(app, ui),
            Tab::Device => {
                egui::ScrollArea::vertical().id_salt("dev-device").auto_shrink(false).show(ui, |ui| device(app, ui));
            }
        }
    });
    if app.dev.drawer {
        let rect = egui::Rect::from_min_max(egui::pos2(full.min.x, split), full.max);
        let open = ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.separator();
            console::ui(app, ui)
        });
        if let Some(seq) = open.inner {
            app.dev.selected = Some(seq);
            app.dev.tab = Tab::Traffic;
        }
    }
    ui.advance_cursor_after_rect(full);
    // The console has taken the Esc that closes its own list or question by now.
    let ours = !menu_open && (detached || ui.rect_contains_pointer(area));
    if ours && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.dev.drawer = !app.dev.drawer;
    }
}

fn menu(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new(app.t("dev.menu.dock")).small().color(theme::MUTED));
    let current = panel_settings(app).map(|p| p.dock);
    ui.horizontal(|ui| {
        for (dock, glyph, key) in DOCKS {
            let tip = app.t(&format!("dev.dock.{key}"));
            if ui.selectable_label(current == Some(dock), glyph).on_hover_text(tip).clicked() {
                set_panel(app, |p| p.dock = dock);
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button(format!("⎘ {}", app.t("dev.copyDiagnostics"))).clicked() {
        copy_diagnostics(app, ui.ctx().clone());
    }
    let label = app.t("dev.uncensored");
    ui.checkbox(&mut app.dev.uncensored, label).on_hover_text(app.t("dev.uncensoredHint"));
    if !app.dev.note.is_empty() {
        ui.label(RichText::new(&app.dev.note).small().color(theme::MUTED));
    }
}

fn copy_diagnostics(app: &mut App, ctx: egui::Context) {
    let Some(id) = app.keyboard.clone() else { return };
    let uncensored = app.dev.uncensored;
    app.jobs.run(move |b| b.dev_diagnostics(&id, uncensored), move |app, r| match r {
        Ok(text) => {
            ctx.copy_text(text);
            app.dev.note = app.t("dev.copied");
        }
        Err(e) => app.dev.note = e,
    });
}

fn device(app: &mut App, ui: &mut Ui) {
    let now = app.status.as_ref().map(|s| (s.device, s.driver_mode, s.profile));
    if app.dev.info.is_none() || (app.dev.info_for.is_some() && app.dev.info_for != now) {
        load_info(app);
        app.dev.info.get_or_insert(None);
    }
    app.dev.info_for = now;
    match app.dev.info.as_ref().and_then(|i| i.as_ref()) {
        None => {
            ui.label(RichText::new(app.t("dev.noDevice")).color(theme::MUTED));
        }
        Some(i) => {
            let opt = |v: Option<String>| v.unwrap_or_else(|| "?".into());
            let rows = [
                ("model", i.model.clone()),
                ("ids", format!("{:04X}:{:04X}", i.vid, i.pid)),
                ("interface", i.interface.to_string()),
                ("tid", format!("{:02X}", i.tid)),
                ("lighting", format!("{} · led {:02X}", i.lighting_protocol, i.led)),
                ("lampArray", i.lamp_array.to_string()),
                ("mode", opt(i.mode.map(|m| format!("{m:02X}")))),
                ("startup", opt(i.startup_slot.map(|s| s.to_string()))),
                ("loaded", opt(i.loaded_profile.map(|p| p.to_string()))),
                ("firmware", opt(i.firmware.clone())),
                ("serial", opt(i.serial.clone())),
            ];
            egui::Grid::new("dev-device").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(app.t(&format!("dev.info.{k}"))).monospace().small().color(theme::MUTED));
                    ui.add(egui::Label::new(RichText::new(v).monospace().small()).selectable(true));
                    ui.end_row();
                }
            });
        }
    }
    if ui.button(app.t("dev.refresh")).clicked() {
        load_info(app);
    }
}

fn load_info(app: &mut App) {
    let Some(id) = app.keyboard.clone() else { return };
    app.jobs.run(move |b| b.dev_device(&id), |app, r| app.dev.info = Some(r.ok().flatten()));
}

/// A monospace block of raw data, as the window shows it in developer mode.
fn block(ui: &mut Ui, lines: &[String]) {
    egui::Frame::new().fill(theme::KEY_OFF).stroke(Stroke::new(1.0, theme::DEV.gamma_multiply(0.5))).corner_radius(theme::RADIUS).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for l in lines {
            ui.add(egui::Label::new(RichText::new(l).monospace().small().color(theme::DEV)).selectable(true));
        }
    });
}

/// The raw slot data of the selected keys, under the map.
pub fn keys(app: &mut App, ui: &mut Ui) {
    if !mode(app) {
        return;
    }
    let selected: Vec<u8> = app.keys.selection.iter().copied().collect();
    if app.dev.keys.as_ref().is_none_or(|(k, _)| *k != selected) {
        app.dev.keys = Some((selected.clone(), BTreeMap::new()));
        if let Some(id) = app.keyboard.clone() {
            let asked = selected.clone();
            let wanted = selected.clone();
            app.jobs.run(move |b| b.dev_keys(&id, asked), move |app, r| {
                let current = app.dev.keys.as_ref().is_some_and(|(k, _)| *k == wanted);
                if let (Ok(map), true) = (r, current) {
                    app.dev.keys = Some((wanted, map));
                }
            });
        }
    }
    let raw = |r: &Option<razer_core::profiles::RawKey>| r.as_ref().map_or("—".to_string(), |r| format!("thr {:02X}/{:02X} fn {:02X} {}", r.thr_low, r.thr_high, r.fn_id, hex(&r.fn_data)));
    let mut lines: Vec<String> = app
        .dev
        .keys
        .as_ref()
        .map(|(_, m)| m.iter().take(KEYS_SHOWN).map(|(k, d)| format!("{k:3} {:<12} {} | hs {}", d.name, raw(&d.normal), raw(&d.hypershift))).collect())
        .unwrap_or_default();
    if selected.len() > KEYS_SHOWN {
        lines.push(app.tf("dev.more", &[("n", &(selected.len() - KEYS_SHOWN).to_string())]));
    }
    if app.section == crate::app::Section::Actuation {
        let (depth, travel, down, _) = app.lead;
        lines.push(if app.driver() { format!("depth {depth} travel {travel} {}", if down { "down" } else { "up" }) } else { app.t("dev.needDriver") });
    }
    if !lines.is_empty() {
        block(ui, &lines);
    }
}

/// The selected macro's body as the keyboard stores it.
pub fn macro_bytes(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.macros.selected().filter(|_| mode(app)) else { return };
    if app.dev.macro_bytes.as_ref().is_none_or(|(m, _)| *m != id) {
        app.dev.macro_bytes = Some((id, None));
        if let Some(kb) = app.keyboard.clone() {
            app.jobs.run(move |b| b.dev_macro(&kb, id), move |app, r| app.dev.macro_bytes = Some((id, r.ok().flatten())));
        }
    }
    let bytes = app.dev.macro_bytes.as_ref().and_then(|(_, b)| b.clone()).unwrap_or_default();
    let free = app.macro_state.as_ref().and_then(|m| m.free).map_or("—".into(), |f| f.to_string());
    block(ui, &[format!("id {id} · {} B · free {free}", bytes.len()), hex(&bytes)]);
}

/// The last lighting commands and the keyboard's answers.
pub fn lighting_log(app: &mut App, ui: &mut Ui) {
    if !mode(app) {
        return;
    }
    let lines: Vec<String> = ["set_effect", "set_brightness", "get_effect", "get_brightness"]
        .iter()
        .map(|name| {
            let last = app.dev.log.iter().rev().find(|e| name_of(&app.dev.registry, &e.cmd) == Some(name));
            match last {
                Some(e) => format!("{name} {} {} ← {} {}", e.cmd, hex(&e.args), e.status.as_deref().or(e.error.as_deref()).unwrap_or("—"), hex(&e.reply)),
                None => format!("{name} —"),
            }
        })
        .collect();
    block(ui, &lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_has_bit_7_of_its_id_clear() {
        assert!(is_write("0F:02"));
        assert!(!is_write("00:84"));
        assert!(!is_write("label"));
    }

    #[test]
    fn a_status_other_than_ok_is_an_error() {
        let e = |status: Option<&str>, error: Option<&str>| Entry { status: status.map(Into::into), error: error.map(Into::into), ..Default::default() };
        assert!(!is_error(&e(Some("Ok"), None)));
        assert!(is_error(&e(Some("Failure"), None)));
        assert!(is_error(&e(None, Some("timeout"))));
    }
}
