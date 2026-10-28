//! Snap Tap groups: keys of which only one types at a time.

use std::collections::BTreeMap;

use easyrazer_backend::settings::SnapTap;
use egui::Color32;
use razer_core::rapid::{SnapGroup, SnapRule};

use crate::app::App;
use crate::{theme, widgets};

pub const RULES: [SnapRule; 3] = [SnapRule::Last, SnapRule::Neutral, SnapRule::Deeper];

/// Group colors, cycled; none is the accent, edited, error or driver color of the map.
const COLORS: [Color32; 4] = [Color32::from_rgb(0x3c, 0xc8, 0xe0), Color32::from_rgb(0xb4, 0x8c, 0xff), Color32::from_rgb(0xff, 0x7a, 0xb6), Color32::from_rgb(0x5b, 0x8c, 0xff)];

pub fn color(i: usize) -> Color32 {
    COLORS[i % COLORS.len()]
}

pub fn colors(s: &SnapTap) -> BTreeMap<u8, Color32> {
    s.groups.iter().enumerate().flat_map(|(i, g)| g.keys.iter().map(move |k| (*k, color(i)))).collect()
}

/// The new group takes its keys from the old ones; a group left with one key goes.
pub fn add_group(s: &SnapTap, keys: &[u8]) -> SnapTap {
    let mut groups: Vec<SnapGroup> = s
        .groups
        .iter()
        .map(|g| SnapGroup { keys: g.keys.iter().copied().filter(|k| !keys.contains(k)).collect(), rule: g.rule })
        .filter(|g| g.keys.len() > 1)
        .collect();
    groups.push(SnapGroup { keys: keys.to_vec(), rule: SnapRule::Last });
    SnapTap { enabled: s.enabled, groups }
}

pub fn remove_group(s: &SnapTap, i: usize) -> SnapTap {
    SnapTap { enabled: s.enabled, groups: s.groups.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, g)| g.clone()).collect() }
}

pub fn set_rule(s: &SnapTap, i: usize, rule: SnapRule) -> SnapTap {
    let mut next = s.clone();
    if let Some(g) = next.groups.get_mut(i) {
        g.rule = rule;
    }
    next
}

/// The Snap Tap switch, the groups of the loaded profile and the picked group's rule.
pub fn card(app: &mut App, ui: &mut egui::Ui) {
    let snap = app.keys.snap().clone();
    let driver = app.driver();
    let firmware = app.fw_rapid() && !driver;
    let can = driver || firmware;
    let selection: Vec<u8> = app.keys.selection.iter().copied().collect();
    let combo = super::combo(app, super::holder(app, razer_core::binding::System::SnapTap));
    let labels: BTreeMap<u8, &str> = app.layout.iter().map(|k| (k.key.key, k.key.label)).collect();
    let current = app.keys_view.snap_picked.filter(|i| *i < snap.groups.len());
    let mut next = None;
    ui.horizontal(|ui| {
        let mut on = snap.enabled;
        if widgets::toggle(ui, &mut on, &app.t("snap.title"), can).changed() {
            next = Some(SnapTap { enabled: on, ..snap.clone() });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::hint(ui, combo.unwrap_or_else(|| app.t("snap.noCombo")));
        });
    });
    for (i, g) in snap.groups.iter().enumerate() {
        ui.horizontal(|ui| {
            let on = Some(i) == current;
            let r = egui::Frame::new()
                .fill(theme::KEY_OFF)
                .stroke(egui::Stroke::new(1.0, if on { theme::TEXT } else { theme::LINE }))
                .corner_radius(theme::RADIUS)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width() - 44.0);
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(dot, 2.0, color(i));
                        for k in &g.keys {
                            let (text, known) = labels.get(k).map_or(("?", false), |l| (*l, true));
                            let chip = egui::Frame::new().fill(theme::KEY).stroke(egui::Stroke::new(1.0, theme::LINE)).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 0));
                            let r = chip.show(ui, |ui| ui.label(egui::RichText::new(text).small().color(if known { theme::TEXT } else { theme::MUTED })));
                            if !known {
                                r.response.on_hover_text(app.t("snap.absent"));
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::hint(ui, app.t(&format!("snap.rules.{}", crate::text::tag(&g.rule))));
                        });
                    });
                })
                .response
                .interact(egui::Sense::click());
            if r.clicked() {
                app.keys_view.snap_picked = Some(i);
            }
            if ui.add(egui::Button::new("×").min_size(egui::vec2(34.0, 30.0))).on_hover_text(app.t("snap.remove")).clicked() {
                next = Some(remove_group(&snap, i));
            }
        });
    }
    if snap.groups.is_empty() {
        widgets::hint(ui, app.t("snap.empty"));
    }
    if widgets::button(ui, app.tf("snap.add", &[("n", &selection.len().to_string())]), can && selection.len() >= 2).clicked() {
        let s = add_group(&snap, &selection);
        app.keys_view.snap_picked = Some(s.groups.len() - 1);
        next = Some(s);
    }
    if let Some(i) = current {
        let rule = snap.groups[i].rule;
        ui.horizontal(|ui| {
            ui.add_sized([100.0, 20.0], egui::Label::new(egui::RichText::new(app.t("snap.rule")).color(theme::MUTED)));
            let items: Vec<(SnapRule, String)> = RULES.iter().map(|r| (*r, app.t(&format!("snap.rules.{}", crate::text::tag(r))))).collect();
            if let Some(r) = widgets::segments(ui, &items, rule, true) {
                next = Some(set_rule(&snap, i, r));
            }
        });
        widgets::hint(ui, app.t(&format!("snap.hints.{}", crate::text::tag(&rule))));
    }
    let foot = app.t(if firmware { "snap.firmwarePair" } else { "rapid.hwOnly" });
    ui.label(egui::RichText::new(foot).small().color(if driver { egui::Color32::TRANSPARENT } else { theme::MUTED }));
    if let Some(s) = next {
        app.keys.set_snap(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_group_takes_its_keys_and_drops_leftover_singles() {
        let s = SnapTap { enabled: true, groups: vec![SnapGroup { keys: vec![1, 2], rule: SnapRule::Deeper }, SnapGroup { keys: vec![3, 4, 5], rule: SnapRule::Last }] };
        let next = add_group(&s, &[2, 3]);
        assert_eq!(next.groups.len(), 2, "1 alone goes");
        assert_eq!(next.groups[0].keys, vec![4, 5]);
        assert_eq!(next.groups[1].keys, vec![2, 3]);
    }
}
