//! The update check: at start and once a day while it is on, or on request from Settings.

use std::time::{Duration, Instant};

use easyrazer_backend::api::{self, Link};
use easyrazer_backend::updates;
use egui::{RichText, Stroke, Ui};

use crate::app::App;
use crate::text::VERSION;
use crate::{theme, widgets};

const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Offline at start is common; the next try should not wait a whole day.
const RETRY: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Newer(String),
    /// A local or nightly build is not compared; the latest release is only named.
    DevBuild(String),
    Failed(String),
}

#[derive(Default)]
pub struct Updates {
    checking: bool,
    next: Option<Instant>,
    outcome: Option<Outcome>,
    /// The version whose notice was hidden; a later one shows again.
    dismissed: Option<String>,
}

pub fn outcome(latest: Result<String, String>, current: &str) -> Outcome {
    match latest {
        Err(e) => Outcome::Failed(e),
        Ok(tag) if !updates::is_release(current) => Outcome::DevBuild(tag),
        Ok(tag) if updates::newer(&tag, current) => Outcome::Newer(tag),
        Ok(_) => Outcome::UpToDate,
    }
}

fn auto(app: &App) -> bool {
    app.settings_view.as_ref().map_or_else(|| app.jobs.backend().settings().check_updates, |s| s.check_updates)
}

/// Runs from the window's logic, in the tray too.
pub fn tick(app: &mut App) {
    if !auto(app) || !updates::is_release(VERSION) || app.updates.checking {
        return;
    }
    match app.updates.next.map(|t| t.saturating_duration_since(Instant::now())) {
        Some(left) if !left.is_zero() => app.jobs.wake_after(left),
        _ => check(app),
    }
}

pub fn check(app: &mut App) {
    if app.updates.checking {
        return;
    }
    app.updates.checking = true;
    app.jobs.run(
        |_| api::latest_release(),
        |app, r| {
            let u = &mut app.updates;
            u.checking = false;
            u.next = Some(Instant::now() + if r.is_ok() { EVERY } else { RETRY });
            u.outcome = Some(outcome(r, VERSION));
        },
    );
}

pub fn checking(app: &App) -> bool {
    app.updates.checking
}

/// What the last check found, for the Settings row.
pub fn status(app: &App) -> Option<String> {
    Some(match app.updates.outcome.as_ref()? {
        Outcome::UpToDate => app.t("updates.upToDate"),
        Outcome::Newer(v) => app.tf("updates.available", &[("version", v)]),
        Outcome::DevBuild(v) => app.tf("updates.devBuild", &[("version", v)]),
        Outcome::Failed(e) => app.tf("updates.failed", &[("error", e)]),
    })
}

/// The notice over every tab while a newer release is out and not hidden.
pub fn banner(app: &mut App, ui: &mut Ui) {
    let Some(Outcome::Newer(version)) = app.updates.outcome.clone() else { return };
    if app.updates.dismissed.as_ref() == Some(&version) {
        return;
    }
    let text = app.tf("updates.available", &[("version", &version)]);
    let (download, dismiss) = (app.t("updates.download"), app.t("updates.dismiss"));
    egui::Frame::new().fill(theme::PANEL).stroke(Stroke::new(1.0, theme::ACCENT)).corner_radius(theme::RADIUS).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(text).color(theme::ACCENT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, &dismiss, true).clicked() {
                    app.updates.dismissed = Some(version.clone());
                }
                if widgets::primary(ui, &download, true).clicked()
                    && let Err(e) = api::open_link(Link::LatestRelease)
                {
                    app.show_error(e);
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_tag_is_news_and_the_same_one_is_not() {
        assert_eq!(outcome(Ok("0.11.0".into()), "0.10.0"), Outcome::Newer("0.11.0".into()));
        assert_eq!(outcome(Ok("0.10.0".into()), "0.10.0"), Outcome::UpToDate);
        assert_eq!(outcome(Ok("0.9.1".into()), "0.10.0"), Outcome::UpToDate);
    }

    #[test]
    fn a_development_build_only_names_the_release() {
        assert_eq!(outcome(Ok("0.10.0".into()), "0.0.0"), Outcome::DevBuild("0.10.0".into()));
        assert_eq!(outcome(Err("offline".into()), "0.10.0"), Outcome::Failed("offline".into()));
    }
}
