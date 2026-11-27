//! The traffic log: every exchange with the keyboard, filtered, and the one picked in detail.

use easyrazer_backend::settings::DevTraffic;
use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use razer_core::logged::Entry;

use super::{hex, is_error, is_write, name_of};
use crate::app::App;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    All,
    Reads,
    Writes,
    Errors,
}

const SHOWS: [(Show, &str); 4] = [(Show::All, "all"), (Show::Reads, "r"), (Show::Writes, "w"), (Show::Errors, "e")];

/// Narrower than this, the details take the whole tab; the type chips fold into a list sooner.
const SPLIT_MIN: f32 = 560.0;
const CHIPS_MIN: f32 = 440.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    Time,
    Name,
    Code,
    Args,
    Status,
    Reply,
    Ms,
}

const COLUMNS: [Col; 7] = [Col::Time, Col::Name, Col::Code, Col::Args, Col::Status, Col::Reply, Col::Ms];
// The last here is the first to go when the table is narrow.
const PRIORITY: [Col; 7] = [Col::Name, Col::Status, Col::Time, Col::Ms, Col::Code, Col::Args, Col::Reply];

impl Col {
    fn key(self) -> &'static str {
        match self {
            Col::Time => "time",
            Col::Name => "name",
            Col::Code => "code",
            Col::Args => "args",
            Col::Status => "status",
            Col::Reply => "reply",
            Col::Ms => "ms",
        }
    }

    fn width(self) -> f32 {
        match self {
            Col::Time => 90.0,
            Col::Name | Col::Args | Col::Reply => 150.0,
            Col::Code => 46.0,
            Col::Status => 64.0,
            Col::Ms => 52.0,
        }
    }
}

/// The enabled columns that fit, dropping the least needed first; the name always stays.
pub fn visible(enabled: &[Col], width: f32) -> Vec<Col> {
    let mut keep: Vec<Col> = PRIORITY.into_iter().filter(|c| *c == Col::Name || enabled.contains(c)).collect();
    while keep.len() > 1 && keep.iter().map(|c| c.width()).sum::<f32>() > width {
        keep.pop();
    }
    COLUMNS.into_iter().filter(|c| keep.contains(c)).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Request,
    Response,
    Timing,
}

pub struct Traffic {
    filter: String,
    show: Show,
    background: bool,
    /// The log as it was when paused.
    frozen: Option<Vec<Entry>>,
    /// Entries up to this one are cleared from view.
    cleared_at: Option<u64>,
    detail: DetailTab,
    columns: Vec<Col>,
}

impl Traffic {
    /// Names nobody knows any more are skipped; an empty column list keeps the default.
    pub fn restore(&mut self, v: &DevTraffic) {
        let columns: Vec<Col> = COLUMNS.into_iter().filter(|c| v.columns.iter().any(|n| n == c.key())).collect();
        if !columns.is_empty() {
            self.columns = columns;
        }
        if let Some((s, _)) = SHOWS.iter().find(|(_, k)| *k == v.show) {
            self.show = *s;
        }
        self.background = v.background;
    }

    pub fn view(&self) -> DevTraffic {
        let show = SHOWS.iter().find(|(s, _)| *s == self.show).map_or("all", |(_, k)| k);
        DevTraffic { columns: self.columns.iter().map(|c| c.key().to_string()).collect(), show: show.into(), background: self.background }
    }
}

impl Default for Traffic {
    fn default() -> Self {
        Self {
            filter: String::new(),
            show: Show::All,
            background: false,
            frozen: None,
            cleared_at: None,
            detail: DetailTab::Request,
            columns: vec![Col::Time, Col::Name, Col::Code, Col::Args, Col::Status, Col::Ms],
        }
    }
}

/// A failed write still counts as a write: the type chips and "errors" overlap.
pub fn matches(e: &Entry, show: Show, background: bool, query: &str, name: Option<&str>) -> bool {
    if e.background && !background {
        return false;
    }
    match show {
        Show::Errors if !is_error(e) => return false,
        Show::Reads if is_write(&e.cmd) => return false,
        Show::Writes if !is_write(&e.cmd) => return false,
        _ => {}
    }
    let q = query.trim().to_lowercase();
    q.is_empty() || format!("{} {} {} {}", e.cmd, name.unwrap_or(""), hex(&e.args), hex(&e.reply)).to_lowercase().contains(&q)
}

