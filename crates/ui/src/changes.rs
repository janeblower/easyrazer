//! The parts of the app with edits of their own, shown and acted on together in the status bar.

use crate::app::{App, Place};
use crate::dialogs::Dialog;
use crate::jobs::Done;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Keys,
    Lighting,
    Macros,
}

const ORDER: [Id; 3] = [Id::Keys, Id::Lighting, Id::Macros];

/// Edits not applied yet; 0 when there are none.
fn pending(app: &App, id: Id) -> usize {
    match id {
        Id::Keys => app.dirty(),
        Id::Lighting => crate::lighting::pending(app),
        Id::Macros => crate::keys::macros::pending(app),
    }
}

fn parts(app: &App, id: Id) -> Vec<Place> {
    match id {
        Id::Keys => {
            let mut out = Vec::new();
            if app.keys.actuation_dirty() {
                out.push(Place::Actuation);
            }
            if app.keys.bindings_dirty() {
                out.push(Place::Bindings);
            }
            if app.gamepad.edit.is_some() {
                out.push(Place::Gamepad);
            }
            out
        }
        Id::Lighting => vec![Place::Lighting],
        Id::Macros => vec![Place::Macros],
    }
}

fn can_apply(app: &App, id: Id) -> bool {
    match id {
        Id::Keys => app.can_apply_keys(),
        Id::Lighting => crate::lighting::can_apply(app),
        Id::Macros => crate::keys::macros::can_apply(app),
    }
}

fn can_revert(app: &App, id: Id) -> bool {
    match id {
        Id::Keys => app.dirty() > 0 && !app.busy,
        Id::Lighting => crate::lighting::pending(app) > 0,
        Id::Macros => crate::keys::macros::pending(app) > 0,
    }
}

fn can_write(app: &App, id: Id) -> bool {
    match id {
        Id::Keys => app.can_save_keys(),
        Id::Lighting => crate::lighting::can_write(app),
        Id::Macros => false,
    }
}

fn apply(app: &mut App, id: Id, then: Done) {
    match id {
        Id::Keys => app.apply_keys(then),
        Id::Lighting => crate::lighting::apply(app, then),
        Id::Macros => crate::keys::macros::apply(app, then),
    }
}

fn revert(app: &mut App, id: Id) {
    match id {
        Id::Keys => app.revert_keys(),
        Id::Lighting => crate::lighting::revert(app),
        Id::Macros => crate::keys::macros::revert(app),
    }
}

fn write(app: &mut App, id: Id, then: Done) {
    match id {
        Id::Keys => app.write_keys(true, then),
        Id::Lighting => crate::lighting::write(app, then),
        Id::Macros => then(app),
    }
}

pub struct Summary {
    pub pending: usize,
    pub parts: Vec<Place>,
    pub can_apply: bool,
    pub can_revert: bool,
    pub can_write: bool,
    /// Edits the flash cannot hold, which Write passes by.
    pub skips_write: bool,
}

pub fn summarize(app: &App) -> Summary {
    let dirty: Vec<Id> = ORDER.into_iter().filter(|&id| pending(app, id) > 0).collect();
    Summary {
        pending: dirty.iter().map(|&id| pending(app, id)).sum(),
        parts: dirty.iter().flat_map(|&id| parts(app, id)).collect(),
        can_apply: ORDER.iter().any(|&id| can_apply(app, id)),
        can_revert: ORDER.iter().any(|&id| can_revert(app, id)),
        can_write: ORDER.iter().any(|&id| can_write(app, id)),
        skips_write: dirty.contains(&Id::Lighting) && crate::lighting::skips_write(app),
    }
}

pub fn has_unapplied(app: &App) -> bool {
    ORDER.iter().any(|&id| pending(app, id) > 0)
}

/// One after another, asking each in turn: an earlier one can leave the next unable to act.
pub fn apply_all(app: &mut App) {
    apply_from(app, 0);
}

fn apply_from(app: &mut App, from: usize) {
    for (i, &id) in ORDER.iter().enumerate().skip(from) {
        if can_apply(app, id) {
            apply(app, id, Box::new(move |app: &mut App| apply_from(app, i + 1)));
            return;
        }
    }
}

pub fn revert_all(app: &mut App) {
    for id in ORDER {
        if can_revert(app, id) || pending(app, id) > 0 {
            revert(app, id);
        }
    }
}

/// Asks before a flash write unless the user turned the question off.
pub fn write_all(app: &mut App) {
    if app.jobs.backend().app_settings().confirm_write {
        app.dialogs.push(Dialog::ConfirmWrite { dont_ask: false });
    } else {
        write_from(app, 0);
    }
}

pub fn write_from(app: &mut App, from: usize) {
    for (i, &id) in ORDER.iter().enumerate().skip(from) {
        if can_write(app, id) {
            write(app, id, Box::new(move |app: &mut App| write_from(app, i + 1)));
            return;
        }
    }
}
