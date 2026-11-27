//! The vertical depth slider: the top of its track is the key at rest, the bottom fully down.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::theme;

const HEIGHT: f32 = 144.0;
const TRACK: f32 = 6.0;
/// Half of the thumb: its centre never reaches the track ends.
const THUMB: f32 = 8.0;
/// Room on both sides for the arrow and the marks, so the scale stays centred.
const MARGIN: f32 = 28.0;

pub struct Slider {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub enabled: bool,
    /// Nothing selected: the scale only shows live depth, a resting thumb would read as a value.
    pub idle: bool,
    /// Lit part of the track, as fractions from the top.
    pub fill: Option<(f32, f32)>,
    pub fill_color: Color32,
    /// Lines across the track: fraction, color, how far they reach past it on each side.
    pub marks: Vec<(f32, Color32, f32)>,
    /// The key's next move next to the fill's end: `true` up.
    pub arrow: Option<bool>,
    pub tip: Option<String>,
}

impl Slider {
    pub fn frac(&self, v: f32) -> f32 {
        ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
}

/// Where fraction `f` of the range sits on the track.
fn at(track: Rect, f: f32) -> f32 {
    track.top() + THUMB + f * (track.height() - 2.0 * THUMB)
}

/// Draws the slider; returns the value picked this frame, in 0.1 mm steps.
pub fn show(ui: &mut Ui, s: &Slider) -> Option<f32> {
    let sense = if s.enabled && !s.idle { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(16.0 + 2.0 * MARGIN, HEIGHT), sense);
    let track = Rect::from_center_size(rect.center(), vec2(16.0, HEIGHT));
    let p = ui.painter();
    let line = Rect::from_center_size(track.center(), vec2(TRACK, HEIGHT - 2.0 * THUMB));
    p.rect_filled(line, 3.0, theme::KEY);
    if let Some((from, to)) = s.fill.filter(|(a, b)| b > a) {
        let r = Rect::from_min_max(pos2(line.left(), at(track, from)), pos2(line.right(), at(track, to)));
        p.rect_filled(r, 3.0, s.fill_color);
    }
    for (f, color, reach) in &s.marks {
        let y = at(track, *f);
        p.hline((track.left() - reach)..=(track.right() + reach), y, Stroke::new(2.0, *color));
    }
    if let (Some(up), Some((_, to))) = (s.arrow, s.fill) {
        let y = at(track, to);
        let x = track.left() - 9.0;
        let tri = if up {
            vec![pos2(x, y - 4.0), pos2(x + 4.0, y + 3.0), pos2(x - 4.0, y + 3.0)]
        } else {
            vec![pos2(x, y + 4.0), pos2(x + 4.0, y - 3.0), pos2(x - 4.0, y - 3.0)]
        };
        p.add(Shape::convex_polygon(tri, theme::ACCENT, Stroke::NONE));
    }
    if !s.idle {
        let y = at(track, s.frac(s.value));
        let color = if s.enabled { theme::TEXT } else { theme::MUTED };
        p.circle_filled(Pos2::new(track.center().x, y), THUMB, color);
    }
    let response = match &s.tip {
        Some(t) => response.on_hover_text(t),
        None => response,
    };
    if !s.enabled || s.idle {
        return None;
    }
    let mut picked = None;
    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let f = ((pos.y - track.top() - THUMB) / (track.height() - 2.0 * THUMB)).clamp(0.0, 1.0);
        picked = Some(s.min + f * (s.max - s.min));
    }
    if response.has_focus() {
        let step = ui.input(|i| f32::from(i.key_pressed(egui::Key::ArrowDown)) - f32::from(i.key_pressed(egui::Key::ArrowUp)));
        if step != 0.0 {
            picked = Some((s.value + step * 0.1).clamp(s.min, s.max));
        }
    }
    picked.map(|v| (v * 10.0).round() / 10.0).filter(|v| (*v - s.value).abs() > f32::EPSILON)
}
