//! Razer control protocol, independent of the OS transport.

pub mod actuation;
pub mod analog;
pub mod control;
pub mod devices;
#[cfg(test)]
mod fake;
#[cfg(feature = "hid")]
pub mod hid;
pub mod keymap;
pub mod layout;
pub mod lighting;
pub mod packet;
pub mod rapid;
pub mod transport;
