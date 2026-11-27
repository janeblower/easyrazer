//! The command console: lines typed as `name bytes…` or `CC:II bytes…`, sent to the keyboard
//! after a question when they would write its flash.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use razer_core::logged::Entry;
use easyrazer_backend::settings::DEV_RECENT_MAX;
use razer_core::registry::{CommandInfo, Flash};

use super::{hex, is_error};
use crate::app::App;
use crate::theme;

/// `packet::ARGS_LEN`.
const ARGS_LEN: usize = 80;

pub type Registry = BTreeMap<String, CommandInfo>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub class: u8,
    pub id: u8,
    pub size: u8,
    pub args: Vec<u8>,
}

impl Parsed {
    /// Getters have bit 7 of the id set.
    fn write(&self) -> bool {
        self.id & 0x80 == 0
    }
}

fn byte(t: &str) -> Option<u8> {
    (t.len() <= 2).then(|| u8::from_str_radix(t, 16).ok()).flatten()
}

fn code(t: &str) -> Option<(u8, u8)> {
    let (c, i) = t.split_once(':')?;
    (c.len() == 2 && i.len() == 2).then(|| Some((u8::from_str_radix(c, 16).ok()?, u8::from_str_radix(i, 16).ok()?))).flatten()
}

/// `name bytes…` or `CC:II bytes…`; bytes and `size=NN` in hex. The error is a message.
pub fn parse(line: &str, registry: &Registry) -> Result<Parsed, String> {
    let mut words = line.split_whitespace();
    let head = words.next().ok_or("empty")?;
    let info = registry.get(&head.to_lowercase());
    let (class, id) = info.map(|i| (i.cmd.class, i.cmd.id)).or_else(|| code(head)).ok_or_else(|| format!("unknown command {head}"))?;
    let mut size = None;
    let mut args = Vec::new();
    for t in words {
        if let Some(v) = t.to_lowercase().strip_prefix("size=") {
            size = Some(byte(v).ok_or_else(|| format!("bad size {v}"))?);
        } else {
            args.push(byte(t).ok_or_else(|| format!("bad byte {t}"))?);
        }
    }
    if args.len() > ARGS_LEN {
        return Err(format!("more than {ARGS_LEN} bytes"));
    }
    let size = size.or(info.and_then(|i| i.size)).unwrap_or(args.len() as u8);
    Ok(Parsed { class, id, size, args })
}

/// Why a line is confirmed before it is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Slot(u8),
    Flash,
    Unknown,
    Unusual { value: u8, help: String },
}

/// A flash write or an unusual value asks first; `None` sends at once.
pub fn confirmation(p: &Parsed, registry: &Registry) -> Option<Confirm> {
    if !p.write() {
        return None;
    }
    // What an unknown write does is anyone's guess, so it is confirmed like the worst case.
    let Some(info) = registry.values().find(|c| c.cmd.class == p.class && c.cmd.id == p.id) else { return Some(Confirm::Unknown) };
    if let Some((arg, values)) = &info.safe {
        let value = p.args.get(*arg as usize).copied().unwrap_or(0);
        if !values.contains(&value) {
            return Some(Confirm::Unusual { value, help: info.help.clone() });
        }
    }
    match info.flash {
        Some(Flash::Always(true)) => Some(Confirm::Flash),
        Some(Flash::Arg(a)) => match p.args.get(a as usize).copied().unwrap_or(0) {
            0 => None,
            slot => Some(Confirm::Slot(slot)),
        },
        _ => None,
    }
}

/// Recent lines first while nothing is typed, then the commands whose name, code or help hold the first word.
pub fn suggest(input: &str, registry: &Registry, recent: &[String]) -> Vec<(String, Option<CommandInfo>)> {
    let q = input.split_whitespace().next().unwrap_or("").to_lowercase();
    let commands = registry
        .iter()
        .filter(|(name, c)| q.is_empty() || name.contains(&q) || c.cmd.to_string().to_lowercase().contains(&q) || c.help.to_lowercase().contains(&q))
        .map(|(n, c)| (n.clone(), Some(c.clone())));
    if q.is_empty() { recent.iter().map(|r| (r.clone(), None)).chain(commands).collect() } else { commands.collect() }
}

/// The line with the picked item in place of its first word; a recent line replaces it whole.
pub fn completion(line: &str, item: &(String, Option<CommandInfo>)) -> String {
    let (name, info) = item;
    if info.is_none() {
        return name.clone();
    }
    let rest: Vec<&str> = line.split_whitespace().skip(1).collect();
    let mut words = vec![name.as_str()];
    words.extend(&rest);
    format!("{}{}", words.join(" "), if rest.is_empty() { " " } else { "" })
}

