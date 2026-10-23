//! Background loads for the UI: run a closure on a thread, collect its result on a
//! later frame (the thread wakes the UI when it finishes) and keep it, or why it
//! failed, with the resource it was for.

use crate::model::{Error, Result};
use eframe::egui;
use std::sync::mpsc::{Receiver, TryRecvError};

/// One resource loaded in the background. Starting it again, or replacing the
/// `Load`, drops the receiver: a result still on its way is never applied.
#[derive(Default)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading(Receiver<Result<T>>),
    Ready(T),
    /// Why it failed, worded for the listener.
    Failed(String),
}

impl<T: Send + 'static> Load<T> {
    /// Run `work` on a thread; whatever was here before is dropped.
    pub fn start(&mut self, ctx: &egui::Context, work: impl FnOnce() -> Result<T> + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        *self = Load::Loading(rx);
    }
}

impl<T> Load<T> {
    /// Keep a finished result: `Loading` becomes `Ready` or `Failed`. Returns the
    /// error when it just failed, for the caller to act on (an expired session).
    pub fn poll(&mut self) -> Option<Error> {
        match self.finished()? {
            Ok(value) => {
                *self = Load::Ready(value);
                None
            }
            Err(e) => Some(e),
        }
    }

    /// For a result merged into other state: hand over a finished one. Success
    /// leaves the load `Idle`; a failure is kept (`Failed`) and returned too.
    pub fn poll_take(&mut self) -> Option<Result<T>> {
        let result = self.finished()?;
        if result.is_ok() {
            *self = Load::Idle;
        }
        Some(result)
    }

    /// The result once it arrived; a failure is already stored as `Failed`.
    fn finished(&mut self) -> Option<Result<T>> {
        let Load::Loading(rx) = self else { return None };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(Error::Other("background task failed".into())),
        };
        if let Err(e) = &result {
            *self = Load::Failed(e.to_string());
        }
        Some(result)
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading(_))
    }

    /// Why the last attempt failed.
    pub fn error(&self) -> Option<&str> {
        match self {
            Load::Failed(e) => Some(e),
            _ => None,
        }
    }
}

impl<T> Load<Vec<T>> {
    /// A cached list is fetched when shown unless it's loading or has items: the
    /// first time, and again after a failure, an empty answer or a reset.
    pub fn wants_load(&self) -> bool {
        !self.is_loading() && self.ready().is_none_or(Vec::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// A load whose result the test sends by hand.
    fn loading<T>() -> (Load<T>, std::sync::mpsc::Sender<Result<T>>) {
        let (tx, rx) = channel();
        (Load::Loading(rx), tx)
    }

    #[test]
    fn loading_then_ready() {
        let (mut load, tx) = loading::<u32>();
        assert!(load.is_loading());
        assert!(load.poll().is_none(), "nothing arrived yet");
        assert!(load.is_loading());
        tx.send(Ok(7)).unwrap();
        assert!(load.poll().is_none());
        assert_eq!(load.ready(), Some(&7));
        assert!(!load.is_loading() && load.error().is_none());
        assert!(load.poll().is_none(), "a ready load stays ready");
        assert_eq!(load.ready(), Some(&7));
    }

    #[test]
    fn failure_is_returned_once_and_kept() {
        let (mut load, tx) = loading::<u32>();
        tx.send(Err(Error::Other("no luck".into()))).unwrap();
        assert!(matches!(load.poll(), Some(Error::Other(m)) if m == "no luck"));
        assert_eq!(load.error(), Some("no luck"));
        assert!(load.poll().is_none(), "the error is news only once");
        assert_eq!(load.error(), Some("no luck"));
        assert!(load.ready().is_none());
    }

    #[test]
    fn a_dropped_worker_fails_the_load() {
        let (mut load, tx) = loading::<u32>();
        drop(tx);
        assert!(load.poll().is_some());
        assert_eq!(load.error(), Some("background task failed"));
    }

    #[test]
    fn take_hands_the_value_over() {
        let (mut load, tx) = loading::<Vec<u32>>();
        assert!(load.poll_take().is_none());
        tx.send(Ok(vec![1, 2])).unwrap();
        assert_eq!(load.poll_take().map(Result::unwrap), Some(vec![1, 2]));
        assert!(matches!(load, Load::Idle));
        assert!(load.poll_take().is_none());

        let (mut load, tx) = loading::<Vec<u32>>();
        tx.send(Err(Error::Other("gone".into()))).unwrap();
        assert!(load.poll_take().is_some_and(|r| r.is_err()));
        assert_eq!(load.error(), Some("gone"), "a failure stays with the load");
    }

    #[test]
    fn errors_stay_with_their_own_load() {
        let (mut likes, likes_tx) = loading::<u32>();
        let (mut tracks, tracks_tx) = loading::<u32>();
        likes_tx.send(Err(Error::Other("like refused".into()))).unwrap();
        tracks_tx.send(Ok(3)).unwrap();
        likes.poll();
        tracks.poll();
        assert_eq!(likes.error(), Some("like refused"));
        assert_eq!((tracks.ready(), tracks.error()), (Some(&3), None));
    }

    #[test]
    fn lists_load_until_they_have_items() {
        assert!(Load::<Vec<u32>>::Idle.wants_load());
        assert!(Load::<Vec<u32>>::Failed("offline".into()).wants_load());
        assert!(Load::<Vec<u32>>::Ready(Vec::new()).wants_load());
        assert!(!Load::Ready(vec![1]).wants_load());
        assert!(!loading::<Vec<u32>>().0.wants_load());
    }

    #[test]
    fn starting_again_drops_the_old_result() {
        let ctx = egui::Context::default();
        let (mut load, old) = loading::<u32>();
        load.start(&ctx, || Ok(2));
        assert!(old.send(Ok(1)).is_err(), "the old receiver is gone");
        while load.is_loading() {
            load.poll();
            std::thread::yield_now();
        }
        assert_eq!(load.ready(), Some(&2));
    }
}
