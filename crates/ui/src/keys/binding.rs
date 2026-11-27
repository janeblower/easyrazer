//! What one key does in one layer: a key with modifiers, a mouse button, media, a macro, a
//! system function or nothing.

use egui::{RichText, Stroke, Ui};
use razer_core::binding::{Action, MacroMode, Mouse, System};
use razer_core::rapid::Media;

use super::map::system_icon;
use super::model::UNIQUE;
use crate::app::App;
use crate::icons::{self, Icon};
use crate::text::tag;
use crate::{theme, widgets};

pub const MOUSE: [Mouse; 7] = [Mouse::Left, Mouse::Right, Mouse::Middle, Mouse::Back, Mouse::Forward, Mouse::WheelUp, Mouse::WheelDown];
pub const MEDIA: [Media; 7] = [Media::Play, Media::Prev, Media::Next, Media::Stop, Media::Mute, Media::VolumeUp, Media::VolumeDown];
const SYSTEM: [System; 8] =
    [System::BrightnessDown, System::BrightnessUp, System::Sleep, System::GameMode, System::MacroLed, System::NextProfile, System::SnapTap, System::GamepadToggle];
/// Bits 0..3 of the HID modifier byte; 4..7 are the same modifiers on the right.
const MODS: [&str; 4] = ["ctrl", "shift", "alt", "win"];
const MODES: [MacroMode; 3] = [MacroMode::Times, MacroMode::Hold, MacroMode::Toggle];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Type {
    Key,
    Mouse,
    Media,
    Macro,
    System,
    Disabled,
}

impl Type {
    fn of(a: &Action) -> Self {
        match a {
            Action::Key { .. } => Self::Key,
            Action::Mouse { .. } => Self::Mouse,
            Action::Media { .. } => Self::Media,
            Action::Macro { .. } => Self::Macro,
            Action::System { .. } => Self::System,
            Action::Disabled => Self::Disabled,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Mouse => "mouse",
            Self::Media => "media",
            Self::Macro => "macro",
            Self::System => "system",
            Self::Disabled => "disabled",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Key => Icon::keyboard,
            Self::Mouse => Icon::mouse,
            Self::Media => Icon::music,
            Self::Macro => Icon::macro_,
            Self::System => Icon::cog,
            Self::Disabled => Icon::ban,
        }
    }
}

/// A modifier counts on either side; the window sets the left one.
fn sides(bit: u8) -> u8 {
    (1 << bit) | (1 << (bit + 4))
}

