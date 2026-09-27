//! Razer control protocol, independent of the OS transport.

pub mod analog;
#[cfg(feature = "hid")]
pub mod hid;
pub mod keymap;
pub mod packet;
pub mod transport;
