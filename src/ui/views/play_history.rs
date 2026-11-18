//! The *history* tab of the panel on the right: the tracks heard on this source,
//! newest first, by day, with the Flow or album they came from; skips marked. Click
//! one to hear it again (it goes in after the track playing), right-click for the
//! track menu, the heart likes it.

use crate::model::Likeable;
use crate::player::Cmd;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::crate_store::Thing;
use crate::ui::format::{mmss, thousands};
use crate::ui::plays::{KEEP, Play, flow_label, when};
use crate::ui::state::PlayMode;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, MenuTarget, Playlists, TrackMenu, text_left};
use eframe::egui::{self, Align2, CornerRadius, RichText, Sense, Ui, pos2, vec2};

/// Plays the panel lists, newest first; the stats count all of them.
const SHOWN: usize = 1000;

/// A line of the history: a day, where a run of plays came from, the end of such a
/// run, or a play (its place among those shown, newest first).
enum Line {
    Day(String),
    From(String),
    Gap,
    Play(usize),
}

/// The panel's lines, worked out again only when the plays, the day or the
/// source change.
#[derive(Default)]
pub struct HistoryView {
    lines: Option<(LinesKey, Vec<Line>)>,
}

#[derive(PartialEq)]
struct LinesKey {
    source: &'static str,
    plays: u64,
    today: jiff::civil::Date,
    radio: bool,
}

impl App {
    pub(in crate::ui) fn history_panel(&mut self, ui: &mut Ui, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        self.side_panel_tabs(ui, acts);
        ui.add_space(4.0);
        let Some(source) = self.client.as_ref().map(|c| c.id().key) else { return };
        let radio = !self.features().flow;
        let zone = when::zone();
        let plays: Vec<&Play> = self.plays.of(source).take(SHOWN).collect();
        let key = LinesKey { source, plays: self.plays.revision(), today: when::today(&zone), radio };
        let cache = &mut self.history_view.lines;
        if cache.as_ref().is_none_or(|(k, _)| *k != key) {
            *cache = Some((key, lines(&plays, &zone, radio)));
        }
        let (key, lines) = cache.take().expect("just made");
        if lines.is_empty() {
            widgets::empty_panel(ui, "Nothing heard yet", "Tracks you listen to show up here");
            self.history_view.lines = Some((key, lines));
            return;
        }
        let playlists = Playlists { all: &self.library.playlists, recent: &self.prefs.recent_playlists };
        let mix = self.features().track_mix;
        let p = colors();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, line) in lines.iter().enumerate() {
                match line {
                    Line::Day(day) => {
                        if i > 0 {
                            ui.add_space(10.0);
                        }
                        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
                        text_left(ui.painter(), pos2(rect.left() + 4.0, rect.center().y), day, ty::TITLE.strong(), p.text, rect.width());
                    }
                    Line::From(from) => widgets::caption(ui, &from.to_uppercase()),
                    Line::Gap => ui.add_space(10.0),
                    &Line::Play(at) => {
                        let play = plays[at];
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::QUEUE_ROW), Sense::click());
                        // Only what's on screen is drawn: the history is long.
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let track = &play.track;
                        let hovered = ui.rect_contains_pointer(rect);
                        if hovered {
                            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
                        }
                        acts.extend(widgets::queue_row(ui, &mut self.covers, rect, track, false, false).map(Action::OpenArtist));
                        let right = rect.right() - 8.0;
                        let liked_now = Likeable::Track(track.id.clone());
                        let liked = self.library.likes.likes.contains(&liked_now);
                        if hovered && !track.is_episode() {
                            let heart = pos2(right - 10.0, rect.center().y);
                            if widgets::heart_at(ui, heart, ("history", i), liked, 14.0).clicked() {
                                acts.push(Action::ToggleLike(liked_now));
                            }
                        } else {
                            let time = when::clock(play.at, &zone);
                            let painter = ui.painter();
                            if play.skipped {
                                painter.text(pos2(right, rect.center().y - 8.0), Align2::RIGHT_CENTER, time, ty::META.font(), p.dim);
                                let skipped = format!("skipped {}", mmss(play.heard_secs as f64));
                                painter.text(
                                    pos2(right, rect.center().y + 9.0),
                                    Align2::RIGHT_CENTER,
                                    skipped,
                                    ty::CAPTION.font(),
                                    p.faint,
                                );
                            } else {
                                painter.text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, time, ty::META.font(), p.dim);
                            }
                        }
                        let target = MenuTarget { count: 1, playable: track.available };
                        match widgets::track_menu(&resp, playlists, false, mix && !track.is_episode(), target) {
                            Some(TrackMenu::Crate) => acts.push(Action::SaveToCrate(Thing::Track(track.clone()), "History".into())),
                            Some(choice) => acts.push(Action::TrackMenu(vec![track.clone()], choice)),
                            None => {}
                        }
                        let over_heart = hovered && resp.hover_pos().is_some_and(|pos| pos.x > right - 24.0);
                        if resp.clicked() && !over_heart && track.available {
                            // In after the track playing, then on to it: the queue
                            // (and endless Flow) carries on afterwards.
                            acts.push(Action::TrackMenu(vec![track.clone()], TrackMenu::Play(PlayMode::Next)));
                            acts.push(Action::Player(Cmd::Next));
                        }
                        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                }
            }
            ui.add_space(14.0);
            let note = format!(
                "The latest {} plays. dust keeps the last {} for the Listening page.",
                thousands(SHOWN as u64),
                thousands(KEEP as u64)
            );
            ui.label(RichText::new(note).font(ty::CAPTION.font()).color(p.faint));
            ui.add_space(10.0);
        });
        self.history_view.lines = Some((key, lines));
    }
}

