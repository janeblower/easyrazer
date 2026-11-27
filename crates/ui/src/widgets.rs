//! The window's building blocks, in the looks of `app/src/style.css`.

use egui::{Color32, CornerRadius, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme;

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(theme::PANEL)
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The page spaces its blocks apart; inside a card things sit closer.
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui)
        })
        .inner
}

pub fn hint(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).small().color(theme::MUTED))
}

/// A button's fill, text and outline at rest, under the pointer and pressed.
pub struct Look {
    pub fill: [Color32; 3],
    pub text: [Color32; 3],
    pub stroke: [Stroke; 3],
}

impl Look {
    /// A tab or segment not picked.
    pub fn idle(stroke: bool) -> Self {
        let line = |c| if stroke { Stroke::new(1.0, c) } else { Stroke::NONE };
        Self { fill: [theme::PANEL, theme::HOVER, theme::KEY], text: [theme::IDLE_TEXT, theme::TEXT, theme::TEXT], stroke: [line(theme::LINE), line(theme::LINE_HOVER), line(theme::LINE_HOVER)] }
    }

    /// The same in every state.
    pub fn fixed(fill: Color32, text: Color32, stroke: Stroke) -> Self {
        Self { fill: [fill; 3], text: [text; 3], stroke: [stroke; 3] }
    }
}

/// Adds widgets drawn in `look`. An explicit fill or text color would stay the same whatever
/// the pointer does, so the look goes into the visuals of each state instead.
pub fn with_look<R>(ui: &mut Ui, look: &Look, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.override_text_color = None;
        let w = &mut v.widgets;
        // A disabled widget is drawn as noninteractive, then faded.
        for (state, i) in [(&mut w.noninteractive, 0), (&mut w.inactive, 0), (&mut w.hovered, 1), (&mut w.active, 2)] {
            state.weak_bg_fill = look.fill[i];
            state.bg_fill = look.fill[i];
            state.fg_stroke.color = look.text[i];
            state.bg_stroke = look.stroke[i];
        }
        add(ui)
    })
    .inner
}

/// The one filled button of a group: what the user most likely wants.
pub fn primary(ui: &mut Ui, text: impl Into<String>, enabled: bool) -> Response {
    let look = Look { fill: [theme::ACCENT, theme::ACCENT_HOVER, theme::ACCENT_PRESS], text: [theme::INK; 3], stroke: [Stroke::NONE; 3] };
    with_look(ui, &look, |ui| ui.add_enabled(enabled, egui::Button::new(RichText::new(text).strong())))
}

pub fn button(ui: &mut Ui, text: impl Into<String>, enabled: bool) -> Response {
    ui.add_enabled(enabled, egui::Button::new(text.into()))
}

pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, color: Color32) -> Response {
    let b = egui::Button::image(icons::image(icon, 18.0, color)).min_size(Vec2::new(34.0, 32.0));
    ui.add_enabled(enabled, b).on_hover_text(tip)
}

/// Puts something into the keyboard's flash: drawn apart, since flash wears out.
pub fn write_button(ui: &mut Ui, tip: &str, enabled: bool) -> Response {
    let b = egui::Button::image(icons::image(Icon::write, 18.0, theme::WRITE)).min_size(Vec2::new(34.0, 32.0));
    let line = Stroke::new(1.0, theme::WRITE_LINE);
    let look = Look { fill: [Color32::TRANSPARENT, theme::WRITE_HOVER, theme::WRITE_HOVER], text: [theme::WRITE; 3], stroke: [line, Stroke::new(1.0, theme::WRITE), Stroke::new(1.0, theme::WRITE)] };
    with_look(ui, &look, |ui| ui.add_enabled(enabled, b)).on_hover_text(tip)
}