/// One layer's binding of the picked key; returns the action picked this frame.
#[allow(clippy::too_many_arguments)]
pub fn card(app: &mut App, ui: &mut Ui, title: &str, key: Option<u8>, action: Option<Action>, factory: Option<Action>, hypershift: bool) -> Option<Action> {
    let mut out = None;
    let macros: Vec<(u16, String)> = app.macro_state.as_ref().map(|m| m.macros.iter().map(|(id, m)| (*id, m.name.clone())).collect()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::revert, &app.t("bindings.factory"), key.is_some() && factory.is_some(), theme::TEXT).clicked() {
                out = factory;
            }
        });
    });
    let current = action.as_ref().map(Type::of);
    let types: &[Type] = if hypershift {
        &[Type::Key, Type::Mouse, Type::Media, Type::Macro, Type::System, Type::Disabled]
    } else {
        &[Type::Key, Type::Mouse, Type::Media, Type::Macro, Type::Disabled]
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for &ty in types {
            let on = key.is_some() && current == Some(ty);
            let accent = if hypershift { theme::HS } else { theme::ACCENT };
            let b = egui::Button::image(icons::image(ty.icon(), 18.0, if on { theme::TEXT } else { theme::MUTED }))
                .stroke(Stroke::new(1.0, if on { accent } else { theme::LINE }))
                .min_size(egui::vec2(34.0, 32.0));
            let enabled = key.is_some() && (ty != Type::Macro || !macros.is_empty());
            let tip = if ty == Type::Macro && macros.is_empty() { app.t("bindings.noMacros") } else { app.t(&format!("bindings.types.{}", ty.name())) };
            if ui.add_enabled(enabled, b).on_hover_text(&tip).on_disabled_hover_text(&tip).clicked()
                && current != Some(ty)
                && let Some(k) = key
            {
                out = match ty {
                    Type::Key => Some(Action::Key { key: k, mods: 0 }),
                    Type::Mouse => Some(Action::Mouse { button: Mouse::Left }),
                    Type::Media => Some(Action::Media { media: Media::Play }),
                    Type::System => Some(Action::System { action: System::BrightnessDown }),
                    Type::Disabled => Some(Action::Disabled),
                    Type::Macro => macros.first().map(|(id, _)| Action::Macro { id: *id, mode: MacroMode::Times, count: 1 }),
                };
            }
        }
        ui.add_space(8.0);
        if let (Some(_), Some(t)) = (key, current) {
            widgets::hint(ui, app.t(&format!("bindings.types.{}", t.name())));
        }
    });
    if key.is_some() && action.is_none() {
        widgets::hint(ui, app.t("bindings.other"));
    }
    let field = |ui: &mut Ui, label: String, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            ui.add_sized([100.0, 24.0], egui::Label::new(RichText::new(label).color(theme::MUTED)));
            add(ui);
        });
    };
    match action {
        Some(Action::Key { key: target, mods }) => {
            let targets: Vec<(u8, String)> = app.layout.iter().filter(|k| k.key.editable).map(|k| (k.key.key, super::key_name(app, k))).collect();
            let name = targets.iter().find(|(k, _)| *k == target).map(|(_, n)| n.clone()).unwrap_or_default();
            let search = app.t("bindings.search");
            let query = &mut app.keys_view.query;
            field(ui, app_t(&app.lang, "bindings.key"), &mut |ui| {
                let id = ui.make_persistent_id(("target", hypershift));
                let r = egui::ComboBox::from_id_salt(id).selected_text(&name).width(200.0).height(300.0).show_ui(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(query).hint_text(&search)).request_focus();
                    let q = query.trim().to_lowercase();
                    for (k, n) in &targets {
                        if (q.is_empty() || n.to_lowercase().contains(&q)) && ui.selectable_label(*k == target, n).clicked() {
                            out = Some(Action::Key { key: *k, mods });
                        }
                    }
                });
                if r.response.clicked() {
                    query.clear();
                }
            });
            field(ui, app_t(&app.lang, "bindings.mods"), &mut |ui| {
                for (bit, m) in MODS.iter().enumerate() {
                    let on = mods & sides(bit as u8) != 0;
                    let b = egui::Button::new(RichText::new(app_t(&app.lang, &format!("bindings.modNames.{m}"))).small().color(if on { theme::TEXT } else { theme::MUTED }))
                        .stroke(Stroke::new(1.0, if on { theme::ACCENT } else { theme::LINE }));
                    if ui.add(b).clicked() {
                        let mods = if on { mods & !sides(bit as u8) } else { mods | (1 << bit) };
                        out = Some(Action::Key { key: target, mods });
                    }
                }
            });
        }
        Some(Action::Mouse { button }) => field(ui, app.t("bindings.button"), &mut |ui| {
            let names: Vec<String> = MOUSE.iter().map(|b| app_t(&app.lang, &format!("bindings.mouse.{}", tag(b)))).collect();
            let i = MOUSE.iter().position(|b| *b == button).unwrap_or(0);
            egui::ComboBox::from_id_salt(("mouse", hypershift)).selected_text(&names[i]).width(200.0).show_ui(ui, |ui| {
                for (j, b) in MOUSE.iter().enumerate() {
                    if ui.selectable_label(j == i, &names[j]).clicked() {
                        out = Some(Action::Mouse { button: *b });
                    }
                }
            });
        }),
        Some(Action::Media { media }) => field(ui, app.t("bindings.media"), &mut |ui| {
            let names: Vec<String> = MEDIA.iter().map(|m| app_t(&app.lang, &format!("bindings.mediaNames.{}", tag(m)))).collect();
            let i = MEDIA.iter().position(|m| *m == media).unwrap_or(0);
            egui::ComboBox::from_id_salt(("media", hypershift)).selected_text(&names[i]).width(200.0).show_ui(ui, |ui| {
                for (j, m) in MEDIA.iter().enumerate() {
                    if ui.selectable_label(j == i, &names[j]).clicked() {
                        out = Some(Action::Media { media: *m });
                    }
                }
            });
        }),
        Some(Action::Macro { id, mode, count }) => {
            field(ui, app.t("bindings.macro"), &mut |ui| {
                let shown = macros.iter().find(|(m, _)| *m == id).map_or(format!("#{id}"), |(m, n)| format!("{m}. {n}"));
                egui::ComboBox::from_id_salt(("macro", hypershift)).selected_text(shown).width(200.0).show_ui(ui, |ui| {
                    for (m, n) in &macros {
                        if ui.selectable_label(*m == id, format!("{m}. {n}")).clicked() {
                            out = Some(Action::Macro { id: *m, mode, count });
                        }
                    }
                });
            });
            field(ui, app.t("bindings.play"), &mut |ui| {
                let name = |m: &MacroMode| app_t(&app.lang, &format!("bindings.modes.{}", tag(m)));
                egui::ComboBox::from_id_salt(("mode", hypershift)).selected_text(name(&mode)).width(200.0).show_ui(ui, |ui| {
                    for m in MODES {
                        if ui.selectable_label(m == mode, name(&m)).clicked() {
                            out = Some(Action::Macro { id, mode: m, count: count.max(1) });
                        }
                    }
                });
                if mode == MacroMode::Times {
                    let mut n = count.max(1);
                    if ui.add(egui::DragValue::new(&mut n).range(1..=255)).changed() {
                        out = Some(Action::Macro { id, mode, count: n });
                    }
                }
            });
        }
        Some(Action::System { action: s }) => field(ui, app.t("bindings.function"), &mut |ui| {
            let name = |s: &System| app_t(&app.lang, &format!("bindings.system.{}", tag(s)));
            let taken = taken(app, key);
            egui::ComboBox::from_id_salt(("system", hypershift)).selected_text(name(&s)).width(240.0).show_ui(ui, |ui| {
                for x in SYSTEM {
                    let mut text = name(&x);
                    if let Some(c) = taken.iter().find(|(t, _)| *t == x).map(|(_, c)| c) {
                        text = format!("{text}  — {c}");
                    }
                    let icon = system_icon(x).map(|i| icons::image(i, 16.0, theme::TEXT));
                    let r = match icon {
                        Some(img) => ui.add(egui::Button::image_and_text(img, text).selected(x == s).frame(false)),
                        None => ui.selectable_label(x == s, text),
                    };
                    if r.clicked() {
                        out = Some(Action::System { action: x });
                    }
                }
            });
        }),
        Some(Action::Disabled) | None => {}
    }
    out
}

fn app_t(lang: &str, key: &str) -> String {
    easyrazer_backend::i18n::t(lang, key)
}

/// Other keys holding a unique system action, as "Fn+LShift".
fn taken(app: &App, key: Option<u8>) -> Vec<(System, String)> {
    UNIQUE
        .iter()
        .filter_map(|s| {
            let k = super::holder(app, *s)?;
            (Some(k) != key).then(|| (*s, super::combo(app, Some(k)).unwrap_or_default()))
        })
        .collect()
}
