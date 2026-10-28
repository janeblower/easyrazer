//! The macro library and its editor: steps typed by hand or recorded from the keyboard.

use std::time::Instant;

use easyrazer_backend::device::MacroState;
use egui::{RichText, Ui};
use razer_core::binding::Mouse;
use razer_core::macros::Event;

use super::binding::MOUSE;
use crate::app::{App, Section, Tab};
use crate::icons::Icon;
use crate::jobs::Done;
use crate::text::tag;
use crate::{theme, widgets};

/// The keyboard keeps names this long.
const NAME_MAX: usize = 40;
const DELAY_MAX: u32 = 600_000;

#[derive(Clone, PartialEq)]
struct Draft {
    name: String,
    events: Vec<Event>,
}

pub struct Editor {
    /// `None` while a new macro is not applied yet.
    selected: Option<u16>,
    draft: Option<Draft>,
    busy: bool,
    message: String,
    /// The delete button asks for a second click.
    armed: bool,
    recording: bool,
    with_delays: bool,
    last: Option<Instant>,
    /// Modifiers as last seen while recording: egui reports them as state, not as key events.
    mods: egui::Modifiers,
}

impl Default for Editor {
    fn default() -> Self {
        Self { selected: None, draft: None, busy: false, message: String::new(), armed: false, recording: false, with_delays: true, last: None, mods: egui::Modifiers::NONE }
    }
}

impl Editor {
    pub fn selected(&self) -> Option<u16> {
        self.selected
    }
}

/// Bytes of the firmware body, as `macros::encode` in core writes it.
pub fn body_size(events: &[Event]) -> usize {
    events
        .iter()
        .map(|e| match e {
            Event::Delay { ms: 0 } => 0,
            Event::Delay { ms } if *ms <= 0xff => 2,
            Event::Delay { ms } if *ms <= 0xffff => 3,
            Event::Delay { ms } if *ms <= 0xff_ffff => 4,
            Event::Delay { .. } => 5,
            Event::Mouse { button: Mouse::WheelUp | Mouse::WheelDown, down: false } => 0,
            _ => 2,
        })
        .sum()
}

/// Flash a body takes: block tags and a header, in 8-byte units.
pub fn footprint(len: usize) -> usize {
    8 + (len + 6).div_ceil(8) * 8
}

fn saved(app: &App) -> Option<Draft> {
    let id = app.macros.selected?;
    app.macro_state.as_ref()?.macros.get(&id).map(|m| Draft { name: m.name.clone(), events: m.events.clone() })
}

fn dirty(app: &App) -> bool {
    app.macros.draft.as_ref().is_some_and(|d| saved(app).as_ref() != Some(d))
}

pub fn pending(app: &App) -> usize {
    usize::from(dirty(app))
}

pub fn can_apply(app: &App) -> bool {
    dirty(app) && !app.macros.busy
}

/// An applied macro not yet in the flash is written from its own tab only: elsewhere it is not in view.
pub fn can_write(app: &App) -> bool {
    let active = app.tab == Tab::Keys && app.section == Section::Macros;
    let unwritten = app.macros.selected.and_then(|id| app.macro_state.as_ref()?.macros.get(&id)).is_some_and(|m| !m.written);
    app.writable() && !app.macros.busy && (dirty(app) || (active && unwritten))
}

pub fn revert(app: &mut App) {
    let id = app.macros.selected;
    open(app, id);
}

fn open(app: &mut App, id: Option<u16>) {
    app.macros.recording = false;
    app.macros.selected = id;
    app.macros.armed = false;
    let n = app.macro_state.as_ref().map_or(0, |m| m.macros.len()) + 1;
    let m = id.and_then(|id| app.macro_state.as_ref()?.macros.get(&id).cloned());
    app.macros.draft = Some(match m {
        Some(m) => Draft { name: m.name, events: m.events },
        None => Draft { name: app.tf("macros.newName", &[("n", &n.to_string())]), events: Vec::new() },
    });
    app.macros.message.clear();
}