/// The time of day on the user's clock, to the millisecond.
pub fn clock(ms: u64) -> String {
    let (h, m, s) = easyrazer_backend::sys::local_time(ms).unwrap_or(((ms / 3_600_000 % 24) as u16, (ms / 60_000 % 60) as u16, (ms / 1000 % 60) as u16));
    format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000)
}

/// 16 bytes a row, the offset first.
pub fn dump(bytes: &[u8]) -> Vec<String> {
    bytes.chunks(16).enumerate().map(|(i, c)| format!("{:02X}  {}", i * 16, hex(c))).collect()
}

pub fn ui(app: &mut App, ui: &mut Ui) {
    let before = app.dev.traffic.view();
    shown(app, ui);
    let after = app.dev.traffic.view();
    if after != before
        && let Err(e) = app.jobs.backend().set_dev_traffic(after)
    {
        app.show_error(e);
    }
}

fn shown(app: &mut App, ui: &mut Ui) {
    let t = &mut app.dev.traffic;
    let source = t.frozen.clone().unwrap_or_else(|| app.dev.log.clone());
    let cleared = t.cleared_at;
    let lang = app.lang.clone();
    let tr = |k: &str| easyrazer_backend::i18n::t(&lang, k);
    let errors = app.dev.log.iter().filter(|e| cleared.is_none_or(|c| e.seq > c) && (t.background || !e.background) && is_error(e)).count();
    let chips = ui.available_width() >= CHIPS_MIN;
    ui.horizontal(|ui| {
        let t = &mut app.dev.traffic;
        if ui.small_button("⊘").on_hover_text(tr("dev.traffic.clear")).clicked() {
            t.cleared_at = app.dev.log.last().map(|e| e.seq);
        }
        let paused = t.frozen.is_some();
        if ui.small_button(if paused { "▶" } else { "⏸" }).on_hover_text(tr(if paused { "dev.traffic.resume" } else { "dev.traffic.pause" })).clicked() {
            t.frozen = if paused { None } else { Some(app.dev.log.clone()) };
        }
        ui.add(egui::TextEdit::singleline(&mut t.filter).hint_text(tr("dev.traffic.filter")).desired_width(120.0));
        let show_label = |key: &str| RichText::new(tr(&format!("dev.traffic.show.{key}"))).small();
        if chips {
            for (s, key) in SHOWS {
                if ui.selectable_label(t.show == s, show_label(key)).clicked() {
                    t.show = s;
                }
            }
        } else {
            let current = SHOWS.iter().find(|(s, _)| *s == t.show).map_or("all", |(_, k)| k);
            ui.menu_button(show_label(current), |ui| {
                for (s, key) in SHOWS {
                    if ui.selectable_label(t.show == s, show_label(key)).clicked() {
                        t.show = s;
                        ui.close();
                    }
                }
            });
        }
        let bg = t.background;
        let tip = tr(if bg { "dev.traffic.hideBackground" } else { "dev.traffic.showBackground" });
        if ui.selectable_label(bg, "◌").on_hover_text(tip).clicked() {
            t.background = !bg;
        }
        if errors > 0 && ui.small_button(RichText::new(format!("● {errors}")).color(theme::ERROR)).on_hover_text(tr("dev.traffic.errors")).clicked() {
            t.show = Show::Errors;
        }
    });
    let t = &app.dev.traffic;
    let rows: Vec<&Entry> = source
        .iter()
        .filter(|e| cleared.is_none_or(|c| e.seq > c))
        .filter(|e| matches(e, t.show, t.background, &t.filter, name_of(&app.dev.registry, &e.cmd)))
        .collect();
    let selected = app.dev.selected.and_then(|s| source.iter().find(|e| e.seq == s && cleared.is_none_or(|c| e.seq > c))).cloned();
    let split = selected.is_some() && ui.available_width() >= SPLIT_MIN;
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        if selected.is_none() || split {
            let width = if selected.is_some() { ui.available_width() * 0.45 } else { ui.available_width() };
            // With the details open the table only points at the exchange.
            let cols = if selected.is_some() { vec![Col::Name] } else { visible(&app.dev.traffic.columns, width - 12.0) };
            // The table lays its rows top-down; it would inherit this row's left-to-right otherwise.
            ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| table(app, ui, &rows, &cols));
        }
        if let Some(e) = &selected {
            if split {
                ui.separator();
            }
            ui.vertical(|ui| detail(app, ui, e));
        }
    });
}

