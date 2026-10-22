//! Razer control protocol, independent of the OS transport.

// The one Win32 call, in hid::container_id, is allowed there alone.
#![deny(unsafe_code)]

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
pub mod logged;
pub mod macros;
pub mod packet;
pub mod profiles;
pub mod rapid;
pub mod registry;
pub mod transport;
pub mod trigger;