/// Runs a macro command; `then` gets its result, the message says how it went.
fn run<T: Send + 'static>(
    app: &mut App,
    work: impl FnOnce(&easyrazer_backend::Backend, &str) -> Result<T, String> + Send + 'static,
    state: fn(&T) -> MacroState,
    done: &'static str,
    then: impl FnOnce(&mut App, T) + Send + 'static,
) {
    let Some(kb) = app.keyboard.clone() else { return };
    app.macros.busy = true;
    let worker_kb = kb.clone();
    app.jobs.run(move |b| work(b, &worker_kb), move |app, r| {
        app.macros.busy = false;
        match r {
            Ok(v) if app.ours(Some(&kb)) => {
                app.macro_state = Some(state(&v));
                app.macros.message = app.t(done);
                then(app, v);
            }
            Ok(_) => {}
            Err(e) => app.macros.message = e,
        }
    });
}

pub fn apply(app: &mut App, then: Done) {
    let Some(d) = app.macros.draft.clone() else {
        then(app);
        return;
    };
    app.macros.recording = false;
    let id = app.macros.selected;
    let name = if d.name.trim().is_empty() { app.t("macros.untitled") } else { d.name.trim().to_string() };
    run(app, move |b, kb| b.set_macro(kb, id, name, d.events), |(_, s)| s.clone(), "macros.applied", move |app, (id, state)| {
        app.macros.selected = Some(id);
        app.macros.draft = state.macros.get(&id).map(|m| Draft { name: m.name.clone(), events: m.events.clone() });
        then(app);
    });
}

pub fn write(app: &mut App, then: Done) {
    fn write_it(app: &mut App, then: Done) {
        let Some(id) = app.macros.selected else {
            then(app);
            return;
        };
        run(app, move |b, kb| b.write_macro(kb, id), MacroState::clone, "macros.written", move |app, _| then(app));
    }
    if dirty(app) {
        apply(app, Box::new(move |app: &mut App| write_it(app, then)));
    } else {
        write_it(app, then);
    }
}

fn remove(app: &mut App) {
    if !app.macros.armed {
        app.macros.armed = true;
        return;
    }
    let Some(id) = app.macros.selected else { return };
    run(app, move |b, kb| b.delete_macro(kb, id), MacroState::clone, "macros.deleted", |app, _| {
        app.macros.selected = None;
        app.macros.draft = None;
    });
}

/// Names of the keys each macro is bound to, applied or not.
fn bound(app: &App, id: u16) -> Vec<String> {
    app.layout
        .iter()
        .filter(|k| matches!(app.keys.binding_of(k.key.key), Some(razer_core::binding::Action::Macro { id: m, .. }) if m == id))
        .map(|k| super::key_name(app, k))
        .collect()
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    if app.macros.recording {
        record(app, ui);
    }
    let list: Vec<(u16, String, bool)> = app.macro_state.as_ref().map(|m| m.macros.iter().map(|(id, m)| (*id, m.name.clone(), m.written)).collect()).unwrap_or_default();
    let free = app.macro_state.as_ref().and_then(|m| m.free);
    if list.is_empty() && app.macros.draft.is_none() {
        widgets::card(ui, |ui| {
            widgets::empty_state(ui, Icon::macro_, &app.t("macros.noneTitle"), &app.t("macros.noneText"));
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 300.0).max(0.0) / 2.0);
                    if widgets::primary(ui, app.t("macros.recordNew"), true).clicked() {
                        open(app, None);
                        start(app);
                    }
                    if ui.button(app.t("macros.createEmpty")).clicked() {
                        open(app, None);
                    }
                });
                if let Some(n) = free {
                    widgets::hint(ui, app.tf("macros.free", &[("n", &n.to_string())]));
                }
            });
        });
        return;
    }
    widgets::card(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(180.0);
                for (id, name, written) in &list {
                    let on = app.macros.selected == Some(*id);
                    let keys = bound(app, *id).join(", ");
                    let mark = if *written { "" } else { " ●" };
                    let b = egui::Button::new(RichText::new(format!("{id}. {}{mark}", crate::shell::truncate(name, 16))))
                        .stroke(egui::Stroke::new(1.0, if on { theme::ACCENT } else { theme::LINE }))
                        .right_text(RichText::new(crate::shell::truncate(&keys, 10)).small().color(theme::MUTED))
                        .min_size(egui::vec2(180.0, 30.0));
                    let mut r = ui.add(b);
                    if !*written {
                        r = r.on_hover_text(app.t("macros.notWritten"));
                    }
                    if r.clicked() {
                        open(app, Some(*id));
                    }
                }
                if ui.add(egui::Button::new(app.t("macros.new")).min_size(egui::vec2(180.0, 30.0))).clicked() {
                    open(app, None);
                }
            });
            ui.vertical(|ui| {
                if app.macros.draft.is_none() {
                    widgets::empty_state(ui, Icon::macro_, &app.t("macros.pickTitle"), &app.t("macros.pickText"));
                    return;
                }
                editor(app, ui, free);
            });
        });
    });
}