fn or_dash(s: String) -> String {
    if s.is_empty() { "—".into() } else { s }
}

/// The text of one cell and its color.
fn cell(e: &Entry, c: Col, name: Option<&str>, color: egui::Color32) -> (String, egui::Color32) {
    match c {
        Col::Time => (clock(e.time), theme::MUTED),
        Col::Name => (name.unwrap_or(&e.cmd).to_string(), color),
        Col::Code => (e.cmd.clone(), if is_write(&e.cmd) { theme::WRITE } else { theme::HS }),
        Col::Args => (or_dash(hex(&e.args)), color),
        Col::Status => (e.error.clone().or(e.status.clone()).unwrap_or_else(|| "—".into()), color),
        Col::Reply => (or_dash(hex(&e.reply)), color),
        Col::Ms => (if e.polls > 0 { format!("{} ·{}", e.ms, e.polls) } else { e.ms.to_string() }, theme::MUTED),
    }
}

fn table(app: &mut App, ui: &mut Ui, rows: &[&Entry], cols: &[Col]) {
    let lang = app.lang.clone();
    let tr = |k: &str| easyrazer_backend::i18n::t(&lang, k);
    let mono = |s: String, c: egui::Color32| RichText::new(s).monospace().small().color(c);
    let enabled = app.dev.traffic.columns.clone();
    let mut picked = None;
    let mut toggled = None;
    let mut builder = TableBuilder::new(ui).striped(true).stick_to_bottom(true).sense(egui::Sense::click());
    for (i, c) in cols.iter().enumerate() {
        builder = builder.column(if i + 1 == cols.len() { Column::remainder() } else { Column::exact(c.width()) }.clip(true));
    }
    builder
        .header(18.0, |mut h| {
            for c in cols {
                let (_, r) = h.col(|ui| {
                    ui.label(RichText::new(tr(&format!("dev.columns.{}", c.key()))).small().color(theme::MUTED));
                });
                r.context_menu(|ui| {
                    for c in COLUMNS {
                        let mut on = c == Col::Name || enabled.contains(&c);
                        if ui.add_enabled(c != Col::Name, egui::Checkbox::new(&mut on, tr(&format!("dev.columns.{}", c.key())))).changed() {
                            toggled = Some(c);
                        }
                    }
                });
            }
        })
        .body(|body| {
            body.rows(18.0, rows.len(), |mut row| {
                let e = rows[row.index()];
                row.set_selected(app.dev.selected == Some(e.seq));
                let color = if is_error(e) { theme::ERROR } else if e.background { theme::MUTED } else { theme::TEXT };
                let name = name_of(&app.dev.registry, &e.cmd);
                for &c in cols {
                    let (mut text, color) = cell(e, c, name, color);
                    if c == Col::Name && app.dev.from_console.contains(&e.seq) {
                        text.insert_str(0, "› ");
                    }
                    row.col(|ui| {
                        ui.label(mono(text, color));
                    });
                }
                if row.response().clicked() {
                    picked = Some(e.seq);
                }
            });
        });
    if let Some(s) = picked {
        app.dev.selected = if app.dev.selected == Some(s) { None } else { Some(s) };
    }
    if let Some(c) = toggled {
        let t = &mut app.dev.traffic;
        let on = !t.columns.contains(&c);
        t.columns = COLUMNS.into_iter().filter(|x| if *x == c { on } else { t.columns.contains(x) }).collect();
    }
}

