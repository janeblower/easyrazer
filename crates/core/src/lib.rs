//! Razer control protocol, independent of the OS transport.

pub mod analog;
pub mod control;
#[cfg(test)]
mod fake;
#[cfg(feature = "hid")]
pub mod hid;
pub mod keymap;
pub mod packet;
pub mod transport;
