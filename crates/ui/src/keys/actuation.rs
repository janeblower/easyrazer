//! The press point and Rapid Trigger of the selected keys, with the switch drawn between them.

use easyrazer_backend::settings::Rapid;
use egui::{Color32, RichText, Shape, Stroke, Ui, vec2};
use razer_core::analog::Analog;

use super::model::{KeyEdits, RapidMode, common};
use super::slider::{self, Slider};
use crate::app::App;
use crate::icons::{self, Icon};
use crate::{theme, widgets};

const RT_MIN: f32 = 0.1;

const RT_MAX: f32 = 1.0;
/// The Huntsman V2 Analog's scale, for a keyboard that reports none.
const V2: ([f32; 2], Option<[f32; 2]>, f32, f32) = ([1.5, 3.6], Some([1.62, 3.56]), 1.5, 3.6);

struct Scale {
    range: [f32; 2],
    hw_range: Option<[f32; 2]>,
    zero_mm: f32,
    full_mm: f32,
}

impl Scale {
    fn of(a: Option<&Analog>) -> Self {
        match a {
            Some(a) => Self { range: a.range, hw_range: a.hw_range, zero_mm: a.zero_mm, full_mm: a.full_mm },
            None => Self { range: V2.0, hw_range: V2.1, zero_mm: V2.2, full_mm: V2.3 },
        }
    }

    fn depth_mm(&self, d: u8) -> f32 {
        self.zero_mm + f32::from(d) / 255.0 * (self.full_mm - self.zero_mm)
    }

    fn units(&self, mm: f32) -> f32 {
        mm / (self.full_mm - self.zero_mm) * 255.0
    }
}

/// Mode, press and release shared by the selected keys; `None` where they differ.
struct RapidView {
    mode: Option<RapidMode>,
    press: Option<f32>,
    release: Option<f32>,
}

fn rapid_view(k: &KeyEdits, keys: impl Iterator<Item = u8> + Clone, fw_hw: bool) -> RapidView {
    let r: Vec<Rapid> = keys.map(|key| k.rapid_of(key)).collect();
    RapidView {
        mode: common(r.iter().map(|x| KeyEdits::mode_of(x, fw_hw))),
        press: common(r.iter().map(|x| x.press)),
        release: common(r.iter().map(|x| x.release)),
    }
}

/// One Rapid Trigger scale: press or release, the value, where the thumb sits, the mark and the fill.
type RtSlider = (bool, Option<f32>, f32, f32, Option<(f32, f32)>);

/// With nothing selected the scales follow the key held, mid-track without one.
fn mid(count: usize, min: f32, max: f32, v: Option<f32>, fallback: f32, idle: Option<f32>) -> f32 {
    if count > 0 { v.unwrap_or(fallback) } else { idle.unwrap_or((min + max) / 2.0) }
}

fn caption(ui: &mut Ui, text: &str, visible: bool) {
    let color = if visible { theme::MUTED } else { Color32::TRANSPARENT };
    ui.label(RichText::new(text).small().color(color));
}

fn readout(ui: &mut Ui, text: &str) {
    ui.add_sized([60.0, 24.0], egui::Label::new(RichText::new(text).size(16.0).strong()));
}

