//! Razer control protocol, independent of the OS transport.

pub mod actuation;
pub mod analog;
pub mod binding;
pub mod control;
pub mod devices;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
#[cfg(feature = "hid")]
pub mod hid;
pub mod keymap;
pub mod layout;
pub mod lighting;
pub mod macros;
pub mod packet;
pub mod profiles;
pub mod rapid;
pub mod transport;
