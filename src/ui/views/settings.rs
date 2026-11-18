//! The Settings window: a list of sections on the left (General, Playback,
//! Features, Sidebar, Crate and history, Updates, Troubleshooting), the section's
//! settings on the right. Opens from the account menu or with ⌘, (Ctrl+, elsewhere).

use crate::model::Quality;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{Appearance, colors, typography as ty};
use crate::ui::switches::{Page, Switch};
use crate::ui::widgets;
use eframe::egui::{self, RichText, Ui, vec2};

const WIDTH: f32 = 640.0;
/// The list of sections.
const NAV_WIDTH: f32 = 168.0;
const HEIGHT: f32 = 460.0;

/// A part of Settings, in the list on the left.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Section {
    #[default]
    General,
    Playback,
    Features,
    Sidebar,
    Sync,
    Updates,
    Troubleshooting,
}

impl Section {
    const ALL: [Section; 7] = [
        Section::General,
        Section::Playback,
        Section::Features,
        Section::Sidebar,
        Section::Sync,
        Section::Updates,
        Section::Troubleshooting,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Playback => "Playback",
            Section::Features => "Features",
            Section::Sidebar => "Sidebar",
            Section::Sync => "Crate and history",
            Section::Updates => "Updates",
            Section::Troubleshooting => "Troubleshooting",
        }
    }
}

impl App {
    /// Open (or close) Settings on its shortcut.
    pub(in crate::ui) fn settings_shortcut(&self, ctx: &egui::Context, acts: &mut Actions) {
        if ctx.input_mut(|i| i.consume_shortcut(&crate::ui::shortcuts::SETTINGS)) {
            acts.push(Action::ShowSettings(!self.show_settings));
        }
    }