pub fn card(app: &mut App, ui: &mut Ui) {
    let k = &app.keys;
    let count = k.selection.len();
    let sel = k.selection.clone();
    let value = common(sel.iter().map(|&s| k.value_of(s))).flatten();
    let hs = common(sel.iter().map(|&s| k.hs_of(s))).flatten();
    let (driver, fw_hw) = (app.driver(), app.fw_hw());
    let rapid = rapid_view(k, sel.iter().copied(), fw_hw);
    let split = k.split();
    let hs_split = k.hs_split();
    let crt = fw_hw.then(|| k.crt());
    let analog = app.status.as_ref().and_then(|s| s.analog);
    let scale = Scale::of(analog);
    let rt_here = driver || analog.is_some_and(|a| a.firmware_rapid);
    let rt_on = rt_here && rapid.mode.is_some_and(|m| m != RapidMode::Off);
    let (depth, travel, down, lead_key) = app.lead;
    let live = driver && depth > 0;
    let held = (count == 0 && live && lead_key > 0).then(|| (k.value_of(lead_key), rapid_view(k, std::iter::once(lead_key), fw_hw)));
    let [min, max] = scale.range;
    let mixed = app.t("actuation.mixed");
    let label = |v: Option<f32>| v.map_or(mixed.clone(), |v| format!("{v:.1}"));
    let hw_marks: Vec<f32> = if driver { Vec::new() } else { scale.hw_range.map(|r| r.to_vec()).unwrap_or_default() };
    let hw_tip = (!hw_marks.is_empty()).then(|| app.tf("actuation.hwLimits", &[("min", &hw_marks[0].to_string()), ("max", &hw_marks[hw_marks.len() - 1].to_string())]));
    let frac = |v: f32| ((v - min) / (max - min)).clamp(0.0, 1.0);
    let fill = live.then(|| (0.0, frac(scale.depth_mm(depth))));
    let mut events = Vec::new();

    ui.horizontal_top(|ui| {
        ui.add_space(((ui.available_width() - 500.0) / 2.0).max(0.0));
        // Press point.
        column(ui, 160.0, |ui| {
            ui.label(format!("{}, {}", app.t("actuation.title"), app.t("actuation.unit")));
            let items = [(false, app.t("actuation.shared")), (true, "Hypershift".to_string())];
            if let Some(on) = widgets::segments(ui, &items, hs_split, count > 0).filter(|_| count > 0) {
                events.push(Event::HsSplit(on));
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let columns: &[bool] = if hs_split { &[false, true] } else { &[false] };
                if !hs_split {
                    ui.add_space(44.0);
                }
                for &is_hs in columns {
                    column(ui, 72.0, |ui| {
                        ui.label(RichText::new(format!("{min:.1}")).small().color(theme::MUTED));
                        let v = if is_hs { hs } else { value };
                        let mut marks: Vec<(f32, Color32, f32)> = hw_marks
                            .iter()
                            .enumerate()
                            .map(|(i, m)| {
                                // A mark lights up when the chosen point is past it and the firmware will clamp it.
                                let clamped = value.is_some_and(|v| if i == 0 { v < *m } else { v > *m });
                                (frac(*m), if clamped { theme::WARN } else { theme::MUTED }, 6.0)
                            })
                            .collect();
                        if let Some((Some(hv), _)) = &held
                            && !is_hs
                        {
                            marks.push((frac(*hv), theme::TEXT, 4.0));
                        }
                        let s = Slider {
                            min,
                            max,
                            value: mid(count, min, max, v, min, if is_hs { None } else { held.as_ref().and_then(|h| h.0) }),
                            enabled: count > 0,
                            idle: count == 0,
                            fill,
                            fill_color: if is_hs { theme::HS } else { theme::ACCENT },
                            marks,
                            arrow: (live && !is_hs).then_some(down),
                            tip: hw_tip.clone(),
                        };
                        if let Some(mm) = slider::show(ui, &s) {
                            events.push(if is_hs { Event::Hs(mm) } else { Event::Set(mm) });
                        }
                        ui.label(RichText::new(format!("{max:.1}")).small().color(theme::MUTED));
                        let live_text = if live { format!("{:.1}", scale.depth_mm(depth)) } else { String::new() };
                        readout(ui, &if count > 0 { label(v) } else { live_text });
                        if is_hs {
                            ui.label(RichText::new("Hypershift").small().color(theme::HS));
                        } else {
                            caption(ui, &app.t("actuation.normal"), hs_split);
                        }
                    });
                }
            });
        });
        column(ui, 120.0, |ui| {
            ui.add_space(60.0);
            switch_art(ui);
        });
        // Rapid Trigger.
        column(ui, 200.0, |ui| {
            ui.label(format!("{}, {}", app.t("rapid.title"), app.t("actuation.unit")));
            ui.horizontal(|ui| {
                let modes: &[RapidMode] = if crt.is_none() { &[RapidMode::Off, RapidMode::Rt, RapidMode::Crt] } else { &[RapidMode::Off, RapidMode::Rt] };
                let items: Vec<(RapidMode, String)> = modes.iter().map(|m| (*m, app.t(&format!("rapid.{}", mode_name(*m))))).collect();
                let current = rapid.mode.unwrap_or(RapidMode::Off);
                let shown = if count > 0 { current } else { RapidMode::Off };
                if let Some(m) = widgets::segments(ui, &items, shown, count > 0 && rt_here) {
                    events.push(Event::Mode(m));
                }
                if let Some(on) = crt {
                    let b = egui::Button::new(RichText::new("CRT").small()).stroke(Stroke::new(1.0, if on { theme::ACCENT } else { theme::LINE }));
                    if ui.add(b).on_hover_text(app.t("rapid.crtProfileHint")).clicked() {
                        events.push(Event::Crt(!on));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if !split {
                    ui.add_space(64.0);
                }
                let shown_press = mid(count, RT_MIN, RT_MAX, rapid.press, 0.4, held.as_ref().and_then(|h| h.1.press));
                let shown_release = mid(count, RT_MIN, RT_MAX, rapid.release, 0.4, held.as_ref().and_then(|h| h.1.release));
                let held_on = held.as_ref().is_some_and(|h| h.1.mode.is_some_and(|m| m != RapidMode::Off));
                // Unsplit, the one slider stands for both: once the key is down it measures the release.
                let first = if split || !down { shown_press } else { shown_release };
                let rt_frac = |v: f32| ((v - RT_MIN) / (RT_MAX - RT_MIN)).clamp(0.0, 1.0);
                let rt_fill = |v: f32, want_down: bool| -> Option<(f32, f32)> {
                    let off = if count > 0 { !rt_on } else { !held_on };
                    if !driver || off || travel == 0 || down != want_down {
                        return None;
                    }
                    let t = f32::from(travel);
                    if !want_down {
                        return Some((0.0, ((t - scale.units(RT_MIN)) / scale.units(RT_MAX - RT_MIN)).clamp(0.0, 1.0)));
                    }
                    let thumb = rt_frac(v);
                    Some((thumb * (1.0 - (t / scale.units(v)).min(1.0)), thumb))
                };
                let sliders: Vec<RtSlider> = if split {
                    vec![(true, rapid.press, shown_press, first, rt_fill(first, false)), (false, rapid.release, shown_release, shown_release, rt_fill(shown_release, true))]
                } else {
                    vec![(true, rapid.press, shown_press, first, rt_fill(first, down))]
                };
                for (press, v, shown, mark, fill) in sliders {
                    column(ui, 72.0, |ui| {
                        ui.label(RichText::new("0.1").small().color(theme::MUTED));
                        let marks = if held_on { vec![(rt_frac(mark), theme::TEXT, 4.0)] } else { Vec::new() };
                        let s = Slider { min: RT_MIN, max: RT_MAX, value: shown, enabled: rt_on, idle: count == 0, fill, fill_color: theme::ACCENT, marks, arrow: None, tip: None };
                        if let Some(mm) = slider::show(ui, &s) {
                            events.push(if press { Event::Press(mm) } else { Event::Release(mm) });
                        }
                        ui.label(RichText::new("1.0").small().color(theme::MUTED));
                        let held_v = held.as_ref().and_then(|h| if press { h.1.press } else { h.1.release });
                        let text = if count > 0 && rt_on { label(v) } else if held_on { held_v.map(|v| format!("{v:.1}")).unwrap_or_default() } else { String::new() };
                        readout(ui, &text);
                        caption(ui, &app.t(if press { "rapid.press" } else { "rapid.release" }), split);
                    });
                }
            });
            if rt_here {
                let items = [(false, app.t("rapid.together")), (true, app.t("rapid.separate"))];
                if let Some(on) = widgets::segments(ui, &items, split, rt_on) {
                    events.push(Event::Split(on));
                }
            } else {
                ui.horizontal(|ui| {
                    ui.add(icons::image(Icon::ban, 14.0, theme::MUTED));
                    ui.label(RichText::new(app.t("rapid.hwOnly")).small().color(theme::MUTED));
                });
            }
        });
    });
    if live {
        ui.ctx().request_repaint();
    }
    for e in events {
        apply(app, e, split, fw_hw);
    }
}

/// A centred column as wide as `w` and as tall as its content.
fn column(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.set_width(w);
        ui.vertical_centered(add);
    });
}