/// Enter takes the picked item, unless the line already names a command and the arrows were not used.
pub fn accepts(line: &str, next: &str, navigated: bool, registry: &Registry) -> bool {
    if next == line {
        return false;
    }
    let head = line.split_whitespace().next().unwrap_or("");
    let known = registry.contains_key(&head.to_lowercase()) || code(head).is_some();
    navigated || !known
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    In,
    Out,
    Error,
    Warn,
    Note,
}

#[derive(Default)]
pub struct Console {
    line: String,
    out: Vec<(Kind, String, Option<u64>)>,
    recent: Vec<String>,
    pending: Option<(Parsed, String)>,
    active: usize,
    focus: bool,
    list_open: bool,
    /// An item was picked with the arrows.
    navigated: bool,
    /// The line the list was built for; a new one starts it from the top.
    listed: String,
    /// Clicking the list takes the focus from the line; that is not leaving the console.
    popup_hovered: bool,
}

impl Console {
    pub fn restore(&mut self, recent: Vec<String>) {
        self.recent = recent;
    }

    fn print(&mut self, kind: Kind, text: impl Into<String>, seq: Option<u64>) {
        self.out.push((kind, text.into(), seq));
    }
}

fn send(app: &mut App, p: Parsed, text: String) {
    let c = &mut app.dev.console;
    c.recent.retain(|r| *r != text);
    c.recent.insert(0, text.clone());
    c.recent.truncate(DEV_RECENT_MAX);
    c.line.clear();
    if let Err(e) = app.jobs.backend().add_dev_recent(text) {
        app.show_error(e);
    }
    let Some(id) = app.keyboard.clone() else {
        let none = app.t("dev.noDevice");
        app.dev.console.print(Kind::Error, none, None);
        return;
    };
    app.jobs.run(move |b| b.dev_exchange(&id, p.class, p.id, p.size, &p.args), |app, r: Result<Entry, String>| match r {
        Ok(e) => {
            app.dev.from_console.insert(e.seq);
            let reply = if e.reply.is_empty() { "—".to_string() } else { hex(&e.reply) };
            let text = format!("{} · {reply} · {} ms", e.error.as_deref().or(e.status.as_deref()).unwrap_or("—"), e.ms);
            app.dev.console.print(if is_error(&e) { Kind::Error } else { Kind::Out }, text, Some(e.seq));
        }
        Err(e) => app.dev.console.print(Kind::Error, e, None),
    });
}

fn submit(app: &mut App) {
    let text = app.dev.console.line.trim().to_string();
    if text.is_empty() {
        return;
    }
    app.dev.console.print(Kind::In, text.clone(), None);
    let p = match parse(&text, &app.dev.registry) {
        Ok(p) => p,
        Err(e) => {
            app.dev.console.print(Kind::Error, e, None);
            return;
        }
    };
    let Some(ask) = confirmation(&p, &app.dev.registry) else {
        send(app, p, text);
        return;
    };
    let warning = match ask {
        Confirm::Slot(slot) => app.tf("dev.drawer.confirmSlot", &[("slot", &slot.to_string())]),
        Confirm::Unusual { value, help } => app.tf("dev.drawer.confirmUnusual", &[("value", &hex(&[value])), ("help", &help)]),
        Confirm::Flash => app.t("dev.drawer.confirmFlash"),
        Confirm::Unknown => app.t("dev.drawer.confirmUnknown"),
    };
    app.dev.console.print(Kind::Warn, format!("⚠ {warning}"), None);
    app.dev.console.pending = Some((p, text));
}

/// The console drawer; returns the exchange a reply line asked to open.
pub fn ui(app: &mut App, ui: &mut Ui) -> Option<u64> {
    let mut open = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.t("dev.drawer.title")).small().color(theme::DEV));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✕").on_hover_text(app.t("dev.drawer.close")).clicked() {
                app.dev.drawer = false;
            }
            if ui.small_button("⊘").on_hover_text(app.t("dev.drawer.clear")).clicked() {
                app.dev.console.out.clear();
            }
        });
    });
    // The line sits at the bottom and the output takes what is left above it.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        input(app, ui);
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| open = output(app, ui));
    });
    open
}

/// The lines so far; returns the exchange a reply line asked to open.
fn output(app: &App, ui: &mut Ui) -> Option<u64> {
    let mut open = None;
    egui::ScrollArea::vertical().id_salt("console-out").max_height(ui.available_height()).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        if app.dev.console.out.is_empty() {
            ui.label(RichText::new(app.t("dev.drawer.empty")).small().color(theme::MUTED));
        }
        for (kind, text, seq) in &app.dev.console.out {
            let (mark, color) = match kind {
                Kind::In => ("›", theme::DEV),
                Kind::Out => ("‹", theme::TEXT),
                Kind::Error => ("‹", theme::ERROR),
                Kind::Warn => ("‹", theme::WARN),
                Kind::Note => ("‹", theme::MUTED),
            };
            let r = ui.add(egui::Label::new(RichText::new(format!("{mark} {text}")).monospace().small().color(color)).sense(egui::Sense::click()));
            if r.clicked() && seq.is_some() {
                open = *seq;
            }
        }
    });
    open
}

