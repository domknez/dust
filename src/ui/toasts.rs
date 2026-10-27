//! Toasts: short messages in the corner above the player bar about what happened
//! away from the page on screen — a playlist changed; a like, a quick play, a
//! playlist change or a track that failed. They fade after a few seconds (errors
//! stay longer), stay while the pointer is on them and close with ×.

use super::style::icons::Icon;
use super::style::{colors, radius, typography as ty};
use super::widgets;
use eframe::egui::{self, Align, Align2, CornerRadius, Layout, Margin, Order, Rect, RichText, Stroke, pos2, vec2};
use std::time::{Duration, Instant};

/// How long a message stays up.
const INFO_FOR: Duration = Duration::from_secs(4);
const ERROR_FOR: Duration = Duration::from_secs(7);
/// Fading in, and again out.
const FADE: Duration = Duration::from_millis(250);
/// Most toasts up at once; a new one pushes out the oldest.
const MOST: usize = 3;
const WIDTH: f32 = 340.0;
/// From the edges of the page area.
const INSET: f32 = 16.0;

struct Toast {
    message: String,
    error: bool,
    /// When it was posted (again), or last under the pointer.
    since: Instant,
}

impl Toast {
    fn stays(&self) -> Duration {
        if self.error { ERROR_FOR } else { INFO_FOR }
    }

    /// How visible it is now: 0 to 1, fading in and out.
    fn opacity(&self) -> f32 {
        let age = self.since.elapsed();
        let fade_in = age.as_secs_f32() / FADE.as_secs_f32();
        let fade_out = (self.stays() + FADE).saturating_sub(age).as_secs_f32() / FADE.as_secs_f32();
        fade_in.min(fade_out).clamp(0.0, 1.0)
    }

    fn gone(&self) -> bool {
        self.since.elapsed() >= self.stays() + FADE
    }
}

#[derive(Default)]
pub struct Toasts {
    /// Oldest first.
    list: Vec<Toast>,
    /// The player's error last shown, so it shows once.
    player_error: Option<String>,
}

impl Toasts {
    pub fn info(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.post(ctx, message.into(), false);
    }

    pub fn error(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.post(ctx, message.into(), true);
    }

    /// The same message again restarts its toast rather than adding another.
    fn post(&mut self, ctx: &egui::Context, message: String, error: bool) {
        self.list.retain(|t| t.message != message);
        self.list.push(Toast { message, error, since: Instant::now() });
        if self.list.len() > MOST {
            self.list.remove(0);
        }
        ctx.request_repaint();
    }

    /// The player's error (a track that couldn't play, the network gone): a new one
    /// shows as a toast.
    pub fn player_error(&mut self, ctx: &egui::Context, error: Option<&str>) {
        if error != self.player_error.as_deref() {
            self.player_error = error.map(str::to_string);
            if let Some(e) = error {
                self.error(ctx, e);
            }
        }
    }

    /// The messages up, oldest first.
    #[cfg(test)]
    fn messages(&self) -> Vec<&str> {
        self.list.iter().filter(|t| !t.gone()).map(|t| t.message.as_str()).collect()
    }

    /// Draw the toasts stacked up from the bottom right of `area` (the page,
    /// between the sidebar, the player bar and a side panel).
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) {
        self.list.retain(|t| !t.gone());
        if self.list.is_empty() {
            return;
        }
        let p = colors();
        let mut closed = None;
        egui::Area::new(egui::Id::unique("toasts"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(area.right_bottom() - vec2(INSET, INSET))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 8.0;
                for (i, toast) in self.list.iter_mut().enumerate() {
                    let card = ui.scope(|ui| {
                        ui.set_opacity(toast.opacity());
                        egui::Frame::new()
                            .fill(p.raised)
                            .stroke(Stroke::new(1.0, p.line))
                            .corner_radius(CornerRadius::same(radius::PANEL))
                            .inner_margin(Margin { left: 18, right: 6, top: 6, bottom: 6 })
                            .show(ui, |ui| {
                                ui.set_width(WIDTH - 24.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if widgets::icon_button(ui, Icon::Close, 10.0, p.faint).on_hover_text("Dismiss").clicked() {
                                        closed = Some(i);
                                    }
                                    let text = RichText::new(&toast.message).font(ty::SECONDARY.font()).color(p.text);
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| ui.add(egui::Label::new(text).wrap()));
                                });
                            })
                            .response
                            .rect
                    });
                    // A bar on the left edge: lime for news, red for what went wrong.
                    let rect = card.inner;
                    let bar = Rect::from_min_max(pos2(rect.left() + 6.0, rect.top() + 9.0), pos2(rect.left() + 9.0, rect.bottom() - 9.0));
                    let color = if toast.error { p.danger } else { p.accent };
                    ui.painter().rect_filled(bar, CornerRadius::same(2), color.gamma_multiply(toast.opacity()));
                    // Kept while read.
                    if ui.rect_contains_pointer(rect) && toast.since.elapsed() > FADE {
                        toast.since = Instant::now() - FADE;
                    }
                }
            });
        if let Some(i) = closed {
            self.list.remove(i);
        }
        // Redraw while one fades; otherwise when the next one is due to go.
        let fading = self.list.iter().any(|t| t.since.elapsed() < FADE || t.since.elapsed() > t.stays());
        match self.list.iter().map(|t| t.stays().saturating_sub(t.since.elapsed())).min() {
            _ if fading => ctx.request_repaint(),
            Some(next) => ctx.request_repaint_after(next),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_stack_up_to_three_without_repeats() {
        let ctx = egui::Context::default();
        let mut toasts = Toasts::default();
        assert!(toasts.messages().is_empty());
        toasts.info(&ctx, "Added 2 tracks to Mix");
        toasts.error(&ctx, "Couldn't update your likes: offline");
        toasts.info(&ctx, "Added 2 tracks to Mix");
        assert_eq!(toasts.messages(), ["Couldn't update your likes: offline", "Added 2 tracks to Mix"], "the repeat moves to the end");
        toasts.info(&ctx, "Removed 1 track");
        toasts.info(&ctx, "Renamed it");
        assert_eq!(toasts.messages(), ["Added 2 tracks to Mix", "Removed 1 track", "Renamed it"], "the oldest gives way");
    }

    #[test]
    fn errors_stay_longer_and_everything_goes() {
        let ctx = egui::Context::default();
        let mut toasts = Toasts::default();
        toasts.info(&ctx, "Done");
        toasts.error(&ctx, "Failed");
        let back = |by: Duration| Instant::now() - by;
        toasts.list.iter_mut().for_each(|t| t.since = back(INFO_FOR + FADE));
        assert_eq!(toasts.messages(), ["Failed"]);
        toasts.list.iter_mut().for_each(|t| t.since = back(ERROR_FOR + FADE));
        assert!(toasts.messages().is_empty());
    }

    #[test]
    fn a_player_error_shows_once() {
        let ctx = egui::Context::default();
        let mut toasts = Toasts::default();
        let unavailable = "Song – Artist: This track isn't available in your region.";
        toasts.player_error(&ctx, Some(unavailable));
        toasts.list.clear(); // dismissed
        toasts.player_error(&ctx, Some(unavailable));
        assert!(toasts.messages().is_empty(), "still the same error");
        toasts.player_error(&ctx, None);
        toasts.player_error(&ctx, Some(unavailable));
        assert_eq!(toasts.messages(), [unavailable], "it happened again");
    }
}