fn mode_name(m: RapidMode) -> &'static str {
    match m {
        RapidMode::Off => "off",
        RapidMode::Rt => "rt",
        RapidMode::Crt => "crt",
    }
}

enum Event {
    Set(f32),
    Hs(f32),
    HsSplit(bool),
    Mode(RapidMode),
    Crt(bool),
    Press(f32),
    Release(f32),
    Split(bool),
}

fn apply(app: &mut App, e: Event, split: bool, fw_hw: bool) {
    let k = &mut app.keys;
    match e {
        Event::Set(mm) => k.set_value(mm),
        Event::Hs(mm) => k.set_hs(|_| mm),
        Event::HsSplit(on) => k.on_hs_split(on),
        Event::Mode(m) => k.set_rapid(|r| Rapid {
            enabled: m != RapidMode::Off,
            continuous: if m == RapidMode::Off || fw_hw { r.continuous } else { m == RapidMode::Crt },
            ..r
        }),
        Event::Crt(on) => k.set_crt(on),
        Event::Press(v) => k.set_rapid(|r| Rapid { press: v, release: if split { r.release } else { v }, ..r }),
        Event::Release(v) => k.set_rapid(|r| Rapid { release: v, ..r }),
        Event::Split(on) => k.on_split(on),
    }
}