#[derive(Default)]
struct Keys {
    up: bool,
    down: bool,
    tab: bool,
    enter: bool,
    esc: bool,
}

/// Taken before the line sees them: Enter keeps the focus, and Esc is the console's only while
/// a list or a question is open, otherwise it reaches the panel.
fn take_keys(ui: &Ui, focused: bool, esc_ours: bool) -> Keys {
    if !focused {
        return Keys::default();
    }
    ui.input_mut(|i| {
        // A held Enter repeats, and one key press must never both ask and answer a flash write.
        let fresh_enter = i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat: false, modifiers, .. } if modifiers.is_none()));
        let mut take = |k| i.consume_key(egui::Modifiers::NONE, k);
        let enter = take(egui::Key::Enter) && fresh_enter;
        Keys { up: take(egui::Key::ArrowUp), down: take(egui::Key::ArrowDown), tab: take(egui::Key::Tab), enter, esc: esc_ours && take(egui::Key::Escape) }
    })
}

fn cursor_to_end(ctx: &egui::Context, id: egui::Id, line: &str) {
    if let Some(mut s) = egui::text_edit::TextEditState::load(ctx, id) {
        let end = egui::text::CCursor::new(line.chars().count());
        s.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        s.store(ctx, id);
    }
}

fn cancel(app: &mut App) {
    app.dev.console.pending = None;
    let note = app.t("dev.drawer.cancelled");
    app.dev.console.print(Kind::Note, note, None);
}

fn input(app: &mut App, ui: &mut Ui) {
    let id = ui.make_persistent_id("console-line");
    let pending = app.dev.console.pending.is_some();
    let had_focus = ui.memory(|m| m.has_focus(id));
    let keys = take_keys(ui, had_focus, pending || app.dev.console.list_open);
    let before = app.dev.console.line.clone();
    let hint = app.t("dev.drawer.placeholder");
    let edit = ui
        .horizontal(|ui| {
            ui.label(RichText::new("›").monospace().color(theme::DEV));
            let line = egui::TextEdit::singleline(&mut app.dev.console.line).id(id).lock_focus(true);
            ui.add(line.font(egui::TextStyle::Monospace).hint_text(hint).desired_width(f32::INFINITY))
        })
        .inner;
    let c = &mut app.dev.console;
    if c.focus {
        edit.request_focus();
        c.focus = false;
    }
    let blurred = had_focus && !edit.has_focus() && !c.popup_hovered;
    if pending {
        // No list is drawn while a question waits, so nothing can be hovered.
        c.popup_hovered = false;
        // The question holds the line as it was sent.
        c.line = before;
        if keys.enter {
            if let Some((p, text)) = c.pending.take() {
                send(app, p, text);
            }
        } else if keys.esc || blurred {
            cancel(app);
        }
        return;
    }
    if edit.gained_focus() || c.line != before {
        c.list_open = true;
    }
    if blurred || keys.esc {
        c.list_open = false;
    }
    suggestions(app, ui, &keys, id, edit.rect);
}

/// The list over the line: arrows pick, Tab or Enter take the picked item into the line.
fn suggestions(app: &mut App, ui: &mut Ui, keys: &Keys, id: egui::Id, at: egui::Rect) {
    let c = &mut app.dev.console;
    let list = suggest(&c.line, &app.dev.registry, &c.recent);
    if c.listed != c.line {
        c.listed = c.line.clone();
        c.active = 0;
        c.navigated = false;
    }
    if !list.is_empty() && (keys.up || keys.down) {
        c.list_open = true;
        c.navigated = true;
        c.active = if keys.down { c.active + 1 } else { c.active.saturating_sub(1) };
    }
    c.active = c.active.min(list.len().saturating_sub(1));
    let next = list.get(c.active).map(|item| completion(&c.line, item));
    if let Some(n) = next.clone().filter(|_| keys.tab) {
        c.line = n;
        cursor_to_end(ui.ctx(), id, &c.line);
    }
    if keys.enter {
        c.popup_hovered = false;
        match next.filter(|n| c.list_open && accepts(&c.line, n, c.navigated, &app.dev.registry)) {
            Some(n) => {
                c.line = n;
                c.list_open = false;
                cursor_to_end(ui.ctx(), id, &c.line);
            }
            None => {
                c.list_open = false;
                submit(app);
            }
        }
        return;
    }
    let c = &mut app.dev.console;
    c.popup_hovered = false;
    if !c.list_open || list.is_empty() {
        return;
    }
    let picked = list_popup(app, ui, &list, at, keys.up || keys.down);
    if let Some(i) = picked {
        let c = &mut app.dev.console;
        c.line = completion(&c.line, &list[i]);
        c.focus = true;
        cursor_to_end(ui.ctx(), id, &c.line);
    }
}