    /// The Settings window, while open (call once per frame).
    pub(in crate::ui) fn settings_window(&self, ctx: &egui::Context, acts: &mut Actions) {
        if !self.show_settings {
            return;
        }
        let screen = ctx.content_rect();
        let width = WIDTH.min(screen.width() - 60.0);
        let height = HEIGHT.min(screen.height() - 120.0).max(240.0);
        let modal = egui::Modal::new(egui::Id::unique("settings")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Settings").font(ty::SECTION_TITLE.font()).color(colors().text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, 14.0, colors().dim).on_hover_text("Close (Esc)").clicked() {
                        acts.push(Action::ShowSettings(false));
                    }
                });
            });
            ui.add_space(10.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(NAV_WIDTH);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for section in Section::ALL {
                        if widgets::sidebar_item(ui, section.label(), None, self.settings_section == section).clicked() {
                            acts.push(Action::SettingsSection(section));
                        }
                    }
                    ui.add_space(12.0);
                    let version = format!("dust {}", env!("CARGO_PKG_VERSION"));
                    ui.label(RichText::new(version).font(ty::CAPTION.font()).color(colors().faint));
                });
                let (line, _) = ui.allocate_exact_size(vec2(1.0, height), egui::Sense::hover());
                ui.painter().vline(line.center().x, line.y_range(), egui::Stroke::new(1.0, colors().line));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("settings", self.settings_section))
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            self.settings_section(ui, acts);
                        });
                });
            });
        });
        if modal.should_close() {
            acts.push(Action::ShowSettings(false));
        }
    }

    fn settings_section(&self, ui: &mut Ui, acts: &mut Actions) {
        match self.settings_section {
            Section::General => self.appearance_picker(ui, acts),
            Section::Playback => self.playback_settings(ui, acts),
            Section::Features => self.feature_switches(ui, acts),
            Section::Sidebar => self.sidebar_switches(ui, acts),
            Section::Sync => self.sync_settings(ui, acts),
            Section::Updates => {
                widgets::caption(ui, "UPDATES");
                let auto = self.updates.auto;
                if widgets::switch_row(ui, "Check automatically", Some("Asks GitHub about new versions"), auto, true).clicked() {
                    acts.push(Action::AutoUpdate(!auto));
                }
                let status = self.updates.status();
                if widgets::menu_row(ui, Some(Icon::Refresh), "Check now", Some(&status), false, colors().text).clicked() {
                    acts.push(Action::CheckForUpdates);
                }
            }
            Section::Troubleshooting => {
                widgets::caption(ui, "TROUBLESHOOTING");
                let sub = Some("What happened with playback and AirPlay");
                if widgets::menu_row(ui, None, "Show log file", sub, false, colors().text).clicked() {
                    acts.push(Action::RevealLog);
                }
            }
        }
    }

    /// Streaming quality (a server plays its files as they are) and listen reports.
    fn playback_settings(&self, ui: &mut Ui, acts: &mut Actions) {
        if self.features().quality {
            self.quality_picker(ui, acts);
            ui.add_space(12.0);
        }
        widgets::caption(ui, "LISTENING");
        let (title, sub) = self.client.as_ref().map_or(("Report listens", ""), |c| c.listen_reports());
        if widgets::switch_row(ui, title, Some(sub), self.prefs.report_listens, true).clicked() {
            acts.push(Action::ToggleReportListens);
        }
    }

    /// The parts of dust that can be turned off.
    fn feature_switches(&self, ui: &mut Ui, acts: &mut Actions) {
        widgets::caption(ui, "FEATURES");
        ui.label(
            RichText::new("Turned off, a part is hidden everywhere; what it saved is kept.").font(ty::SMALL.font()).color(colors().faint),
        );
        ui.add_space(6.0);
        let row = |ui: &mut Ui, acts: &mut Actions, switch: Switch, title: &str, sub: &str| {
            let on = self.switch_on(switch);
            if widgets::switch_row(ui, title, Some(sub), on, true).clicked() {
                acts.push(Action::Switch(switch, !on));
            }
        };
        row(ui, acts, Switch::Crate, "Crate", "Save tracks, albums, artists and notes to check out later");
        if self.features().releases {
            let sub = "Albums, singles and EPs from artists you follow or play a lot go in your Crate as new";
            let on = self.prefs.release_alerts && self.prefs.crate_on;
            if widgets::switch_row(ui, "    New releases to the Crate", Some(sub), on, self.prefs.crate_on).clicked() {
                acts.push(Action::ReleaseAlerts(!self.prefs.release_alerts));
            }
        }
        let sub = "What you've heard: the history tab, Listening, lists made from it, new-to-you marks";
        row(ui, acts, Switch::History, "Play history", sub);
        row(ui, acts, Switch::Lyrics, "Lyrics", "The lyrics page and its button");
        row(ui, acts, Switch::AirPlay, "AirPlay speakers", "Look for speakers on the network to play on");
    }

    /// Which pages the sidebar lists (Home and Search always).
    fn sidebar_switches(&self, ui: &mut Ui, acts: &mut Actions) {
        widgets::caption(ui, "PAGES IN THE SIDEBAR");
        for page in Page::ALL.into_iter().filter(|p| self.has_page(*p) || *p == Page::Listening) {
            let switch = Switch::Page(page);
            let on = self.switch_on(switch);
            // Listening is made from the play history.
            let (enabled, sub) = match page {
                Page::Listening if !self.prefs.history_on => (false, Some("Needs the play history (Features)")),
                _ => (true, None),
            };
            if widgets::switch_row(ui, page.label(), sub, on && enabled, enabled).clicked() {
                acts.push(Action::Switch(switch, !on));
            }
        }
    }

    fn appearance_picker(&self, ui: &mut Ui, acts: &mut Actions) {
        widgets::caption(ui, "APPEARANCE");
        let options = Appearance::ALL.map(|a| {
            let icon = match a {
                Appearance::System => Icon::Computer,
                Appearance::Dark => Icon::Moon,
                Appearance::Light => Icon::Sun,
            };
            (icon, a.label())
        });
        let current = Appearance::ALL.iter().position(|a| *a == self.prefs.theme).unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, &options, current) {
            acts.push(Action::Appearance(Appearance::ALL[i]));
        }
    }

    fn quality_picker(&self, ui: &mut Ui, acts: &mut Actions) {
        widgets::caption(ui, "STREAMING QUALITY");
        for quality in Quality::ALL {
            let description = match quality {
                Quality::Mp3_128 => "Saves data",
                Quality::Mp3_320 => "High quality",
                Quality::Flac => "Lossless · needs a HiFi plan",
            };
            if widgets::menu_row(ui, None, quality.label(), Some(description), self.prefs.quality == quality, colors().text).clicked() {
                acts.push(Action::Quality(quality));
            }
        }
    }
}

/// Show the log file in the system's file manager.
pub fn reveal_log() {
    let Some(path) = crate::log::file_path() else { return };
    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(&path).spawn()
    } else if cfg!(windows) {
        std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(&path)).spawn()
    };
    if let Err(e) = opened {
        log_warn!("could not show {}: {e}", path.display());
    }
}