fn editor(app: &mut App, ui: &mut Ui, free: Option<u32>) {
    let keys: Vec<(u8, String)> = app.layout.iter().filter(|k| k.key.editable).map(|k| (k.key.key, super::key_name(app, k))).collect();
    let mouse: Vec<String> = MOUSE.iter().map(|b| app.t(&format!("bindings.mouse.{}", tag(b)))).collect();
    let (down, up) = (app.t("macros.down"), app.t("macros.up"));
    let (pause, ms) = (app.t("macros.delay"), app.t("macros.ms"));
    let tips = [app.t("macros.moveUp"), app.t("macros.moveDown"), app.t("macros.remove")];
    let name_label = app.t("macros.name");
    let Some(d) = &mut app.macros.draft else { return };
    ui.horizontal(|ui| {
        ui.add_sized([100.0, 24.0], egui::Label::new(RichText::new(name_label).color(theme::MUTED)));
        ui.add(egui::TextEdit::singleline(&mut d.name).char_limit(NAME_MAX).desired_width(f32::INFINITY));
    });
    let mut swap = None;
    let mut drop = None;
    egui::ScrollArea::vertical().max_height(320.0).id_salt("steps").show(ui, |ui| {
        let n = d.events.len();
        for (i, e) in d.events.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_sized([24.0, 24.0], egui::Label::new(RichText::new((i + 1).to_string()).small().color(theme::MUTED)));
                match e {
                    Event::Delay { ms: v } => {
                        ui.add_sized([130.0, 24.0], egui::Label::new(&pause));
                        ui.add(egui::DragValue::new(v).range(0..=DELAY_MAX));
                        ui.label(&ms);
                    }
                    Event::Key { key, down: is_down } => {
                        step_down(ui, i, is_down, &down, &up);
                        let name = keys.iter().find(|(k, _)| k == key).map_or("?".to_string(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt(("step-key", i)).selected_text(name).width(170.0).height(300.0).show_ui(ui, |ui| {
                            for (k, n) in &keys {
                                ui.selectable_value(key, *k, n);
                            }
                        });
                    }
                    Event::Mouse { button, down: is_down } => {
                        step_down(ui, i, is_down, &down, &up);
                        let at = MOUSE.iter().position(|b| b == button).unwrap_or(0);
                        egui::ComboBox::from_id_salt(("step-mouse", i)).selected_text(&mouse[at]).width(170.0).show_ui(ui, |ui| {
                            for (j, b) in MOUSE.iter().enumerate() {
                                ui.selectable_value(button, *b, &mouse[j]);
                            }
                        });
                    }
                }
                if widgets::icon_button(ui, Icon::up, &tips[0], i > 0, theme::TEXT).clicked() {
                    swap = Some((i, i - 1));
                }
                if widgets::icon_button(ui, Icon::down, &tips[1], i + 1 < n, theme::TEXT).clicked() {
                    swap = Some((i, i + 1));
                }
                if widgets::icon_button(ui, Icon::remove, &tips[2], true, theme::TEXT).clicked() {
                    drop = Some(i);
                }
            });
        }
    });
    if let Some((a, b)) = swap {
        d.events.swap(a, b);
    }
    if let Some(i) = drop {
        d.events.remove(i);
    }
    let empty = d.events.is_empty();
    let size = footprint(body_size(&d.events));
    if empty {
        widgets::hint(ui, app.t("macros.empty"));
    }
    let first_key = keys.first().map_or(1, |(k, _)| *k);
    ui.horizontal_wrapped(|ui| {
        let recording = app.macros.recording;
        let text = RichText::new(app.t(if recording { "macros.stop" } else { "macros.record" })).color(if recording { theme::ERROR } else { theme::TEXT });
        let b = egui::Button::new(text).stroke(egui::Stroke::new(1.0, if recording { theme::ERROR } else { theme::LINE }));
        if ui.add(b).clicked() {
            if recording {
                app.macros.recording = false;
            } else {
                start(app);
            }
        }
        let with_delays = app.t("macros.withDelays");
        ui.checkbox(&mut app.macros.with_delays, with_delays);
        let mut add: Vec<Event> = Vec::new();
        if ui.button(app.t("macros.addKey")).clicked() {
            add = vec![Event::Key { key: first_key, down: true }, Event::Key { key: first_key, down: false }];
        }
        if ui.button(app.t("macros.addMouse")).clicked() {
            add = vec![Event::Mouse { button: Mouse::Left, down: true }, Event::Mouse { button: Mouse::Left, down: false }];
        }
        if ui.button(app.t("macros.addDelay")).clicked() {
            add = vec![Event::Delay { ms: 50 }];
        }
        let clear = widgets::button(ui, app.t("macros.clear"), !empty).clicked();
        if let Some(d) = &mut app.macros.draft {
            d.events.extend(add);
            if clear {
                d.events.clear();
            }
        }
    });
    if app.macros.recording {
        widgets::hint(ui, app.t("macros.recordingHint"));
    }
    let n = size.to_string();
    widgets::hint(ui, match free {
        Some(f) => app.tf("macros.sizeFree", &[("n", &n), ("free", &f.to_string())]),
        None => app.tf("macros.size", &[("n", &n)]),
    });
    ui.horizontal(|ui| {
        if !app.macros.message.is_empty() {
            widgets::hint(ui, app.macros.message.clone());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(id) = app.macros.selected {
                let keys = bound(app, id);
                let text = app.t(if app.macros.armed { "macros.confirmDelete" } else { "macros.delete" });
                let r = ui.add_enabled(!app.macros.busy && keys.is_empty(), egui::Button::new(text));
                let r = if keys.is_empty() { r } else { r.on_disabled_hover_text(app.tf("macros.boundTo", &[("keys", &keys.join(", "))])) };
                if r.clicked() {
                    remove(app);
                }
            }
        });
    });
    crate::dev::macro_bytes(app, ui);
}

