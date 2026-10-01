//! Home: the Flow mood row first, then Deezer's recommendation sections as
//! horizontally scrolling rows of cards.

use crate::deezer::{Item, Section};
use crate::player::{Cmd, Status};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::state::{Coll, PlayMode, Source, View};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, CardClick, text_left};
use eframe::egui::{self, RichText, Sense, Ui, pos2, vec2};

/// What the listener picked in a row of cards this frame.
#[derive(Default)]
pub(in crate::ui) struct CardPicks {
    open: Option<Coll>,
    play: Option<(Source, PlayMode)>,
    flow: Option<Option<String>>,
}

/// Height [`card_row`] takes: spacing, section title and one row of cards.
pub(in crate::ui) const CARD_ROW_HEIGHT: f32 = 22.0 + 28.0 + 8.0 + metrics::HOME_CARD + metrics::CARD_TEXT;

impl App {
    pub(in crate::ui) fn home_page(&mut self, ui: &mut Ui, st: &Status) {
        let ctx = ui.ctx().clone();
        let mut picked = CardPicks::default();
        let mut reload = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(96.0, 44.0), Sense::hover());
                text_left(ui.painter(), pos2(r.left(), r.center().y), "Home", ty::PAGE_TITLE, colors().text, r.width());
                reload = widgets::icon_button(ui, Icon::Refresh, 16.0, colors().dim).on_hover_text("Refresh recommendations").clicked();
            });
            if self.home.is_empty() {
                if self.home_task.is_some() {
                    ui.add_space(24.0);
                    ui.add(egui::Spinner::new().size(22.0));
                } else if let Some(e) = &self.list_error {
                    ui.label(RichText::new(e).color(colors().danger));
                }
                return;
            }
            for i in flow_first(&self.home) {
                let section = &self.home[i];
                card_row(ui, &mut self.covers, ("home-row", i), &section.title, &section.items, st, &mut picked);
            }
            ui.add_space(24.0);
        });

        if reload {
            self.home.clear();
            self.open_view(&ctx, View::Home);
        }
        self.apply_card_picks(&ctx, picked);
    }

    /// Act on what was picked in [`card_row`]s.
    pub(in crate::ui) fn apply_card_picks(&mut self, ctx: &egui::Context, picked: CardPicks) {
        if let Some(mood) = picked.flow {
            self.player.send(Cmd::PlayFlow(mood));
        }
        if let Some((source, mode)) = picked.play {
            self.play_source(ctx, source, mode);
        }
        if let Some(coll) = picked.open {
            self.open_collection(ctx, coll);
        }
    }
}

/// A titled, horizontally scrolling row of cards (Flow moods or collections).
/// Draws nothing when none of `items` can be shown.
pub(in crate::ui) fn card_row(
    ui: &mut Ui,
    covers: &mut Covers,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    items: &[Item],
    st: &Status,
    picked: &mut CardPicks,
) {
    let is_flow = items.iter().all(|it| it.kind == "flow");
    let items: Vec<&Item> = items.iter().filter(|it| it.kind == "flow" || Coll::from_item(it).is_some()).collect();
    if items.is_empty() {
        return;
    }
    ui.add_space(22.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    text_left(ui.painter(), pos2(r.left(), r.center().y), title, ty::SECTION_TITLE, colors().text, r.width());
    ui.add_space(8.0);
    egui::ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        // Bar visibility animation on these rows never settled and kept the UI
        // redrawing while idle; trackpad and shift-scroll still work.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = if is_flow { 10.0 } else { 18.0 };
                for item in items {
                    if is_flow {
                        let active = st.flow.as_deref() == Some(item.id.as_str());
                        if let Some(mood) = widgets::flow_card(ui, covers, item, active) {
                            picked.flow = Some(mood);
                        }
                    } else if let Some(coll) = Coll::from_item(item) {
                        match widgets::card(ui, covers, &coll) {
                            CardClick::Open => picked.open = Some(coll),
                            CardClick::Play(mode) => picked.play = Some((coll.source.clone(), mode)),
                            CardClick::None => {}
                        }
                    }
                }
            });
        });
}

/// Section order with the Flow section first, as on Deezer.
fn flow_first(sections: &[Section]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sections.len()).collect();
    order.sort_by_key(|&i| !sections[i].items.iter().any(|it| it.kind == "flow"));
    order
}
