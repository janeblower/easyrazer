//! Safe wrappers over the Win32 calls the app makes; the only module allowed to use `unsafe`.

mod focus;
mod handle;
mod input;
mod ipc;
mod notify;
mod shell;
mod wheel;
mod window;

pub use focus::*;
pub use handle::*;
pub use input::*;
pub use ipc::{Channel, ControlMessage};
pub use notify::*;
pub use shell::*;
pub use wheel::*;
pub use window::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
