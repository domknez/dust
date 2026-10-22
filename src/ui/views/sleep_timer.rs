//! The sleep timer button in the player bar and its menu.

use crate::player::{Cmd, Sleep, Status};
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, RichText, Ui};
use std::time::{Duration, Instant};

const MINUTES: [u64; 5] = [15, 30, 45, 60, 90];

impl App {
    pub(in crate::ui) fn sleep_timer_button(&mut self, ui: &mut Ui, st: &Status) {
        let p = colors();
        let sleep = st.sleep.filter(|s| !matches!(s, Sleep::At(at) if *at <= Instant::now()));
        let hint = match sleep {
            None => "Sleep timer".to_string(),
            Some(Sleep::EndOfTrack) => "Sleep timer: stops after this track".to_string(),
            Some(Sleep::At(at)) => format!("Sleep timer: pauses in {}", left(at)),
        };
        let resp = widgets::icon_button(ui, Icon::Moon, 16.0, if sleep.is_some() { p.accent } else { p.dim }).on_hover_text(hint);
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(180.0);
            ui.add_space(4.0);
            ui.label(RichText::new("SLEEP TIMER").color(p.faint).font(ty::OVERLINE.font()));
            ui.add_space(2.0);
            let mut pick = None;
            if let Some(Sleep::At(at)) = sleep {
                ui.label(RichText::new(format!("Pauses in {}", left(at))).color(p.accent).font(ty::SMALL.font()));
            }
            for m in MINUTES {
                let label = if m < 60 || m % 60 != 0 { format!("{m} minutes") } else { format!("{} hour", m / 60) };
                if ui.button(RichText::new(label).font(ty::BODY.font())).clicked() {
                    pick = Some(Some(Sleep::At(Instant::now() + Duration::from_secs(m * 60))));
                }
            }
            let end = sleep == Some(Sleep::EndOfTrack);
            if ui.selectable_label(end, RichText::new("End of track").font(ty::BODY.font())).clicked() {
                pick = Some(Some(Sleep::EndOfTrack));
            }
            if sleep.is_some() && ui.button(RichText::new("Turn off").font(ty::BODY.font()).color(p.dim)).clicked() {
                pick = Some(None);
            }
            if let Some(sleep) = pick {
                self.player.send(Cmd::Sleep(sleep));
                ui.close();
            }
        });
    }
}

/// Time left until `at`, in whole minutes rounded up ("1 min" in the last minute).
fn left(at: Instant) -> String {
    let secs = at.saturating_duration_since(Instant::now()).as_secs();
    match secs.div_ceil(60) {
        m @ 0..=60 => format!("{} min", m.max(1)),
        m => format!("{} h {} min", m / 60, m % 60),
    }
}
