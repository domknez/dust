//! The command palette (⌘K, Ctrl+K elsewhere): type to find a page, a playlist, an
//! artist you listen to, or something to do with the track playing (like it, add
//! it to a playlist, save it to the Crate), Flow by mood, player controls; Enter
//! does it. The last row searches for what was typed.

use crate::model::{ArtistRef, ItemKind, Likeable, Track};
use crate::player::{Cmd, Repeat, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::{App, SidePanel};
use crate::ui::crate_store::Thing;
use crate::ui::shortcuts;
use crate::ui::state::{Coll, View};
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::views::SleepChoice;
use crate::ui::widgets::{self, TrackMenu, text_left};
use eframe::egui::{self, Align2, CornerRadius, Key, Rect, Sense, vec2};

/// Rows shown at once.
const SHOWN: usize = 9;
const ROW: f32 = 38.0;
const WIDTH: f32 = 560.0;

/// The palette while it's open: what's typed and which row is picked.
#[derive(Default)]
pub struct Palette {
    pub query: String,
    pub picked: usize,
    /// The artists listened to lately, worked out once when the palette opens
    /// (from the whole history: too much to do every frame).
    pub listened: Vec<(String, String)>,
}

/// A thing the palette can do.
pub struct Command {
    pub label: String,
    /// What kind of thing it is: "Page", "Playing", "Playlist", ...
    pub group: &'static str,
    pub keys: Option<String>,
    make: Box<dyn Fn() -> Vec<Action>>,
}

impl Command {
    fn new(group: &'static str, label: impl Into<String>, make: impl Fn() -> Vec<Action> + 'static) -> Self {
        Command { label: label.into(), group, keys: None, make: Box::new(make) }
    }

    fn keys(mut self, keys: &str) -> Self {
        self.keys = Some(keys.to_string());
        self
    }

    pub fn actions(&self) -> Vec<Action> {
        (self.make)()
    }
}

/// How well `label` matches `query`: every word typed must be in it; words that
/// start the label or one of its words count more, whole words more still. None when it doesn't match.
pub fn score(label: &str, query: &str) -> Option<u32> {
    let label = label.to_lowercase();
    let mut total = 0;
    for word in query.to_lowercase().split_whitespace() {
        let at = label.find(word)?;
        let starts_word = at == 0 || !label[..at].chars().next_back().is_some_and(char::is_alphanumeric);
        let whole_word = !label[at + word.len()..].chars().next().is_some_and(char::is_alphanumeric);
        total += match (at, starts_word) {
            (0, _) => 4,
            (_, true) => 2,
            _ => 1,
        } + u32::from(starts_word && whole_word);
    }
    Some(total)
}

/// The commands matching `query`, best first (all, in their order, when it's empty).
pub fn ranked<'a>(commands: &'a [Command], query: &str) -> Vec<&'a Command> {
    if query.trim().is_empty() {
        return commands.iter().collect();
    }
    let mut hits: Vec<(u32, usize, &Command)> =
        commands.iter().enumerate().filter_map(|(i, c)| score(&c.label, query).map(|s| (s, i, c))).collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.label.len().cmp(&b.2.label.len())).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, _, c)| c).collect()
}

