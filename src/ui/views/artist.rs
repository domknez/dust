//! Artist page: header with Play/Shuffle, the most popular tracks, then card rows
//! for the discography (albums, singles & EPs, live & compilations), playlists
//! featuring the artist and related artists.

use super::collection::{Columns, Picked, track_row};
use super::home::card_row;
use crate::deezer::{Track, image_url, thousands};
use crate::player::{Cmd, State, Status};
use crate::ui::app::App;
use crate::ui::state::{ArtistView, Coll, Source};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Rect, RichText, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

/// Popular tracks shown before "Show all".
const POPULAR: usize = 10;
const HEADER: f32 = 236.0;

impl App {
    pub(in crate::ui) fn artist_view(&mut self, ui: &mut Ui, st: &Status, artist: &ArtistView) {
        let ctx = ui.ctx().clone();
        let mut picked = Picked::default();
        let mut show_all = false;
        let now = (st.track.as_ref().map(|t| t.id), st.state == State::Playing);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            let (head, _) = ui.allocate_exact_size(vec2(width, HEADER), Sense::hover());
            self.artist_header(ui, head, artist, &mut picked);

            let Some(page) = &self.artist_page else {
                ui.add_space(24.0);
                if self.artist_task.is_some() {
                    ui.add(egui::Spinner::new().size(22.0));
                } else if let Some(e) = &self.list_error {
                    ui.label(RichText::new(e).color(colors().danger));
                }
                return;
            };

            if !page.top.is_empty() {
                section_title(ui, "Popular");
                let columns = Columns::new(ui.cursor().left(), width);
                for (i, track) in page.top.iter().take(POPULAR).enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(width, metrics::TRACK_ROW), Sense::hover());
                    track_row(ui, &mut self.covers, track, i, rect, &columns, now, &mut picked);
                }
                if page.top.len() > POPULAR {
                    ui.add_space(6.0);
                    let label = format!("Show all {} popular tracks", page.top.len());
                    show_all = ui.add(egui::Button::new(RichText::new(label).color(colors().dim)).frame(false)).clicked();
                }
            }
            for (i, section) in page.sections.iter().enumerate() {
                card_row(ui, &mut self.covers, ("artist-row", i), &section.title, &section.items, st, &mut picked.cards);
            }
            ui.add_space(28.0);
        });

        self.apply_card_picks(&ctx, picked.cards);
        let top: &[Track] = self.artist_page.as_ref().map_or(&[], |p| &p.top);
        if let Some((i, shuffle)) = picked.play
            && !top.is_empty()
        {
            let mut queue = top.to_vec();
            if shuffle {
                fastrand::shuffle(&mut queue);
            }
            self.player.send(Cmd::Play(queue, i));
        } else if let Some((i, mode)) = picked.queue
            && let Some(track) = top.get(i)
        {
            self.player.send(mode.command(vec![track.clone()]));
        }
        if picked.toggle {
            self.player.send(Cmd::Toggle);
        }
        if show_all {
            let picture = artist.picture.clone().map(|md5| ("artist".to_string(), md5));
            let coll = Coll {
                source: Source::TopTracks(artist.id.clone()),
                kind: "POPULAR TRACKS",
                title: artist.name.clone(),
                subtitle: String::new(),
                picture,
            };
            self.open_collection(&ctx, coll);
        }
        if let Some(album) = picked.album {
            self.open_collection(&ctx, album);
        }
        if let Some(other) = picked.artist {
            self.open_artist(&ctx, ArtistView::from_ref(&other));
        }
    }

    fn artist_header(&mut self, ui: &mut Ui, head: Rect, artist: &ArtistView, picked: &mut Picked) {
        let p = colors();
        let page = self.artist_page.as_ref();
        let picture = page.and_then(|pg| pg.picture.clone()).or_else(|| artist.picture.clone());
        let url = picture.map(|md5| image_url("artist", &md5, metrics::HEADER_PX));
        let art = Rect::from_min_size(head.min + vec2(0.0, 8.0), Vec2::splat(metrics::HEADER_ART));
        widgets::cover(ui, &mut self.covers, url.as_deref(), art, (metrics::HEADER_ART / 2.0) as u8);

        let x = art.right() + 28.0;
        let width = head.right() - x;
        let name = page.map_or(artist.name.as_str(), |pg| pg.name.as_str());
        text_left(ui.painter(), pos2(x, head.top() + 48.0), "ARTIST", ty::CAPTION.strong(), p.dim, width);
        text_left(ui.painter(), pos2(x, head.top() + 90.0), name, ty::HERO, p.text, width);
        if let Some(pg) = page {
            let releases: usize = pg.sections.iter().filter(|s| s.items.iter().all(|i| i.kind == "album")).map(|s| s.items.len()).sum();
            let meta = format!("{} fans  ·  {releases} releases", thousands(pg.fans));
            text_left(ui.painter(), pos2(x, head.top() + 132.0), &meta, ty::ITEM, p.dim, width);
        }
        let has_top = page.is_some_and(|pg| !pg.top.is_empty());
        let buttons = Rect::from_min_size(pos2(x, head.top() + 160.0), vec2(width, 44.0));
        ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if has_top && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                picked.play = Some((0, false));
            }
            if has_top && widgets::pill(ui, "Shuffle", Some(Icon::Shuffle), false).clicked() {
                picked.play = Some((0, true));
            }
        });
    }
}

fn section_title(ui: &mut Ui, title: &str) {
    ui.add_space(18.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    text_left(ui.painter(), pos2(r.left(), r.center().y), title, ty::SECTION_TITLE, colors().text, r.width());
    ui.add_space(6.0);
}
