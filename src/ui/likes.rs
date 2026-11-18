//! Likes: the account's liked tracks, albums and followed artists, toggled from
//! hearts across the UI. A toggle shows at once and is sent in the background; a
//! toast says it's done, or, if the source refuses, it is rolled back and the
//! error shown instead.

use crate::model::{Error, Likeable, Likes};
use crate::ui::app::App;
use crate::ui::load::Load;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub struct LikeState {
    pub likes: Likes,
    /// Goes up whenever `likes` changes, for what's worked out from them.
    pub revision: u64,
    /// Finished toggles: what, the state that was asked for, and the outcome.
    done_tx: Sender<(Likeable, bool, Result<(), Error>)>,
    done: Receiver<(Likeable, bool, Result<(), Error>)>,
}

impl Default for LikeState {
    fn default() -> Self {
        let (done_tx, done) = channel();
        Self { likes: Likes::default(), revision: 0, done_tx, done }
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

    /// Collect toggle results: confirm them, or undo the ones the source refused.
    /// An expired session goes to `failed`, to end it.
    pub(in crate::ui) fn poll_likes(&mut self, ctx: &egui::Context, failed: &mut Vec<Error>) {
        loop {
            match self.library.likes.done.try_recv() {
                Ok((what, liked, Err(e))) => {
                    self.library.likes.likes.set(&what, !liked);
                    self.like_changed(&what);
                    if e.is_session_expired() {
                        failed.push(e);
                    } else {
                        log_warn!("like {what:?}: {e}");
                        self.toasts.error(ctx, format!("Couldn't update your likes: {e}"));
                    }
                }
                Ok((what, liked, Ok(()))) => self.toasts.info(ctx, done_message(&what, liked)),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            }
        }
    }

    /// Lists built from likes are out of date now; reload them when next shown.
    fn like_changed(&mut self, what: &Likeable) {
        self.library.likes.revision += 1;
        match what {
            Likeable::Artist(_) => self.library.followed = Load::Idle,
            Likeable::Album(_) => self.library.favorite_albums = Load::Idle,
            Likeable::Track(_) => {}
        }
    }
}

/// What a like or follow that went through did.
fn done_message(what: &Likeable, liked: bool) -> &'static str {
    match (what, liked) {
        (Likeable::Track(_), true) => "Added to Loved tracks",
        (Likeable::Track(_), false) => "Removed from Loved tracks",
        (Likeable::Album(_), true) => "Added to your albums",
        (Likeable::Album(_), false) => "Removed from your albums",
        (Likeable::Artist(_), true) => "Following the artist",
        (Likeable::Artist(_), false) => "No longer following the artist",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackId;

    #[test]
    fn likes_that_went_through_say_so() {
        assert_eq!(done_message(&Likeable::Track(TrackId::from(1)), true), "Added to Loved tracks");
        assert_eq!(done_message(&Likeable::Album("302127".into()), false), "Removed from your albums");
        assert_eq!(done_message(&Likeable::Artist("27".into()), true), "Following the artist");
    }
}
