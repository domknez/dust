//! The Crate page: everything in it, filtered, one entry in detail.

use super::*;

impl App {
    pub(in crate::ui) fn crate_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let head = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover()).0;
        text_left(ui.painter(), head.min + vec2(0.0, 18.0), "Crate", ty::PAGE_TITLE, p.text, head.width());
        let line = "Things to check out later, from any source and from anyone.";
        text_left(ui.painter(), head.min + vec2(0.0, 50.0), line, ty::LEAD, p.dim, head.width());
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.set_max_width(680.0);
            let typed = !self.crates.view.jot.trim().is_empty();
            let playable = self.crate_playable();
            // Room for the button beside the field (Save while typing, else Play), a
            // shorter Play when the field would get too narrow.
            let mut play = format!("Play the new ones · {playable}");
            if ui.available_width() - widgets::pill_width(ui, &play, true, true) < 280.0 {
                play = format!("Play new · {playable}");
            }
            let button = if typed {
                widgets::pill_width(ui, "Save", false, true)
            } else if playable > 0 {
                widgets::pill_width(ui, &play, true, true)
            } else {
                0.0
            };
            let jot = &mut self.crates.view.jot;
            let field = ui.available_width() - button - ui.spacing().item_spacing.x;
            let mut save = jot_field_sized(ui, "crate-page-jot", jot, field);
            if typed && widgets::pill(ui, "Save", None, true).clicked() {
                save = Some(std::mem::take(jot));
            }
            if !typed && playable > 0 {
                let pill = widgets::pill(ui, &play, Some(Icon::Play), true);
                if pill.on_hover_text(format!("The new tracks; {EACH} from each new album and artist")).clicked() {
                    acts.push(Action::PlayCrate);
                }
            }
            if let Some(note) = save {
                acts.push(Action::CrateNote(note));
            }
        });
        ui.add_space(14.0);
        self.filter_chips(ui);
        ui.add_space(12.0);

        let filter = self.crates.view.filter;
        let shown: Vec<Entry> = self.crates.store.entries().iter().filter(|e| filter.keeps(e)).cloned().collect();
        if self.crates.view.selected.as_ref().is_none_or(|id| !shown.iter().any(|e| e.id == *id)) {
            self.crates.view.selected = shown.first().map(|e| e.id.clone());
        }
        if shown.is_empty() {
            ui.add_space(30.0);
            let (title, hint) = if self.crates.store.entries().is_empty() {
                (
                    "Your Crate is empty",
                    format!(
                        "Save what's playing with {}, or right-click a track → Save to Crate",
                        shortcuts::label(&shortcuts::SAVE_TO_CRATE)
                    ),
                )
            } else {
                ("Nothing here", "Try another filter".to_string())
            };
            widgets::empty_state(ui, title, &hint);
            return;
        }
        let area = ui.available_rect_before_wrap();
        let current = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        // Too narrow for both: the list, or the entry picked with a way back.
        if one_column(area.width()) {
            let entry = self.crates.view.selected.as_ref().and_then(|id| self.crates.store.get(id)).cloned();
            match entry.filter(|_| self.crates.view.open) {
                Some(entry) => {
                    egui::ScrollArea::vertical().id_salt("crate-detail").auto_shrink([false, false]).show(ui, |ui| {
                        if ui.link(RichText::new("‹ Back to the list").font(ty::SMALL.font())).clicked() {
                            self.crates.view.open = false;
                        }
                        ui.add_space(12.0);
                        self.entry_detail(ui, &entry, current, acts);
                    });
                }
                None => {
                    ui.scope_builder(UiBuilder::new().max_rect(area), |ui| self.crate_list(ui, &shown, current, acts));
                }
            }
            ui.allocate_rect(area, Sense::hover());
            return;
        }
        let detail_width = DETAIL_WIDTH.min(area.width() * 0.45);
        let list = Rect::from_min_max(area.min, pos2(area.right() - detail_width - 24.0, area.bottom()));
        let detail = Rect::from_min_max(pos2(list.right() + 24.0, area.top()), area.max);
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| self.crate_list(ui, &shown, current, acts));
        let entry = self.crates.view.selected.as_ref().and_then(|id| self.crates.store.get(id)).cloned();
        if let Some(entry) = entry {
            ui.scope_builder(UiBuilder::new().max_rect(detail), |ui| {
                egui::ScrollArea::vertical().id_salt("crate-detail").auto_shrink([false, false]).show(ui, |ui| {
                    self.entry_detail(ui, &entry, current, acts);
                });
            });
        }
        ui.allocate_rect(area, Sense::hover());
    }

    /// The page's list of entries; a click picks one to show in full.
    fn crate_list(&mut self, ui: &mut Ui, shown: &[Entry], current: &str, acts: &mut Actions) {
        let p = colors();
        egui::ScrollArea::vertical().id_salt("crate-list").auto_shrink([false, false]).show(ui, |ui| {
            for entry in shown {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW + 6.0), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let selected = self.crates.view.selected.as_ref() == Some(&entry.id);
                if selected || resp.hovered() {
                    let fill = if selected { p.raised } else { p.hover };
                    ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), fill);
                }
                let status = Rect::from_min_max(pos2(rect.right() - 90.0, rect.top()), rect.max);
                entry_row(ui, &mut self.covers, rect.with_max_x(status.left()), entry, current);
                status_line(ui, status, entry);
                acts.extend(entry_menu(&resp, entry));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.crates.view.selected = Some(entry.id.clone());
                    self.crates.view.open = true;
                }
            }
        });
    }

    /// All · New · Heard, then the kinds there are, each with its count.
    fn filter_chips(&mut self, ui: &mut Ui) {
        let entries = self.crates.store.entries();
        let count = |f: Filter| entries.iter().filter(|e| f.keeps(e)).count();
        let mut chips = vec![("All", Filter::All), ("New", Filter::New), ("Heard", Filter::Heard)];
        for (label, kind) in [("Tracks", "track"), ("Albums", "album"), ("Artists", "artist"), ("Notes", "note")] {
            if count(Filter::Kind(kind)) > 0 {
                chips.push((label, Filter::Kind(kind)));
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (label, filter) in chips {
                let on = self.crates.view.filter == filter;
                if widgets::chip(ui, &format!("{label}  {}", count(filter)), on).clicked() {
                    self.crates.view.filter = filter;
                    self.crates.view.open = false;
                }
            }
        });
    }

    /// One entry in full: what it is, how to get to it, its note, where it came from
    /// and when it was saved, and marking it heard or removing it.
    fn entry_detail(&mut self, ui: &mut Ui, entry: &Entry, current: &str, acts: &mut Actions) {
        let p = colors();
        let here = entry.source.is_empty() || entry.source == current;
        let mut kind = match &entry.thing {
            Thing::Album { about, .. } if !about.kind.is_empty() => about.kind.to_uppercase(),
            thing => thing.kind().to_uppercase(),
        };
        if !here {
            kind = format!("{kind} · ON {}", source_name(&entry.source).to_uppercase());
        }
        let picture = entry.picture().cloned();
        if !matches!(entry.thing, Thing::Note) {
            let size = 120.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            let corner = if matches!(entry.thing, Thing::Artist { .. }) { (size / 2.0) as u8 } else { radius::CARD };
            widgets::cover(ui, &mut self.covers, picture.as_ref(), metrics::HOME_CARD_PX, rect, corner);
            ui.add_space(12.0);
        }
        ui.label(RichText::new(kind).font(ty::OVERLINE.font()).color(p.dim));
        ui.add_space(2.0);
        let title = if entry.thing == Thing::Note { format!("“{}”", entry.title()) } else { entry.title().to_string() };
        ui.add(egui::Label::new(RichText::new(title).font(ty::SECTION_TITLE.font()).color(p.text)).wrap());
        if !entry.by().is_empty() {
            ui.label(RichText::new(entry.by()).font(ty::SECONDARY.font()).color(p.dim));
        }
        if let Thing::Album { about, .. } = &entry.thing {
            let tracks = About { kind: String::new(), ..about.clone() }.summary();
            let line = [about.year.as_str(), tracks.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            if !line.is_empty() {
                ui.label(RichText::new(line).font(ty::SMALL.font()).color(p.faint));
            }
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match (&entry.thing, here) {
                (Thing::Track(track), true) if track.available => {
                    if widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                        acts.push(Action::PlayNow(track.clone()));
                    }
                    self.crate_add_to_playlist(ui, track, acts);
                }
                (Thing::Album { id, title, artist, picture, .. }, true) => {
                    if widgets::pill(ui, "Open album", None, true).clicked() {
                        acts.push(Action::OpenCollection(Coll {
                            source: CollSource::Album(id.clone()),
                            kind: "ALBUM",
                            title: title.clone(),
                            subtitle: artist.clone(),
                            picture: picture.clone(),
                        }));
                    }
                }
                (Thing::Artist { id, name, picture }, true) => {
                    if widgets::pill(ui, "Open artist", None, true).clicked() {
                        let artist = ArtistView { id: id.clone(), name: name.clone(), picture: picture.clone() };
                        acts.push(Action::Open(View::Artist(Box::new(artist))));
                    }
                }
                _ => {
                    let query = match &entry.thing {
                        Thing::Note => entry.note.clone(),
                        _ if entry.by().is_empty() => entry.title().to_string(),
                        _ => format!("{} {}", entry.by(), entry.title()),
                    };
                    let hint = format!("Search {} for it", self.client.as_ref().map_or("", |c| c.id().name));
                    if widgets::pill(ui, "Search for it", Some(Icon::Search), true).on_hover_text(hint).clicked() {
                        acts.push(Action::SearchFor(query));
                    }
                }
            }
        });
        if let Thing::Track(track) = &entry.thing
            && here
            && !track.is_episode()
        {
            let what = Likeable::Track(track.id.clone());
            let liked = self.library.likes.likes.contains(&what);
            ui.add_space(6.0);
            let label = if liked { "In Loved tracks" } else { "Add to Loved tracks" };
            if ui.link(RichText::new(label).font(ty::SMALL.font())).clicked() {
                acts.push(Action::ToggleLike(what));
            }
        }

        ui.add_space(18.0);
        if entry.thing != Thing::Note {
            self.note_field(ui, entry, acts);
            ui.add_space(14.0);
        }
        let zone = when::zone();
        let saved = format!(
            "{}, {}",
            when::day(entry.saved, &zone).map_or_else(String::new, |d| when::day_name(d, when::today(&zone))),
            when::clock(entry.saved, &zone)
        );
        for (label, value) in [("From", entry.from.as_str()), ("Saved", saved.as_str())] {
            if value.is_empty() {
                continue;
            }
            ui.label(RichText::new(label.to_uppercase()).font(ty::OVERLINE.font()).color(p.faint));
            ui.label(RichText::new(value).font(ty::BODY.font()).color(p.text));
            ui.add_space(10.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (label, heard) = if entry.heard { ("Mark as new", false) } else { ("Mark as heard", true) };
            if widgets::pill(ui, label, Some(Icon::Check), false).clicked() {
                acts.push(Action::Crate(Change::Heard { id: entry.id.clone(), heard }));
            }
            if ui.link(RichText::new("Remove").font(ty::SMALL.font()).color(p.danger)).clicked() {
                acts.push(Action::Crate(Change::Remove { id: entry.id.clone() }));
            }
        });
    }

    /// The entry's note, saved when the field is left.
    fn note_field(&mut self, ui: &mut Ui, entry: &Entry, acts: &mut Actions) {
        let p = colors();
        ui.label(RichText::new("NOTE").font(ty::OVERLINE.font()).color(p.faint));
        let note = &mut self.crates.view.note;
        if note.as_ref().is_none_or(|(id, _)| *id != entry.id) {
            // Another entry was picked while its note was being typed: keep it.
            if let Some((id, text)) = note.take()
                && self.crates.store.get(&id).is_some_and(|e| e.note != text.trim())
            {
                acts.push(Action::Crate(Change::Note { id, note: text.trim().to_string() }));
            }
            *note = Some((entry.id.clone(), entry.note.clone()));
        }
        let Some((_, text)) = note.as_mut() else { return };
        let field = egui::TextEdit::multiline(text)
            .hint_text("Who told you, what to start with…")
            .font(ty::BODY.font())
            .desired_rows(3)
            .desired_width(f32::INFINITY);
        let resp = ui.add(field);
        if resp.lost_focus() && text.trim() != entry.note {
            acts.push(Action::Crate(Change::Note { id: entry.id.clone(), note: text.trim().to_string() }));
        }
    }

    /// "Add to playlist" with the playlist picker, for a track in the Crate.
    fn crate_add_to_playlist(&self, ui: &mut Ui, track: &crate::model::Track, acts: &mut Actions) {
        let filter = ui.scope_id().with("crate-playlist-filter");
        let resp = widgets::pill(ui, "Add to playlist", Some(Icon::Plus), false);
        if resp.clicked() {
            ui.data_mut(|d| d.remove::<String>(filter));
        }
        let mut pick = None;
        egui::Popup::menu(&resp).width(240.0).align(RectAlign::BOTTOM_START).close_behavior(PopupCloseBehavior::CloseOnClickOutside).show(
            |ui| {
                pick = widgets::playlist_picker(ui, self.playlist_choices(), Some(filter));
                if pick.is_some() {
                    ui.close();
                }
            },
        );
        if let Some(choice) = pick {
            acts.push(Action::TrackMenu(vec![track.clone()], choice));
        }
    }
}

/// Whether a page `width` wide shows the list or one entry, not both side by side.
fn one_column(width: f32) -> bool {
    width < NARROW_PAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_page_shows_the_list_or_one_entry() {
        assert!(one_column(500.0));
        assert!(!one_column(800.0));
    }
}
