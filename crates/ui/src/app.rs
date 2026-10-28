//! The main window: a snapshot of the backend's state, kept fresh by its events.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use easyrazer_backend::boards::DevicesView;
use easyrazer_backend::device::Status;
use easyrazer_backend::settings::CloseAction;
use easyrazer_backend::{Backend, Event, i18n};
use egui::{CornerRadius, RichText, Stroke, ViewportCommand};

use crate::icons::{self, Icon};
use crate::status::{self, Kind};
use crate::theme;
use crate::tray::{self, Tray};
use crate::instance::ACTIVATE_REQUESTED;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keys,
    Lighting,
    Settings,
}

const TABS: [(Tab, &str); 3] = [(Tab::Keys, "tabs.keys"), (Tab::Lighting, "tabs.lighting"), (Tab::Settings, "tabs.settings")];

pub struct App {
    backend: Arc<Backend>,
    events: Receiver<Event>,
    tray: Tray,
    /// Read by the event pump: key depth is dropped while nobody sees it.
    visible: Arc<AtomicBool>,
    lang: String,
    tab: Tab,
    devices: DevicesView,
    selected: Option<String>,
    status: Option<Status>,
    progress: Option<(usize, usize)>,
    error: String,
    /// Latest depth per key of the selected keyboard; drawn once a frame.
    depth: BTreeMap<u8, u8>,
    quitting: bool,
}

impl App {
    pub fn new(backend: Arc<Backend>, events: Receiver<Event>, tray: Tray, ctx: &egui::Context, shown: bool) -> Self {
        icons::install(ctx);
        let visible = Arc::new(AtomicBool::new(shown));
        let events = pump(events, ctx.clone(), visible.clone());
        let devices = backend.devices();
        let selected = devices.last.clone().or_else(|| devices.devices.first().map(|d| d.id.clone()));
        let status = selected.is_some().then(|| backend.status(selected.clone()));
        // Everything the first connect reports now reaches the window.
        backend.window_ready();
        Self {
            lang: backend.lang(),
            backend,
            events,
            tray,
            visible,
            tab: Tab::Keys,
            devices,
            selected,
            status,
            progress: None,
            error: String::new(),
            depth: BTreeMap::new(),
            quitting: false,
        }
    }

    fn t(&self, key: &str) -> String {
        i18n::t(&self.lang, key)
    }

    fn ours(&self, id: Option<&str>) -> bool {
        id.is_some() && id == self.selected.as_deref()
    }

    fn on_event(&mut self, e: Event) {
        match e {
            Event::Status(s) => {
                if self.selected.is_none() {
                    self.selected = s.id.clone();
                }
                if s.id.is_none() || self.ours(s.id.as_deref()) {
                    if let Some(err) = &s.error {
                        self.error = err.clone();
                    }
                    self.status = Some(s);
                }
            }
            Event::Devices => {
                self.devices = self.backend.devices();
                if self.selected.is_none() {
                    self.selected = self.devices.last.clone().or_else(|| self.devices.devices.first().map(|d| d.id.clone()));
                }
            }
            Event::KeyDepth { id, key, depth, .. } if self.ours(Some(&id)) => {
                self.depth.insert(key, depth);
            }
            Event::ReadProgress { id, done, total, .. } if self.ours(Some(&id)) => {
                self.progress = (done < total).then_some((done, total));
            }
            Event::SettingsChanged => self.tray.sync_autostart(),
            Event::LanguageChanged => {
                self.lang = self.backend.lang();
                self.tray.retitle(&self.lang);
            }
            Event::Error(message) => self.error = message,
            _ => {}
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        self.visible.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.visible.store(false, Ordering::Relaxed);
        self.depth.clear();
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        if let Err(e) = self.backend.drop_previews() {
            self.error = e;
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn on_tray(&mut self, ctx: &egui::Context, a: tray::Action) {
        match a {
            tray::Action::Show => self.show(ctx),
            tray::Action::Autostart => {
                if let Err(e) = self.backend.set_autostart(self.tray.autostart_checked()) {
                    self.error = i18n::tf(&self.lang, "backend.autostart", &[("error", &e)]);
                }
            }
            tray::Action::Quit => self.quit(ctx),
        }
    }

    fn on_close(&mut self, ctx: &egui::Context) {
        if self.quitting || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        match self.backend.close_action() {
            CloseAction::Exit => self.quitting = true,
            // Asking needs the dialogs of the next phase; until then the window goes to the tray.
            CloseAction::Tray | CloseAction::Ask => {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.hide(ctx);
            }
        }
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (tab, key) in TABS {
                let on = self.tab == tab;
                let text = RichText::new(self.t(key)).color(if on { theme::TEXT } else { theme::MUTED });
                let button = egui::Button::new(text)
                    .fill(if on { theme::BG } else { theme::PANEL })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius { nw: theme::RADIUS, ne: theme::RADIUS, sw: 0, se: 0 });
                let r = ui.add(button);
                if on {
                    let rect = r.rect;
                    ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(2.0, theme::ACCENT));
                }
                if r.clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.mode(ui));
        });
    }

    /// The HW / Driver switch: who types, the keyboard or the app.
    fn mode(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.status else { return };
        let (id, driver, writable) = (s.id.clone(), s.driver_mode, s.device && !s.synapse);
        ui.add_enabled_ui(writable, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (on, icon, key) in [(true, Icon::rust, "mode.driver"), (false, Icon::chip, "mode.hw")] {
                let color = match (driver == on, on) {
                    (false, _) => theme::MUTED,
                    (true, true) => theme::DRIVER,
                    (true, false) => theme::ACCENT,
                };
                let r = ui.add(egui::Button::image(icons::image(icon, 28.0, color)).min_size(egui::vec2(52.0, 34.0)));
                let r = r.on_hover_text(self.t("mode.hint")).on_hover_text(self.t(key));
                if r.clicked() && driver != on
                    && let Some(id) = &id
                {
                    if let Err(e) = self.backend.set_driver_mode(id, on) {
                        self.error = e;
                    }
                    self.status = Some(self.backend.status(Some(id.clone())));
                }
            }
        });
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let name = self.devices.devices.iter().find(|d| self.ours(Some(&d.id))).map(|d| d.name.as_str());
        let input = status::Input {
            status: self.status.as_ref(),
            progress: self.progress,
            unsupported: self.devices.unsupported,
            name,
            message: "",
            error: &self.error,
        };
        let (kind, text) = status::line(&self.lang, &input);
        let (fill, color) = match kind {
            Kind::Info => (theme::PANEL, theme::TEXT),
            Kind::Warn => (theme::WARN_BG, theme::WARN),
            Kind::Ok => (theme::PANEL, theme::ACCENT),
        };
        egui::Frame::new().fill(fill).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 34.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(text).color(color));
            });
        });
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(a) = self.tray.actions.try_recv() {
            self.on_tray(ctx, a);
        }
        if ACTIVATE_REQUESTED.swap(false, Ordering::Relaxed) {
            self.show(ctx);
        }
        while let Ok(e) = self.events.try_recv() {
            self.on_event(e);
        }
        self.on_close(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(16)).show(ui, |ui| {
            ui.set_max_width(1121.0);
            self.tabs(ui);
            self.status_line(ui);
        });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }
}

/// Moves backend events to the window and wakes it; key depth only while it is seen, so a game
/// session in the tray costs nothing.
fn pump(events: Receiver<Event>, ctx: egui::Context, visible: Arc<AtomicBool>) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for e in events {
            if matches!(e, Event::KeyDepth { .. }) && !visible.load(Ordering::Relaxed) {
                continue;
            }
            if tx.send(e).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
    rx
}