fn detail(app: &mut App, ui: &mut Ui, e: &Entry) {
    let lang = app.lang.clone();
    let tr = |k: &str| easyrazer_backend::i18n::t(&lang, k);
    let mut text = Vec::new();
    ui.horizontal(|ui| {
        if ui.small_button("✕").on_hover_text(tr("dev.detail.close")).clicked() {
            app.dev.selected = None;
        }
        for (tab, key) in [(DetailTab::Request, "request"), (DetailTab::Response, "response"), (DetailTab::Timing, "timing")] {
            if ui.selectable_label(app.dev.traffic.detail == tab, RichText::new(tr(&format!("dev.detail.tabs.{key}"))).small()).clicked() {
                app.dev.traffic.detail = tab;
            }
        }
    });
    let info = app.dev.registry.values().find(|c| c.cmd.to_string() == e.cmd).cloned();
    match app.dev.traffic.detail {
        DetailTab::Request => {
            let kind = tr(if is_write(&e.cmd) { "dev.detail.write" } else { "dev.detail.read" });
            let console = if app.dev.from_console.contains(&e.seq) { format!(" · {}", tr("dev.detail.fromConsole")) } else { String::new() };
            text.push(format!("{} · {} · {kind}{console}", name_of(&app.dev.registry, &e.cmd).unwrap_or(&e.cmd), e.cmd));
            text.push(String::new());
            text.push(tr("dev.detail.args").to_uppercase());
            // The last names cover variable data, so bytes past the names go under the last one.
            let names: Vec<String> = info.as_ref().map(|i| i.args.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default();
            if e.args.is_empty() {
                text.push("—".into());
            }
            for (i, b) in e.args.iter().enumerate() {
                let n = names.get(i.min(names.len().saturating_sub(1))).cloned().unwrap_or_else(|| format!("args[{i}]"));
                text.push(format!("{n:<16} {b:02X}"));
            }
            text.push(String::new());
            text.push(tr("dev.detail.bytes").to_uppercase());
            text.extend(dump(&e.tx));
        }
        DetailTab::Response => {
            text.push(format!("{} {}", tr("dev.detail.status"), e.error.clone().or(e.status.clone()).unwrap_or_else(|| "—".into())));
            text.push(String::new());
            text.push(tr("dev.detail.data").to_uppercase());
            text.push(if e.reply.is_empty() { "—".into() } else { hex(&e.reply) });
            text.push(String::new());
            text.push(tr("dev.detail.bytes").to_uppercase());
            text.extend(dump(&e.rx));
        }
        DetailTab::Timing => {
            text.push(format!("{:<16} {}", tr("dev.detail.sent"), clock(e.time)));
            text.push(format!("{:<16} {}", tr("dev.detail.duration"), e.ms));
            text.push(format!("{:<16} {}", tr("dev.detail.polls"), e.polls));
            text.push(format!("{:<16} {}", tr("dev.detail.background"), tr(if e.background { "dev.detail.yes" } else { "dev.detail.no" })));
        }
    }
    let all = text.join("\n");
    if ui.small_button("⎘").on_hover_text(tr("dev.detail.copy")).clicked() {
        ui.ctx().copy_text(all.clone());
    }
    egui::ScrollArea::vertical().id_salt("detail").show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(all).monospace().small()).selectable(true));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cmd: &str, status: Option<&str>, background: bool) -> Entry {
        Entry { cmd: cmd.into(), status: status.map(Into::into), background, args: vec![0xAB], ..Default::default() }
    }

    #[test]
    fn filters_split_by_kind_and_errors_overlap() {
        let failed_write = entry("0F:02", Some("Failure"), false);
        assert!(matches(&failed_write, Show::Writes, false, "", None));
        assert!(matches(&failed_write, Show::Errors, false, "", None));
        assert!(!matches(&failed_write, Show::Reads, false, "", None));
        assert!(!matches(&entry("00:84", Some("Ok"), true), Show::All, false, "", None), "polling is hidden unless asked for");
        assert!(matches(&entry("00:84", Some("Ok"), false), Show::All, false, "ab", None), "the query matches the bytes");
    }

    #[test]
    fn narrow_tables_drop_the_least_needed_columns_but_keep_the_name() {
        let all = COLUMNS.to_vec();
        assert_eq!(visible(&all, 2000.0), all);
        assert_eq!(visible(&all, 350.0), vec![Col::Time, Col::Name, Col::Status]);
        assert_eq!(visible(&[], 10.0), vec![Col::Name]);
        assert_eq!(visible(&[Col::Reply], 2000.0), vec![Col::Name, Col::Reply]);
    }

    #[test]
    fn the_view_survives_a_restart_and_unknown_names_are_skipped() {
        let mut t = Traffic { columns: vec![Col::Name, Col::Reply], show: Show::Errors, background: true, ..Default::default() };
        let mut back = Traffic::default();
        back.restore(&t.view());
        assert_eq!((back.columns, back.show, back.background), (vec![Col::Name, Col::Reply], Show::Errors, true));
        t = Traffic::default();
        t.restore(&DevTraffic { columns: vec!["nope".into()], show: "nope".into(), background: false });
        assert_eq!((t.columns, t.show), (Traffic::default().columns, Show::All));
    }

    #[test]
    fn a_dump_breaks_every_16_bytes() {
        let rows = dump(&[0u8; 20]);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].starts_with("10  00 00 00 00"));
    }
}
