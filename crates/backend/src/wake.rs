//! What wakes the watcher: a HID interface came or went, the system resumed, or something
//! changed that the next step must see. Between them the watcher only checks for Synapse.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

static TX: OnceLock<Sender<()>> = OnceLock::new();
static RETRY: AtomicBool = AtomicBool::new(false);

/// Starts the notifications; each one arrives as a message. They last as long as the process.
pub fn listen() -> Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let _ = TX.set(tx);
    // After a resume a keyboard may come back without being plugged again.
    crate::sys::notify(wake, wake);
    rx
}

/// Runs a watcher step now.
pub fn wake() {
    if let Some(tx) = TX.get() {
        let _ = tx.send(());
    }
}

/// Runs a watcher step on the next Synapse check: a busy board or a failure to show and try again.
pub fn retry() {
    RETRY.store(true, Ordering::SeqCst);
}

pub fn take_retry() -> bool {
    RETRY.swap(false, Ordering::SeqCst)
}
