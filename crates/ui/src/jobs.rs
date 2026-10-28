//! Backend calls off the window's thread: the device lock can be held for seconds by a read.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use easyrazer_backend::Backend;

use crate::app::App;

/// What to do with a call's result, on the window's thread.
pub type Done = Box<dyn FnOnce(&mut App) + Send>;

#[derive(Clone)]
pub struct Jobs {
    backend: Arc<Backend>,
    tx: Sender<Done>,
    ctx: egui::Context,
}

impl Jobs {
    pub fn new(backend: Arc<Backend>, ctx: egui::Context) -> (Self, Receiver<Done>) {
        let (tx, rx) = channel();
        (Self { backend, tx, ctx }, rx)
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// Runs `work` on its own thread and hands its result to `then` on the window's thread.
    pub fn run<T: Send + 'static>(&self, work: impl FnOnce(&Backend) -> T + Send + 'static, then: impl FnOnce(&mut App, T) + Send + 'static) {
        let (backend, tx, ctx) = (self.backend.clone(), self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = work(&backend);
            let _ = tx.send(Box::new(move |app: &mut App| then(app, r)));
            ctx.request_repaint();
        });
    }

    /// Makes the window run its logic again after `delay`, even while hidden.
    pub fn wake_after(&self, delay: std::time::Duration) {
        self.ctx.request_repaint_after(delay);
    }
}
