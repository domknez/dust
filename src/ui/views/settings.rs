//! The Settings window: appearance, streaming quality, listen sharing, updates and
//! troubleshooting. Opens from the account menu or with ⌘, (Ctrl+, elsewhere).

use crate::deezer::Quality;
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{Appearance, colors, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Ui};

/// ⌘, on macOS; Ctrl+, on Windows and Linux (as in most apps there).
const SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
pub(in crate::ui) const SETTINGS_HINT: &str = if cfg!(target_os = "macos") { "⌘," } else { "Ctrl+," };
const WIDTH: f32 = 380.0;

impl App {
    /// Open (or close) Settings on its shortcut.
    pub(in crate::ui) fn settings_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT)) {
            self.show_settings = !self.show_settings;
        }
    }

    /// The Settings window, while open (call once per frame).
    pub(in crate::ui) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let max_height = (ctx.content_rect().height() - 120.0).max(240.0);
        let modal = egui::Modal::new(egui::Id::new("settings")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Settings").font(ty::SECTION_TITLE.font()).color(colors().text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, Icon::Close, 14.0, colors().dim).on_hover_text("Close (Esc)").clicked() {
                        self.show_settings = false;
                    }
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                self.settings_sections(ui);
            });
        });
        if modal.should_close() {
            self.show_settings = false;
        }
    }

    fn settings_sections(&mut self, ui: &mut Ui) {
        self.appearance_picker(ui);
        ui.add_space(12.0);
        // A server plays its files as they are.
        if self.features().quality {
            self.quality_picker(ui);
            ui.add_space(12.0);
        }
        widgets::caption(ui, "LISTENING");
        let (title, sub) = if self.features().flow {
            ("Share listening with Deezer", "History, Flow and Last.fm scrobbling via Deezer")
        } else {
            ("Report plays to the server", "Play counts and \"Recently played\" (and Last.fm, if the server scrobbles)")
        };
        if widgets::menu_row(ui, None, title, Some(sub), self.report_listens, colors().text).clicked() {
            self.toggle_report_listens();
        }
        ui.add_space(12.0);
        widgets::caption(ui, "UPDATES");
        let auto = self.updates.auto;
        if widgets::menu_row(ui, None, "Check automatically", Some("Asks GitHub about new versions"), auto, colors().text).clicked() {
            self.updates.set_auto(!auto);
        }
        let status = self.updates.status();
        if widgets::menu_row(ui, Some(Icon::Refresh), "Check now", Some(&status), false, colors().text).clicked() {
            self.updates.check_now(ui.ctx());
        }
        ui.add_space(12.0);
        widgets::caption(ui, "TROUBLESHOOTING");
        if widgets::menu_row(ui, None, "Show log file", Some("What happened with playback and AirPlay"), false, colors().text).clicked() {
            reveal_log();
        }
        ui.add_space(4.0);
        let version = format!("dust {}", env!("CARGO_PKG_VERSION"));
        ui.label(RichText::new(version).font(ty::CAPTION.font()).color(colors().faint));
    }

    fn appearance_picker(&mut self, ui: &mut Ui) {
        widgets::caption(ui, "APPEARANCE");
        let options = Appearance::ALL.map(|a| {
            let icon = match a {
                Appearance::System => Icon::Computer,
                Appearance::Dark => Icon::Moon,
                Appearance::Light => Icon::Sun,
            };
            (icon, a.label())
        });
        let current = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, &options, current) {
            self.set_appearance(Appearance::ALL[i]);
        }
    }

    fn quality_picker(&mut self, ui: &mut Ui) {
        widgets::caption(ui, "STREAMING QUALITY");
        for quality in Quality::ALL {
            let description = match quality {
                Quality::Mp3_128 => "Saves data",
                Quality::Mp3_320 => "High quality",
                Quality::Flac => "Lossless · needs a HiFi plan",
            };
            if widgets::menu_row(ui, None, quality.label(), Some(description), self.quality == quality, colors().text).clicked() {
                self.set_quality(quality);
            }
        }
    }
}

/// Show the log file in the system's file manager.
fn reveal_log() {
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
