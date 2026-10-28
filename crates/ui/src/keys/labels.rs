//! Short texts for bindings, as the key caps show them.

use easyrazer_backend::api::KeyView;
use razer_core::binding::Action;

use crate::app::App;
use crate::text::tag;

/// Bits 0..3 of the HID modifier byte; 4..7 are the same modifiers on the right.
const GLYPHS: [&str; 4] = ["⌃", "⇧", "⌥", "⊞"];

/// Text on the key cap; `None` is a binding the app does not edit.
pub fn short_label(app: &App, a: Option<&Action>, keys: &[KeyView]) -> String {
    let Some(a) = a else { return "…".into() };
    match a {
        Action::Disabled => "⊘".into(),
        Action::Key { key, mods } => {
            let mods: String = GLYPHS.iter().enumerate().filter(|(i, _)| mods & (0x11 << i) != 0).map(|(_, g)| *g).collect();
            let label = keys.iter().find(|k| k.key.key == *key).map(|k| k.key.label).filter(|l| !l.is_empty()).unwrap_or("␣");
            format!("{mods}{label}")
        }
        Action::Mouse { button } => app.t(&format!("bindings.mouseShort.{}", tag(button))),
        Action::Media { media } => app.t(&format!("bindings.mediaShort.{}", tag(media))),
        Action::Macro { id, .. } => format!("M{id}"),
        Action::System { action } => app.t(&format!("bindings.system.{}", tag(action))),
    }
}
