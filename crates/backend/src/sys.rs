//! Safe wrappers over the Win32 calls the app makes; the only module allowed to use `unsafe`.

mod cert;
mod focus;
mod handle;
mod input;
mod locale;
mod ipc;
mod notify;
mod shell;
mod web;
mod wheel;
mod window;

pub use cert::*;
pub use focus::*;
pub use handle::*;
pub use input::*;
pub use locale::*;
pub use ipc::{Channel, ControlMessage};
pub use notify::*;
pub use shell::*;
pub use web::*;
pub use wheel::*;
pub use window::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
