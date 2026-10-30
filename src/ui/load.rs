//! Background loads for the UI: run a closure on a thread, collect its result on a
//! later frame (the thread wakes the UI when it finishes).

use eframe::egui;
use std::sync::mpsc::{Receiver, TryRecvError};

/// One piece of work running in the background. Replacing a `Load` drops its
/// receiver: a result still on its way is never applied.
#[derive(Default)]
pub enum Load<T, E> {
    #[default]
    Idle,
    Running(Receiver<Result<T, E>>),
}

impl<T: Send + 'static, E: Send + 'static> Load<T, E> {
    pub fn start(ctx: &egui::Context, work: impl FnOnce() -> Result<T, E> + Send + 'static) -> Load<T, E> {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        Load::Running(rx)
    }

    /// The result, once it arrived (the load is idle again after).
    pub fn take(&mut self) -> Option<Result<T, E>> {
        let Load::Running(rx) = self else { return None };
        match rx.try_recv() {
            Ok(result) => {
                *self = Load::Idle;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            // The thread died without an answer (it panicked): nothing will come.
            Err(TryRecvError::Disconnected) => {
                *self = Load::Idle;
                None
            }
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Load::Running(_))
    }
}