fn step_down(ui: &mut Ui, i: usize, down: &mut bool, down_text: &str, up_text: &str) {
    egui::ComboBox::from_id_salt(("step-down", i)).selected_text(if *down { down_text } else { up_text }).width(120.0).show_ui(ui, |ui| {
        ui.selectable_value(down, true, down_text);
        ui.selectable_value(down, false, up_text);
    });
}

fn start(app: &mut App) {
    app.macros.recording = true;
    app.macros.last = None;
    app.macros.mods = egui::Modifiers::NONE;
}

/// Takes the keys pressed this frame into the draft; they go into the macro, not to the widgets.
fn record(app: &mut App, ui: &mut Ui) {
    let (events, mods) = ui.input_mut(|i| {
        let taken: Vec<(egui::Key, bool, bool)> = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, physical_key, pressed, repeat, .. } => Some((physical_key.unwrap_or(*key), *pressed, *repeat)),
                _ => None,
            })
            .collect();
        i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)));
        (taken, i.modifiers)
    });
    if let Some(id) = ui.ctx().memory(|m| m.focused()) {
        ui.ctx().memory_mut(|m| m.surrender_focus(id));
    }
    let mut steps = Vec::new();
    // Modifiers come as a state; a change is a press or a release of the left one.
    let before = app.macros.mods;
    for (on, was, name) in [(mods.shift, before.shift, "LEFT_SHIFT"), (mods.ctrl, before.ctrl, "LEFT_CTRL"), (mods.alt, before.alt, "LEFT_ALT")] {
        if on != was {
            steps.push((name.to_string(), on));
        }
    }
    app.macros.mods = mods;
    for (key, pressed, repeat) in events {
        if !repeat && let Some(name) = keymap_name(key) {
            steps.push((name, pressed));
        }
    }
    for (name, down) in steps {
        let Some(key) = app.layout.iter().find(|k| k.name == name).map(|k| k.key.key) else { continue };
        let now = Instant::now();
        let with_delays = app.macros.with_delays;
        let last = app.macros.last.replace(now);
        let Some(d) = &mut app.macros.draft else { continue };
        if with_delays
            && let Some(t) = last
            && !d.events.is_empty()
        {
            d.events.push(Event::Delay { ms: now.duration_since(t).as_millis().min(u128::from(DELAY_MAX)) as u32 });
        }
        d.events.push(Event::Key { key, down });
    }
}