/// A text button without a frame, like a link.
pub fn link(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let r = ui.add(egui::Label::new(RichText::new(text).small().color(color)).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// Segmented buttons; the chosen one is outlined, not filled — a fill is kept for Apply.
pub fn segments<T: Clone + PartialEq>(ui: &mut Ui, items: &[(T, String)], selected: T, enabled: bool) -> Option<T> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let last = items.len().saturating_sub(1);
        for (i, (value, text)) in items.iter().enumerate() {
            let on = *value == selected;
            let r = theme::RADIUS;
            let corners = CornerRadius { nw: if i == 0 { r } else { 0 }, sw: if i == 0 { r } else { 0 }, ne: if i == last { r } else { 0 }, se: if i == last { r } else { 0 } };
            let look = if on { Look::fixed(theme::KEY, theme::TEXT, Stroke::new(1.0, theme::ACCENT)) } else { Look::idle(true) };
            let b = egui::Button::new(text.as_str()).corner_radius(corners);
            if with_look(ui, &look, |ui| ui.add_enabled(enabled, b)).clicked() && !on {
                picked = Some(value.clone());
            }
        }
    });
    picked
}

/// An on/off switch with its label on the right.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str, enabled: bool) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            let size = Vec2::new(34.0, 18.0);
            let (rect, mut r) = ui.allocate_exact_size(size, Sense::click());
            if r.clicked() {
                *on = !*on;
                r.mark_changed();
            }
            let t = ui.ctx().animate_bool_responsive(r.id, *on);
            let fill = if *on { theme::ACCENT } else { theme::LINE };
            let fill = if enabled { fill } else { fill.gamma_multiply(0.4) };
            ui.painter().rect_filled(rect, 9.0, fill);
            let x = egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t);
            ui.painter().circle_filled(egui::pos2(x, rect.center().y), 7.0, if *on { theme::INK } else { theme::TEXT });
            let l = ui.add(egui::Label::new(label).sense(Sense::click()));
            if l.clicked() {
                *on = !*on;
                r.mark_changed();
            }
            r
        })
        .inner
    })
    .inner
}

/// A picture, a title and a line of text where content would be.
pub fn empty_state(ui: &mut Ui, icon: Icon, title: &str, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().rect_filled(rect, 10.0, theme::KEY);
        icons::paint(ui.painter(), icon, egui::Rect::from_center_size(rect.center(), Vec2::splat(20.0)), theme::MUTED);
        ui.label(title);
        ui.set_max_width(400.0);
        hint(ui, text);
        ui.add_space(12.0);
    });
}

/// What a modal dialog's buttons asked for.
pub enum Answer<T> {
    Open,
    /// Escape, a click outside or a cancel button.
    Cancel,
    Pick(T),
}

/// A dialog over the window; `buttons` returns the choice made this frame.
pub fn modal<T>(ctx: &egui::Context, id: &str, title: &str, body: impl FnOnce(&mut Ui), buttons: impl FnOnce(&mut Ui) -> Option<T>) -> Answer<T> {
    let frame = egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(8)
        .inner_margin(18);
    let r = egui::Modal::new(egui::Id::unique(id)).frame(frame).show(ctx, |ui| {
        ui.set_max_width(460.0);
        ui.label(RichText::new(title).heading());
        body(ui);
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| buttons(ui)).inner
    });
    match r.inner {
        Some(t) => Answer::Pick(t),
        None if r.should_close() => Answer::Cancel,
        None => Answer::Open,
    }
}

/// Text that wraps inside a dialog.
pub fn para(ui: &mut Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text).color(theme::TEXT)).wrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fills of the rectangles a frame painted.
    fn fills(ctx: &egui::Context, pointer: Option<egui::Pos2>) -> Vec<Color32> {
        let mut input = egui::RawInput::default();
        if let Some(p) = pointer {
            input.events.push(egui::Event::PointerMoved(p));
        }
        let mut out = ctx.run_ui(input, |ui| {
            with_look(ui, &Look::idle(false), |ui| ui.add(egui::Button::new("tab")));
        });
        out.textures_delta.clear();
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) => Some(r.fill),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_tab_lightens_under_the_pointer() {
        let ctx = egui::Context::default();
        let at_rest = fills(&ctx, None);
        assert!(at_rest.contains(&theme::PANEL) && !at_rest.contains(&theme::HOVER), "{at_rest:?}");
        // egui finds what is hovered from the last frame's layout.
        let pointer = Some(egui::pos2(12.0, 12.0));
        fills(&ctx, pointer);
        let hovered = fills(&ctx, pointer);
        assert!(hovered.contains(&theme::HOVER), "{hovered:?}");
    }
}
