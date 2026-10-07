//! Likes: the account's liked tracks, albums and followed artists, toggled from
//! hearts across the UI. A toggle shows at once and is sent in the background;
//! if Deezer refuses, it is rolled back and the error shown.

use crate::deezer::{Deezer, Error, Likeable, Likes};
use crate::ui::app::App;
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
        self.likes.likes.contains(what)
    }

    /// Flip a like: shown immediately, confirmed by Deezer in the background.
    pub(in crate::ui) fn toggle_like(&mut self, ctx: &egui::Context, what: Likeable) {
        let Some(client) = self.client.clone() else { return };
        let liked = !self.is_liked(&what);
        self.likes.likes.set(&what, liked);
        self.like_changed(&what);
        let (tx, ctx) = (self.likes.done_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = client.set_liked(&what, liked);
            let _ = tx.send((what, liked, result));
            ctx.request_repaint();
        });
    }

    /// Collect toggle results; undo the ones Deezer refused.
    pub(in crate::ui) fn poll_likes(&mut self) {
        loop {
            match self.likes.done.try_recv() {
                Ok((what, liked, Err(e))) => {
                    log_warn!("like {what:?}: {e}");
                    self.likes.likes.set(&what, !liked);
                    self.like_changed(&what);
                    self.list_error = Some(format!("Couldn't update your likes: {e}"));
                }
                Ok(_) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            }
        }
    }

    /// Lists built from likes are out of date now; reload them when next shown.
    fn like_changed(&mut self, what: &Likeable) {
        match what {
            Likeable::Artist(_) => self.followed.clear(),
            Likeable::Album(_) => self.favorite_albums.clear(),
            Likeable::Track(_) => {}
        }
    }
}

/// Load the account's likes (after login).
pub fn load(client: &Deezer) -> Result<Likes, Error> {
    client.likes()
}