/// Keymap name of a key as egui reports it, for recording.
fn keymap_name(k: egui::Key) -> Option<String> {
    use egui::Key::*;
    let fixed = match k {
        Backtick => "TILDE",
        Minus => "HYPEN",
        Equals => "EQUAL",
        Backspace => "BACKSPACE",
        Tab => "TAB",
        OpenBracket => "OPEN_SQUARE_BRACKET",
        CloseBracket => "CLOSE_SQUARE_BRACKET",
        Backslash => "BACKSLASH",
        Semicolon => "SEMICOLON",
        Quote => "APOSTROPHE",
        Enter => "ENTER",
        Comma => "COMMA",
        Period => "PERIOD",
        Slash => "SLASH",
        Space => "SPACEBAR",
        Insert => "INSERT",
        Delete => "DELETE",
        Home => "HOME",
        End => "END",
        PageUp => "PAGE_UP",
        PageDown => "PAGE_DOWN",
        ArrowLeft => "LEFT_ARROW",
        ArrowUp => "UP_ARROW",
        ArrowDown => "DOWN_ARROW",
        ArrowRight => "RIGHT_ARROW",
        Escape => "ESC",
        Plus => "NUMPAD_PLUS",
        _ => "",
    };
    if !fixed.is_empty() {
        return Some(fixed.into());
    }
    let name = k.name();
    // Letters, digits and F-keys go by their own names.
    let digit = name.strip_prefix("Num").filter(|d| d.len() == 1);
    Some(digit.unwrap_or(name).to_uppercase()).filter(|n| n.len() == 1 || (n.starts_with('F') && n[1..].parse::<u8>().is_ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_counts_like_the_firmware_writes_it() {
        let events = [Event::Key { key: 4, down: true }, Event::Delay { ms: 300 }, Event::Mouse { button: Mouse::WheelUp, down: false }, Event::Delay { ms: 0 }];
        assert_eq!(body_size(&events), 2 + 3);
        assert_eq!(footprint(0), 16);
        assert_eq!(footprint(10), 24);
    }

    #[test]
    fn recorded_keys_map_to_keymap_names() {
        assert_eq!(keymap_name(egui::Key::A).as_deref(), Some("A"));
        assert_eq!(keymap_name(egui::Key::Num7).as_deref(), Some("7"));
        assert_eq!(keymap_name(egui::Key::F12).as_deref(), Some("F12"));
        assert_eq!(keymap_name(egui::Key::Backtick).as_deref(), Some("TILDE"));
    }
}
