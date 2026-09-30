//! Background work for the UI: run a closure on a thread, poll for its result
//! each frame, and wake the UI when it finishes.

use eframe::egui;
use std::sync::mpsc::{Receiver, TryRecvError};

/// A pending background result; `None` when idle.
pub type Task<T> = Option<Receiver<Result<T, String>>>;

pub fn spawn<T: Send + 'static>(ctx: &egui::Context, work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Task<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(work());
        ctx.request_repaint();
    });
    Some(rx)
}

/// `Some(result)` once the task finished (and the task becomes idle).
pub fn poll<T>(task: &mut Task<T>) -> Option<Result<T, String>> {
    let result = match task.as_ref()?.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => Err("task failed".into()),
    };
    *task = None;
    Some(result)
}
