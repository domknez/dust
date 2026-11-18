//! Editing the account's own playlists from the UI: add tracks, create (from a track,
//! a list or the queue), remove tracks, rename, delete. Dialogs ask for names and
//! confirm deletes; a [toast](crate::ui::toasts) reports what happened.

use crate::model::{Error, Playlist, PlaylistId, Track, TrackId};
use crate::source::Source;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::format::count;
use crate::ui::load::Load;
use crate::ui::state::{CollSource, View, for_playlist};
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, Playlists};
use eframe::egui::{self, Key, RichText};

/// A change to make on the music source (Deezer or a server).
pub enum Edit {
    Add { playlist: Playlist, tracks: Vec<Track> },
    Create { title: String, private: bool, tracks: Vec<Track> },
    Remove { playlist: PlaylistId, tracks: Vec<TrackId> },
    Rename { playlist: Playlist, title: String },
    Delete { playlist: Playlist },
}

/// An open dialog.
pub enum Dialog {
    /// Name a new playlist; it starts with `tracks` (possibly none).
    Create {
        title: String,
        private: bool,
        tracks: Vec<Track>,
    },
    Rename {
        playlist: Playlist,
        title: String,
    },
    Delete {
        playlist: Playlist,
    },
}

/// What finished: the toast to show, and the playlist it changed (to reload it).
pub struct Done {
    message: String,
    playlist: Option<PlaylistId>,
    deleted: bool,
}

#[derive(Default)]
pub struct PlaylistEditing {
    pub dialog: Option<Dialog>,
    /// Edits sent and not back yet, oldest first: each keeps its own result.
    tasks: Vec<Load<Done>>,
}

impl PlaylistEditing {
    pub fn busy(&self) -> bool {
        self.tasks.iter().any(Load::is_loading)
    }

    /// The edits that came back since the last call, oldest first.
    fn finished(&mut self) -> Vec<Result<Done, Error>> {
        let done = self.tasks.iter_mut().filter_map(Load::poll_take).collect();
        self.tasks.retain(Load::is_loading);
        done
    }
}

fn run(client: &dyn Source, edit: Edit) -> Result<Done, Error> {
    let ids = |tracks: Vec<Track>| for_playlist(tracks).into_iter().map(|t| t.id).collect::<Vec<_>>();
    Ok(match edit {
        Edit::Add { playlist, tracks } => {
            let added = client.add_to_playlist(&playlist.id, &ids(tracks))?;
            let message = match added {
                0 => format!("Already in {}", playlist.title),
                n => format!("Added {} to {}", count(n, "track"), playlist.title),
            };
            Done { message, playlist: Some(playlist.id), deleted: false }
        }
        Edit::Create { title, private, tracks } => {
            let id = client.create_playlist(&title, private, &ids(tracks))?;
            Done { message: format!("Created {title}"), playlist: id, deleted: false }
        }
        Edit::Remove { playlist, tracks } => {
            client.remove_from_playlist(&playlist, &tracks)?;
            Done { message: format!("Removed {}", count(tracks.len(), "track")), playlist: Some(playlist), deleted: false }
        }
        Edit::Rename { playlist, title } => {
            client.rename_playlist(&playlist, &title)?;
            Done { message: format!("Renamed to {title}"), playlist: Some(playlist.id), deleted: false }
        }
        Edit::Delete { playlist } => {
            client.delete_playlist(&playlist.id)?;
            Done { message: format!("Deleted {}", playlist.title), playlist: Some(playlist.id), deleted: true }
        }
    })
}

impl App {
    pub(in crate::ui) fn edit_playlist(&mut self, edit: Edit) {
        let Some(client) = self.client.clone() else { return };
        if let Edit::Add { playlist, .. } = &edit {
            self.prefs.used_playlist(playlist.id.clone());
        }
        let mut task = Load::default();
        task.start(&self.jobs, move || run(&*client, edit));
        self.library.editing.tasks.push(task);
    }

