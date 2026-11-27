//! Who made the app, under which license, and where its source lives.

use easyrazer_backend::api::{self, Link};
use egui::RichText;

use crate::app::App;
use crate::text::VERSION;
use crate::widgets::{self, Answer};
use crate::theme;

#[derive(Default)]
pub struct About {
    error: String,
}

/// Shows the dialog; `false` once it is closed.
pub fn show(app: &App, ctx: &egui::Context, a: &mut About) -> bool {
    let t = |k: &str| app.t(k);
    let (description, unofficial, close) = (t("about.description"), t("about.unofficial"), t("about.close"));
    // The links sit inside the sentences, where the locale files put `{gpl}` and `{github}`.
    let license = t("about.license");
    let source = t("about.source");
    let r = widgets::modal(
        ctx,
        "about",
        &t("about.title"),
        |ui| {
            ui.label(RichText::new(format!("EasyRazer {VERSION}")).strong());
            widgets::para(ui, &description);
            ui.label(RichText::new(&unofficial).color(theme::MUTED));
            sentence(ui, &license, "{gpl}", "GNU GPL", Link::License, &mut a.error);
            sentence(ui, &source, "{github}", "GitHub", Link::Repo, &mut a.error);
            if !a.error.is_empty() {
                ui.label(RichText::new(&a.error).small().color(theme::ERROR));
            }
        },
        |ui| widgets::primary(ui, &close, true).clicked().then_some(()),
    );
    matches!(r, Answer::Open)
}

fn sentence(ui: &mut egui::Ui, text: &str, slot: &str, name: &str, link: Link, error: &mut String) {
    let (before, after) = text.split_once(slot).unwrap_or((text, ""));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new(before).color(theme::MUTED));
        if ui.link(RichText::new(name).color(theme::ACCENT)).clicked()
            && let Err(e) = api::open_link(link)
        {
            *error = e;
        }
        ui.label(RichText::new(after).color(theme::MUTED));
    });
}