/// The history as lines: a day heading before each day's plays, and where a run of
/// plays came from (a Flow, or an album heard through) before it.
fn lines(plays: &[&Play], zone: &when::Zone, radio: bool) -> Vec<Line> {
    let today = when::today(zone);
    let mut lines = Vec::new();
    let mut last_day = None;
    let mut last_from: Option<String> = None;
    for (i, play) in plays.iter().enumerate() {
        let day = when::day(play.at, zone);
        if day != last_day {
            lines.push(Line::Day(day.map_or_else(String::new, |d| when::day_name(d, today))));
            last_day = day;
            last_from = None;
        }
        let from = from(play, i.checked_sub(1).and_then(|k| plays.get(k)).copied(), plays.get(i + 1).copied(), radio);
        if from != last_from {
            lines.push(from.clone().map_or(Line::Gap, Line::From));
        }
        last_from = from;
        lines.push(Line::Play(i));
    }
    lines
}

/// Where `play` came from: "Flow · chill", or "Album · Fortitude" when the plays
/// next to it are from the same album. None for a mix of things.
fn from(play: &Play, newer: Option<&Play>, older: Option<&Play>, radio: bool) -> Option<String> {
    if let Some(flow) = &play.flow {
        return Some(flow_label(flow, radio));
    }
    let album = &play.track.album_id;
    let same = |other: Option<&Play>| other.is_some_and(|o| o.flow.is_none() && !album.is_empty() && o.track.album_id == *album);
    (same(newer) || same(older)).then(|| format!("Album · {}", play.track.album))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeezerSong, Origin, Track};
    use crate::player::Flow;

    fn play(id: u64, at: u64, album: &str, flow: Option<&str>) -> Play {
        let track = Track {
            album: format!("Album {album}"),
            album_id: album.into(),
            origin: Origin::Deezer(DeezerSong { token: "t".into(), fallback: None }),
            ..Track::test(id)
        };
        let flow = flow.map(|m| Flow { mood: Some(m.to_string()).filter(|m| !m.is_empty()) });
        Play { source: "deezer".into(), at, heard_secs: 100, skipped: false, flow, track }
    }

    fn shape(plays: &[Play], radio: bool) -> Vec<String> {
        let plays: Vec<&Play> = plays.iter().collect();
        lines(&plays, &when::Zone::UTC, radio)
            .iter()
            .map(|l| match l {
                Line::Day(_) => "day".to_string(),
                Line::From(from) => from.clone(),
                Line::Gap => "gap".to_string(),
                &Line::Play(at) => plays[at].track.id.to_string(),
            })
            .collect()
    }

    #[test]
    fn runs_say_where_they_came_from() {
        let noon = 1_763_294_400; // 2025-11-16 12:00 UTC
        let plays = [
            play(6, noon + 60, "x", Some("chill")),
            play(5, noon + 50, "y", Some("genre-classicrock")),
            play(4, noon + 40, "9", None),
            play(3, noon + 30, "9", None),
            play(2, noon + 20, "8", None),
            play(1, noon - 86_400, "7", Some("")),
        ];
        assert_eq!(
            shape(&plays, false),
            ["day", "Flow · chill", "6", "Flow · classicrock", "5", "Album · Album 9", "4", "3", "gap", "2", "day", "Flow", "1"]
        );
        assert_eq!(shape(&plays[..1], true)[1], "Radio · chill");
    }
}