/// The switch in section, its chevrons running down to press and up to release.
fn switch_art(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(120.0, 200.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let o = rect.min;
    let at = |x: f32, y: f32| o + vec2(x, y);
    // Two seconds a cycle: the press chevrons fall in the first half, the release ones rise in the second.
    let t = (ui.input(|i| i.time) % 2.0) as f32 / 2.0;
    let chevrons = |dy: f32, alpha: f32, up: bool| {
        if alpha <= 0.0 {
            return;
        }
        let c = theme::ACCENT.gamma_multiply(alpha);
        let color = if up { theme::TEXT.gamma_multiply(alpha) } else { c };
        for y0 in [40.0, 58.0] {
            let (a, b) = if up { (12.0, 0.0) } else { (0.0, 12.0) };
            let pts = vec![at(46.0, y0 + a + dy), at(60.0, y0 + b + dy), at(74.0, y0 + a + dy)];
            p.add(Shape::line(pts, Stroke::new(5.0, color)));
        }
    };
    let ease = |x: f32| 0.5 - 0.5 * (x * std::f32::consts::PI).cos();
    if t < 0.5 {
        let k = (t / 0.45).min(1.0);
        chevrons(-14.0 + 24.0 * ease(k), if t < 0.075 { t / 0.075 } else if t < 0.45 { 1.0 } else { 1.0 - (t - 0.45) / 0.05 }, false);
    } else {
        let k = ((t - 0.5) / 0.45).min(1.0);
        chevrons(10.0 - 24.0 * ease(k), if t < 0.575 { (t - 0.5) / 0.075 } else if t < 0.95 { 1.0 } else { 1.0 - (t - 0.95) / 0.05 }, true);
    }
    p.add(Shape::convex_polygon(vec![at(34.0, 120.0), at(86.0, 120.0), at(92.0, 160.0), at(28.0, 160.0)], theme::KEY, Stroke::NONE));
    let stroke = Stroke::new(6.0, theme::MUTED);
    p.line_segment([at(10.0, 168.0), at(20.0, 112.0)], stroke);
    let corner = |from: egui::Pos2, ctrl: egui::Pos2, to: egui::Pos2| {
        Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke([from, ctrl, to], false, Color32::TRANSPARENT, stroke))
    };
    p.add(corner(at(20.0, 112.0), at(22.0, 100.0), at(34.0, 100.0)));
    p.line_segment([at(34.0, 100.0), at(86.0, 100.0)], stroke);
    p.add(corner(at(86.0, 100.0), at(98.0, 100.0), at(100.0, 112.0)));
    p.line_segment([at(100.0, 112.0), at(110.0, 168.0)], stroke);
    p.rect_filled(egui::Rect::from_min_size(at(16.0, 160.0), vec2(88.0, 7.0)), 0.0, theme::LINE);
    p.rect_filled(egui::Rect::from_min_max(at(28.0, 167.0), at(92.0, 178.0)), 0.0, theme::KEY);
    p.rect_filled(egui::Rect::from_min_max(at(36.0, 178.0), at(84.0, 194.0)), 0.0, theme::KEY);
    for x in [54.0, 60.0, 66.0] {
        p.line_segment([at(x, 180.0), at(x, 194.0)], Stroke::new(2.0, theme::PANEL));
    }
    p.line_segment([at(34.0, 172.0), at(86.0, 172.0)], Stroke::new(3.0, theme::ERROR));
    if ui.is_rect_visible(rect) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
}
