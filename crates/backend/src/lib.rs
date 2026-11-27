//! Everything EasyRazer does apart from drawing its window: keyboards, profiles, the
//! remapping engine and the settings. A window talks to it only through [`api::Backend`].

// Win32 calls live behind safe wrappers in sys, the only exception.
#![deny(unsafe_code, unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks, clippy::multiple_unsafe_ops_per_block)]

pub mod api;
pub mod autostart;
pub mod boards;
pub mod dev;
pub mod device;
mod dynamic_lighting;
pub mod engine;
mod focus;
pub mod gamepad;
pub mod i18n;
mod profiles;
pub mod settings;
mod stored;
#[allow(unsafe_code)]
pub mod sys;
mod transfer;
mod wake;
mod wheel;

pub use api::{Backend, Event};