fn list_popup(app: &mut App, ui: &mut Ui, list: &[(String, Option<CommandInfo>)], at: egui::Rect, moved: bool) -> Option<usize> {
    let recent_label = app.t("dev.recent");
    let active = app.dev.console.active;
    let mut picked = None;
    let shown = egui::Popup::new(ui.make_persistent_id("console-list"), ui.ctx().clone(), at, ui.layer_id())
        .align(egui::RectAlign::TOP_START)
        .open(true)
        .width(at.width())
        .show(|ui| {
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for (i, (text, info)) in list.iter().enumerate() {
                    let right = match info {
                        Some(c) => format!("{} {}  {}", c.cmd, if c.cmd.is_get() { "R" } else { "W" }, c.help),
                        None => recent_label.clone(),
                    };
                    let r = ui.selectable_label(i == active, RichText::new(format!("{text}   {right}")).monospace().small());
                    if i == active && moved {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        picked = Some(i);
                    }
                }
            });
        });
    app.dev.console.popup_hovered = shown.is_some_and(|s| s.response.contains_pointer());
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        serde_json::from_str(
            r#"{
                "set_effect": {"cmd": "0F:02", "help": "Lighting effect", "flash": 1},
                "get_mode": {"cmd": "00:84", "size": 2, "help": "Mode"},
                "set_mode": {"cmd": "00:04", "help": "Mode", "safe": [0, [0, 3]]},
                "flash_it": {"cmd": "05:08", "help": "Slot", "flash": true}
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn a_line_parses_by_name_or_code() {
        let r = registry();
        assert_eq!(parse("get_mode", &r), Ok(Parsed { class: 0, id: 0x84, size: 2, args: vec![] }));
        assert_eq!(parse("0F:02 01 ff size=0a", &r), Ok(Parsed { class: 0x0f, id: 2, size: 10, args: vec![1, 0xff] }));
        assert!(parse("nope", &r).is_err());
        assert!(parse("00:84 zz", &r).is_err());
    }

    #[test]
    fn writes_ask_by_what_they_touch() {
        let r = registry();
        let q = |l: &str| confirmation(&parse(l, &r).unwrap(), &r);
        assert_eq!(q("get_mode"), None);
        assert_eq!(q("set_effect 00 00"), None, "slot 0 is the temporary store");
        assert_eq!(q("set_effect 00 02"), Some(Confirm::Slot(2)));
        assert_eq!(q("flash_it"), Some(Confirm::Flash));
        assert_eq!(q("set_mode 03"), None);
        assert!(matches!(q("set_mode 07"), Some(Confirm::Unusual { value: 7, .. })));
        assert_eq!(q("0A:01"), Some(Confirm::Unknown));
    }

    #[test]
    fn a_completion_keeps_the_typed_bytes() {
        let r = registry();
        let cmd = |n: &str| (n.to_string(), r.get(n).cloned());
        assert_eq!(completion("eff", &cmd("set_effect")), "set_effect ");
        assert_eq!(completion("eff 01 02", &cmd("set_effect")), "set_effect 01 02");
        assert_eq!(completion("x", &("get_mode 00".to_string(), None)), "get_mode 00", "a recent line replaces the whole line");
    }

    #[test]
    fn enter_sends_a_known_command_unless_the_arrows_picked_another() {
        let r = registry();
        assert!(!accepts("get_mode", "get_mode", false, &r), "nothing to take");
        assert!(!accepts("get_mode 01", "set_mode 01", false, &r));
        assert!(!accepts("00:84", "get_mode ", false, &r));
        assert!(accepts("get_mode 01", "set_mode 01", true, &r));
        assert!(accepts("mod", "get_mode ", false, &r));
    }

    #[test]
    fn suggestions_start_with_recent_lines_until_typed() {
        let r = registry();
        let recent = vec!["get_mode".to_string()];
        assert_eq!(suggest("", &r, &recent)[0].0, "get_mode");
        assert!(suggest("", &r, &recent)[0].1.is_none());
        assert!(suggest("effect", &r, &recent).iter().all(|(_, i)| i.is_some()));
    }
}