impl App {
    /// ⌘K opens the palette, or closes it.
    pub(in crate::ui) fn palette_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&shortcuts::PALETTE)) {
            self.palette = match self.palette {
                Some(_) => None,
                None => Some(self.new_palette()),
            };
        }
    }

    /// The palette, opened.
    pub(in crate::ui) fn new_palette(&self) -> Palette {
        Palette { listened: self.palette_artists(), ..Default::default() }
    }

    /// The artists the palette offers: those listened to lately.
    fn palette_artists(&self) -> Vec<(String, String)> {
        let Some(source) = self.client.as_ref().map(|c| c.data_key()).filter(|_| self.prefs.history_on) else { return Vec::new() };
        self.listened_artists(&source, crate::clock::unix_now())
    }

    /// Everything the palette offers now, in the order shown before anything is
    /// typed; `listened`: the artists listened to lately.
    pub(in crate::ui) fn commands(&self, st: &Status, listened: &[(String, String)]) -> Vec<Command> {
        let features = self.features();
        let mut all = Vec::new();

        // The track playing.
        if let Some(track) = st.track.as_ref().filter(|t| !t.is_episode()) {
            all.extend(playing_commands(self, track, features.track_mix));
        }
        // Pages: Home, the ones turned on, the Crate.
        all.push(Command::new("Page", "Home", || vec![Action::Open(View::Home)]).keys(&shortcuts::label(&shortcuts::page(Key::Num1))));
        for page in self.pages_on() {
            let command = Command::new("Page", page.label(), move || vec![Action::Open(page.view())]);
            all.push(match page.key_code() {
                Some(key) => command.keys(&shortcuts::label(&shortcuts::page(key))),
                None => command,
            });
        }
        all.push(Command::new("Page", "Crate", || vec![Action::Open(View::Crate)]));
        all.push(Command::new("Page", "Search", || vec![Action::OpenSearch]).keys(&shortcuts::label(&shortcuts::SEARCH)));
        all.push(Command::new("Page", "Lyrics", || vec![Action::ToggleLyrics]));
        all.push(Command::new("Window", "Settings", || vec![Action::ShowSettings(true)]).keys(&shortcuts::label(&shortcuts::SETTINGS)));
        all.push(Command::new("Window", "Mini player", || vec![Action::ToggleMiniPlayer]));
        all.push(Command::new("Window", "Show the queue", || vec![Action::ShowPanel(Some(SidePanel::Queue))]));
        all.push(Command::new("Window", "Show the play history", || vec![Action::ShowPanel(Some(SidePanel::History))]));
        all.push(Command::new("Window", "Import a playlist…", || vec![Action::ImportPlaylist]));

        // The player.
        let paused = st.state != crate::player::State::Playing;
        all.push(Command::new("Player", if paused { "Play" } else { "Pause" }, || vec![Action::Player(Cmd::Toggle)]).keys("Space"));
        all.push(Command::new("Player", "Next track", || vec![Action::Player(Cmd::Next)]));
        all.push(Command::new("Player", "Previous track", || vec![Action::Player(Cmd::Prev)]));
        let shuffle = self.prefs.shuffle;
        all.push(Command::new("Player", if shuffle { "Shuffle off" } else { "Shuffle on" }, move || vec![Action::Shuffle(!shuffle)]));
        for (label, repeat) in [("Repeat all", Repeat::All), ("Repeat one", Repeat::One), ("Repeat off", Repeat::Off)] {
            if self.prefs.repeat != repeat {
                all.push(Command::new("Player", label, move || vec![Action::Repeat(repeat)]));
            }
        }
        for minutes in [15, 30, 45, 60] {
            all.push(Command::new("Player", format!("Sleep in {minutes} minutes"), move || {
                vec![Action::Sleep(Some(SleepChoice::Minutes(minutes)))]
            }));
        }
        for (what, choice) in [("track", SleepChoice::EndOfTrack), ("album", SleepChoice::EndOfAlbum), ("queue", SleepChoice::EndOfQueue)] {
            all.push(Command::new("Player", format!("Sleep at the end of the {what}"), move || vec![Action::Sleep(Some(choice))]));
        }
        if st.sleep.is_some() {
            all.push(Command::new("Player", "Sleep timer off", || vec![Action::Sleep(None)]));
        }

        // Flow (Radio) and its moods and genres.
        let endless = if features.flow { "Flow" } else { "Radio" };
        all.push(Command::new("Flow", format!("Play {endless}"), || vec![Action::Player(Cmd::PlayFlow(None))]));
        for item in self.library.home.ready().into_iter().flatten().flat_map(|s| &s.items) {
            if matches!(item.kind, ItemKind::Flow { .. } | ItemKind::Radio { .. })
                && let Some(tuning) = item.kind.tuning()
            {
                let tuning = tuning.to_string();
                all.push(Command::new("Flow", format!("Play {endless} · {}", item.title), move || {
                    vec![Action::Player(Cmd::PlayFlow(Some(tuning.clone())))]
                }));
            }
        }
        for which in crate::ui::stats::Smart::ALL {
            all.push(Command::new("List", which.title(), move || vec![Action::OpenCollection(Coll::history(which))]));
        }
        if self.crate_playable() > 0 {
            all.push(Command::new("Crate", "Play the new ones in your Crate", || vec![Action::PlayCrate]));
        }

        // Playlists, and artists listened to lately.
        for playlist in &self.library.playlists {
            let coll = Coll::from_playlist(playlist);
            all.push(Command::new("Playlist", playlist.title.clone(), move || vec![Action::OpenCollection(coll.clone())]));
        }
        for (id, name) in listened {
            let artist = ArtistRef { id: id.clone(), name: name.clone() };
            all.push(Command::new("Artist", name, move || vec![Action::OpenArtist(artist.clone())]));
        }
        // Nothing that reaches a part turned off in Settings.
        all.retain(|command| command.actions().iter().all(|a| self.allows(a)));
        all
    }

    /// The palette, over everything, while it's open.
    pub(in crate::ui) fn palette_window(&mut self, ctx: &egui::Context, st: &Status, acts: &mut Actions) {
        let Some(mut palette) = self.palette.take() else { return };
        let commands = self.commands(st, &palette.listened);
        let query = palette.query.trim().to_string();
        let mut hits = ranked(&commands, &query);
        if query.is_empty() {
            hits.truncate(SHOWN * 2);
        }
        let search = (!query.is_empty()).then(|| {
            Command::new("Search", format!("Search for “{query}”"), {
                let query = query.clone();
                move || vec![Action::SearchFor(query.clone())]
            })
        });
        hits.extend(search.as_ref());
        // ↑ ↓ pick, Enter does it, Esc closes.
        let (up, down, enter, escape) = ctx
            .input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if down {
            palette.picked = (palette.picked + 1).min(hits.len().saturating_sub(1));
        }
        if up {
            palette.picked = palette.picked.saturating_sub(1);
        }
        palette.picked = palette.picked.min(hits.len().saturating_sub(1));
        let mut chosen = enter.then_some(palette.picked).filter(|_| !hits.is_empty());
        let p = colors();
        let modal = egui::Modal::new(egui::Id::unique("command-palette")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(WIDTH);
            let before = palette.query.clone();
            let field = egui::TextEdit::singleline(&mut palette.query)
                .hint_text("Go to a page, playlist or artist, or do something…")
                .font(ty::LEAD.font())
                .frame(egui::Frame::new())
                .desired_width(f32::INFINITY);
            ui.add(field).request_focus();
            if palette.query != before {
                palette.picked = 0;
            }
            ui.add_space(6.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, p.line));
            ui.add_space(6.0);
            if hits.is_empty() {
                ui.label(egui::RichText::new("Type to find a page, a playlist or something to do").color(p.faint).font(ty::SMALL.font()));
                return;
            }
            egui::ScrollArea::vertical().max_height(ROW * SHOWN as f32).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, command) in hits.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    let picked = i == palette.picked;
                    if picked || resp.hovered() {
                        let fill = if picked { p.raised } else { p.hover };
                        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), fill);
                    }
                    if picked && (up || down) {
                        ui.scroll_to_rect(rect, None);
                    }
                    let group_width = 76.0;
                    text_left(
                        ui.painter(),
                        egui::pos2(rect.left() + 10.0, rect.center().y),
                        command.group,
                        ty::OVERLINE,
                        p.faint,
                        group_width,
                    );
                    let keys_width = command.keys.as_ref().map_or(0.0, |k| {
                        let width = ui.painter().layout_no_wrap(k.clone(), ty::META.font(), p.faint).size().x;
                        ui.painter().text(
                            egui::pos2(rect.right() - 10.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            k,
                            ty::META.font(),
                            p.faint,
                        );
                        width + 16.0
                    });
                    let x = rect.left() + 10.0 + group_width + 8.0;
                    let color = if picked { p.text } else { p.dim };
                    let width = rect.right() - x - keys_width - 10.0;
                    text_left(ui.painter(), egui::pos2(x, rect.center().y), &command.label, ty::ITEM, color, width);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        chosen = Some(i);
                    }
                }
            });
            let hint_rect = Rect::from_min_size(ui.cursor().min + vec2(0.0, 6.0), vec2(ui.available_width(), 16.0));
            ui.allocate_rect(hint_rect, Sense::hover());
            let hint = "↑ ↓ to pick · Enter to do it · Esc to close";
            text_left(
                ui.painter(),
                egui::pos2(hint_rect.left() + 10.0, hint_rect.center().y),
                hint,
                ty::CAPTION,
                p.faint,
                hint_rect.width(),
            );
        });
        if let Some(i) = chosen {
            if let Some(command) = hits.get(i) {
                acts.extend(command.actions());
            }
            return;
        }
        if escape || modal.should_close() {
            return;
        }
        self.palette = Some(palette);
    }
}

