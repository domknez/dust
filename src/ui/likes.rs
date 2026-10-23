//! Likes: the account's liked tracks, albums and followed artists, toggled from
//! hearts across the UI. A toggle shows at once and is sent in the background;
//! if the source refuses, it is rolled back and the error shown as a notice.

use crate::model::{Error, Likeable, Likes};
use crate::ui::app::App;
use crate::ui::load::Load;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub struct LikeState {
    pub likes: Likes,
    /// Finished toggles: what, the state that was asked for, and the outcome.
    done_tx: Sender<(Likeable, bool, Result<(), Error>)>,
    done: Receiver<(Likeable, bool, Result<(), Error>)>,
}

impl Default for LikeState {
    fn default() -> Self {
        let (done_tx, done) = channel();
        Self { likes: Likes::default(), done_tx, done }
    }
}

impl App {
    pub(in crate::ui) fn is_liked(&self, what: &Likeable) -> bool {
        self.library.likes.likes.contains(what)
    }

    /// Flip a like: shown immediately, confirmed by the source in the background.
    pub(in crate::ui) fn toggle_like(&mut self, what: Likeable) {
        let Some(client) = self.client.clone() else { return };
        let liked = !self.is_liked(&what);
        self.library.likes.likes.set(&what, liked);
        self.like_changed(&what);
        let tx = self.library.likes.done_tx.clone();
        self.jobs.run(move || {
            let result = client.set_liked(&what, liked);
            let _ = tx.send((what, liked, result));
        });
    }

    /// Collect toggle results; undo the ones the source refused.
    pub(in crate::ui) fn poll_likes(&mut self, ctx: &egui::Context) {
        loop {
            match self.library.likes.done.try_recv() {
                Ok((what, liked, Err(e))) => {
                    log_warn!("like {what:?}: {e}");
                    self.library.likes.likes.set(&what, !liked);
                    self.like_changed(&what);
                    self.notice.error(ctx, format!("Couldn't update your likes: {e}"));
                }
                Ok(_) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            }
        }
    }

    /// Lists built from likes are out of date now; reload them when next shown.
    fn like_changed(&mut self, what: &Likeable) {
        match what {
            Likeable::Artist(_) => self.library.followed = Load::Idle,
            Likeable::Album(_) => self.library.favorite_albums = Load::Idle,
            Likeable::Track(_) => {}
        }
    }
}