    /// What "Add to playlist" offers: the account's playlists, those used last first.
    pub(in crate::ui) fn playlist_choices(&self) -> Playlists<'_> {
        Playlists { all: &self.library.playlists, recent: &self.prefs.recent_playlists }
    }

    /// Ask for a name, then create a playlist with `tracks`.
    pub(in crate::ui) fn new_playlist(&mut self, tracks: Vec<Track>) {
        self.library.editing.dialog = Some(Dialog::Create { title: String::new(), private: false, tracks });
    }

    /// Collect the finished edits: toast, refreshed sidebar, reloaded page. An
    /// expired session goes to `failed`, to end it.
    pub(in crate::ui) fn poll_playlist_edits(&mut self, ctx: &egui::Context, failed: &mut Vec<Error>) {
        let results = self.library.editing.finished();
        if results.is_empty() {
            return;
        }
        // Even a failed edit may have changed some of it.
        if let Some(client) = self.client.clone() {
            self.library.playlists_load.start(&self.jobs, move || client.playlists());
        }
        for result in results {
            match result {
                Ok(done) => self.edit_done(ctx, done),
                Err(e) if e.is_session_expired() => failed.push(e),
                Err(e) => self.toasts.error(ctx, format!("Couldn't change the playlist: {e}")),
            }
        }
    }

    fn edit_done(&mut self, ctx: &egui::Context, done: Done) {
        self.toasts.info(ctx, done.message);
        let showing = matches!(&self.browse.view, View::Collection(c) if done.playlist.as_ref().is_some_and(|id| matches!(&c.source, CollSource::Playlist(p) if p == id)));
        if showing && done.deleted {
            self.open_view(View::Playlists);
        } else if showing {
            self.show_view(self.browse.view.clone());
        }
    }

    /// The open dialog, if any (call once per frame).
    pub(in crate::ui) fn playlist_dialog(&mut self, ctx: &egui::Context, acts: &mut Actions) {
        let Some(mut dialog) = self.library.editing.dialog.take() else { return };
        let p = colors();
        let private_playlists = self.features().private_playlists;
        let service = self.client.as_ref().map_or("", |c| c.id().name);
        let mut submit = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("playlist-dialog")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            let confirm = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                ui.add_enabled(enabled, egui::Button::new(RichText::new(label).strong())).clicked()
                    || (enabled && ui.input(|i| i.key_pressed(Key::Enter)))
            };
            match &mut dialog {
                Dialog::Create { title, private, tracks } => {
                    ui.label(RichText::new("New playlist").font(ty::SECTION_TITLE.font()).color(p.text));
                    if !tracks.is_empty() {
                        ui.label(RichText::new(format!("With {}", count(tracks.len(), "track"))).color(p.dim));
                    }
                    let field = ui.add(egui::TextEdit::singleline(title).hint_text("Name").desired_width(f32::INFINITY));
                    field.request_focus();
                    if private_playlists {
                        ui.checkbox(private, "Private (only you can see it)");
                    }
                    ui.horizontal(|ui| {
                        if confirm(ui, "Create", !title.trim().is_empty()) {
                            submit =
                                Some(Edit::Create { title: title.trim().to_string(), private: *private, tracks: std::mem::take(tracks) });
                        }
                        close |= ui.button("Cancel").clicked();
                    });
                }
                Dialog::Rename { playlist, title } => {
                    ui.label(RichText::new("Rename playlist").font(ty::SECTION_TITLE.font()).color(p.text));
                    let field = ui.add(egui::TextEdit::singleline(title).desired_width(f32::INFINITY));
                    field.request_focus();
                    ui.horizontal(|ui| {
                        let changed = !title.trim().is_empty() && title.trim() != playlist.title;
                        if confirm(ui, "Rename", changed) {
                            submit = Some(Edit::Rename { playlist: playlist.clone(), title: title.trim().to_string() });
                        }
                        close |= ui.button("Cancel").clicked();
                    });
                }
                Dialog::Delete { playlist } => {
                    ui.label(RichText::new("Delete playlist?").font(ty::SECTION_TITLE.font()).color(p.text));
                    ui.label(RichText::new(format!("“{}” will be deleted from your {service} account.", playlist.title)).color(p.dim));
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Delete").strong().color(p.danger)).clicked() {
                            submit = Some(Edit::Delete { playlist: playlist.clone() });
                        }
                        close |= ui.button("Cancel").clicked();
                    });
                }
            }
        });
        close |= modal.should_close();
        match submit {
            Some(edit) => acts.push(Action::EditPlaylist(edit)),
            None if !close => self.library.editing.dialog = Some(dialog),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing::{TestApp, deezer};

    #[test]
    fn edits_sent_together_each_come_back() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        t.app.edit_playlist(Edit::Remove { playlist: 5.into(), tracks: vec![1.into()] });
        t.app.edit_playlist(Edit::Remove { playlist: 6.into(), tracks: vec![2.into()] });
        assert_eq!(t.app.library.editing.tasks.len(), 2, "the second doesn't drop the first");
        t.jobs.run_held();
        let finished = t.app.library.editing.finished();
        assert_eq!(finished.len(), 2);
        assert!(!t.app.library.editing.busy());
    }
}