/// Like, add to a playlist, save to the Crate, mix, go to its artist or album.
fn playing_commands(app: &App, track: &Track, mix: bool) -> Vec<Command> {
    let mut all = Vec::new();
    let what = Likeable::Track(track.id.clone());
    let liked = app.library.likes.likes.contains(&what);
    let label = if liked { format!("Unlike “{}”", track.title) } else { format!("Like “{}”", track.title) };
    all.push(Command::new("Playing", label, move || vec![Action::ToggleLike(what.clone())]));
    let t = track.clone();
    all.push(
        Command::new("Playing", format!("Save “{}” to the Crate", track.title), move || {
            vec![Action::SaveToCrate(Thing::Track(t.clone()), "Command".into())]
        })
        .keys(&shortcuts::label(&shortcuts::SAVE_TO_CRATE)),
    );
    if mix {
        let t = track.clone();
        all.push(Command::new("Playing", format!("Start a mix of “{}”", track.title), move || {
            vec![Action::TrackMenu(vec![t.clone()], TrackMenu::Mix)]
        }));
    }
    if let Some(artist) = track.artists.first().cloned() {
        all.push(Command::new("Playing", format!("Go to {}", artist.name), move || vec![Action::OpenArtist(artist.clone())]));
    }
    if let Some(album) = Coll::origin_of(track) {
        all.push(Command::new("Playing", format!("Go to the album {}", track.album), move || vec![Action::OpenCollection(album.clone())]));
    }
    for playlist in app.library.playlists.iter().filter(|p| p.owned) {
        let (t, pl) = (track.clone(), playlist.clone());
        all.push(Command::new("Playing", format!("Add to {}", playlist.title), move || {
            vec![Action::TrackMenu(vec![t.clone()], TrackMenu::AddTo(pl.clone()))]
        }));
    }
    let t = track.clone();
    all.push(Command::new("Playing", "Add to a new playlist…", move || vec![Action::TrackMenu(vec![t.clone()], TrackMenu::NewPlaylist)]));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(labels: &[&str]) -> Vec<Command> {
        labels.iter().map(|l| Command::new("Page", *l, Vec::new)).collect()
    }

    #[test]
    fn every_word_must_match() {
        assert!(score("Add to Night drive", "add night").is_some());
        assert!(score("Add to Night drive", "add jazz").is_none());
        assert!(score("Play Flow · Chill", "chill").is_some());
    }

    #[test]
    fn starts_count_more() {
        let commands = named(&["Go to the album Heaven", "Playlists", "Play Flow", "Display"]);
        let order: Vec<&str> = ranked(&commands, "play").iter().map(|c| c.label.as_str()).collect();
        assert_eq!(order, ["Play Flow", "Playlists", "Display"], "a whole word at the start first; inside a word last");
        assert_eq!(ranked(&commands, "").len(), 4, "everything before typing");
        assert_eq!(ranked(&commands, "album")[0].label, "Go to the album Heaven");
    }
}
