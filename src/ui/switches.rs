//! Parts of dust that can be turned off in Settings: the Crate, the play history,
//! lyrics, AirPlay and each sidebar page. Off hides a part everywhere (sidebar,
//! panel tabs, menus, shortcuts, the command palette); what it saved is kept, and
//! comes back when it's turned on again.
//!
//! Views hide what's off; [`App::allows`] also turns away any action that would
//! reach it, so a shortcut or a stale menu can't.

use super::action::Action;
use super::app::{App, SidePanel};
use super::state::{ChannelView, CollSource, View};
use super::widgets::TrackMenu;
use crate::output::Output;
use eframe::egui::{self, Key};

/// A page of the sidebar that can be hidden. Home and Search always show.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Loved,
    Playlists,
    Albums,
    Artists,
    Podcasts,
    Explore,
    NewReleases,
    Discover,
    Listening,
}

impl Page {
    pub const ALL: [Page; 9] = [
        Page::Loved,
        Page::Playlists,
        Page::Albums,
        Page::Artists,
        Page::Podcasts,
        Page::Explore,
        Page::NewReleases,
        Page::Discover,
        Page::Listening,
    ];

    /// How it's kept in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Page::Loved => "loved",
            Page::Playlists => "playlists",
            Page::Albums => "albums",
            Page::Artists => "artists",
            Page::Podcasts => "podcasts",
            Page::Explore => "explore",
            Page::NewReleases => "new_releases",
            Page::Discover => "discover",
            Page::Listening => "listening",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Page::Loved => "Loved tracks",
            Page::Playlists => "Playlists",
            Page::Albums => "Albums",
            Page::Artists => "Artists",
            Page::Podcasts => "Podcasts",
            Page::Explore => "Explore",
            Page::NewReleases => "New releases",
            Page::Discover => "Discover",
            Page::Listening => "Listening",
        }
    }

    pub fn view(self) -> View {
        match self {
            Page::Loved => View::Loved,
            Page::Playlists => View::Playlists,
            Page::Albums => View::Albums,
            Page::Artists => View::Artists,
            Page::Podcasts => View::Podcasts,
            Page::Explore => View::Channel(Box::new(ChannelView::explore())),
            Page::NewReleases => View::Channel(Box::new(ChannelView::new_releases())),
            Page::Discover => View::Channel(Box::new(ChannelView::discover())),
            Page::Listening => View::Stats,
        }
    }

    /// The page `view` is, if it's one of these.
    pub fn of(view: &View) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.view() == *view)
    }

    /// Its ⌘-number (Home is ⌘1); pages past the numbered ones have none.
    pub fn key_code(self) -> Option<Key> {
        Some(match self {
            Page::Loved => Key::Num2,
            Page::Playlists => Key::Num3,
            Page::Albums => Key::Num4,
            Page::Artists => Key::Num5,
            Page::Podcasts => Key::Num6,
            Page::Explore => Key::Num7,
            Page::NewReleases => Key::Num8,
            Page::Discover => Key::Num9,
            Page::Listening => return None,
        })
    }
}

/// Something Settings turns on or off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Switch {
    Crate,
    History,
    Lyrics,
    AirPlay,
    Page(Page),
}

impl App {
    /// Whether the source has `page` at all.
    pub(super) fn has_page(&self, page: Page) -> bool {
        let features = self.features();
        match page {
            Page::Podcasts => features.podcasts,
            Page::Explore | Page::NewReleases | Page::Discover => features.explore,
            Page::Listening => self.prefs.history_on,
            _ => true,
        }
    }

    /// Whether `page` shows: the source has it and it isn't turned off.
    pub(super) fn page_on(&self, page: Page) -> bool {
        self.has_page(page) && !self.prefs.hidden_pages.iter().any(|k| k == page.key())
    }

    /// The pages the sidebar shows, in order.
    pub(super) fn pages_on(&self) -> Vec<Page> {
        Page::ALL.into_iter().filter(|p| self.page_on(*p)).collect()
    }

    pub(super) fn switch_on(&self, switch: Switch) -> bool {
        match switch {
            Switch::Crate => self.prefs.crate_on,
            Switch::History => self.prefs.history_on,
            Switch::Lyrics => self.prefs.lyrics,
            Switch::AirPlay => self.prefs.airplay,
            Switch::Page(page) => !self.prefs.hidden_pages.iter().any(|k| k == page.key()),
        }
    }

    /// Whether `view` can be shown with what's turned off.
    pub(super) fn view_allowed(&self, view: &View) -> bool {
        match view {
            View::Crate => self.prefs.crate_on,
            View::Lyrics => self.prefs.lyrics,
            View::Collection(coll) if matches!(coll.source, CollSource::History(_)) => self.prefs.history_on,
            _ => Page::of(view).is_none_or(|page| self.page_on(page)),
        }
    }

    pub(super) fn panel_allowed(&self, panel: SidePanel) -> bool {
        match panel {
            SidePanel::Queue => true,
            SidePanel::History => self.prefs.history_on,
            SidePanel::Crate => self.prefs.crate_on,
        }
    }

    /// Whether `action` reaches only parts that are on.
    pub(super) fn allows(&self, action: &Action) -> bool {
        let prefs = &self.prefs;
        match action {
            Action::Open(view) => self.view_allowed(view),
            Action::OpenCollection(coll) => self.view_allowed(&View::Collection(Box::new(coll.clone()))),
            Action::PlaySource(CollSource::History(_), _) | Action::FlowNewOnly(_) => prefs.history_on,
            Action::SaveToCrate(..)
            | Action::CrateNote(_)
            | Action::Crate(_)
            | Action::OpenCrate(..)
            | Action::PlayCrate
            | Action::ReleaseAlerts(_)
            | Action::TrackMenu(_, TrackMenu::Crate) => prefs.crate_on,
            Action::ShowPanel(Some(panel)) => self.panel_allowed(*panel),
            Action::ToggleLyrics => prefs.lyrics,
            Action::Output(Output::AirPlay(_)) | Action::FindSpeakers => prefs.airplay,
            _ => true,
        }
    }

    /// Turn a part of dust on or off, and leave what it was showing.
    pub(super) fn set_switch(&mut self, ctx: &egui::Context, switch: Switch, on: bool) {
        self.prefs.update(|p| match switch {
            Switch::Crate => p.crate_on = on,
            Switch::History => p.history_on = on,
            Switch::Lyrics => p.lyrics = on,
            Switch::AirPlay => p.airplay = on,
            Switch::Page(page) => {
                p.hidden_pages.retain(|k| k != page.key());
                if !on {
                    p.hidden_pages.push(page.key().to_string());
                }
            }
        });
        match switch {
            Switch::History => self.send_heard(),
            Switch::AirPlay if on => self.start_discovery(ctx),
            Switch::AirPlay => {
                self.discovery = None;
                self.restore_output = None;
                if self.player.status().output != Output::Local.name() {
                    self.player.send(crate::player::Cmd::Output(Output::Local));
                }
            }
            _ => {}
        }
        if !self.panel_allowed(self.side_tab) {
            self.side_tab = SidePanel::Queue;
        }
        if self.side_panel.is_some_and(|p| !self.panel_allowed(p)) {
            self.side_panel = Some(SidePanel::Queue);
        }
        if !self.view_allowed(&self.browse.view) {
            self.open_view(View::Home);
        }
        // Last: leaving the page just turned off put it behind Back.
        let mut history = std::mem::take(&mut self.browse.history);
        history.keep(|v| self.view_allowed(&v.view));
        self.browse.history = history;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Cmd;
    use crate::ui::app::testing::*;
    use crate::ui::crate_store::Thing;
    use crate::ui::state::Coll;
    use crate::ui::stats::Smart;

    fn app(settings: &str) -> TestApp {
        let mut t = TestApp::new(settings);
        t.app.client = Some(deezer());
        t
    }

    /// Logged in to Deezer, with what that sent the player taken.
    fn logged_in(settings: &str) -> TestApp {
        let mut t = TestApp::new(settings);
        t.app.source_ready(deezer());
        t.sent();
        t
    }

    fn heard(id: u64) -> crate::player::Played {
        crate::player::Played {
            source: "deezer".into(),
            track: track(id, true),
            started_unix: 1_000_000,
            heard_secs: 200,
            skipped: false,
            flow: None,
        }
    }

    /// The palette's labels, with `playing` playing.
    fn palette(t: &TestApp, playing: Option<u64>) -> Vec<String> {
        let st = crate::player::Status { track: playing.map(|id| track(id, true)), ..Default::default() };
        t.app.commands(&st, &t.app.new_palette().listened).into_iter().map(|c| c.label).collect()
    }

    /// The pages ⌘`key` opens.
    fn shortcut(t: &TestApp, key: Key) -> Vec<View> {
        let mut acts = crate::ui::action::Actions::default();
        let press = egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND };
        let input = egui::RawInput { events: vec![press], ..Default::default() };
        t.ctx.run_ui(input, |ui| t.app.nav_shortcuts(ui.ctx(), &mut acts)).textures_delta.clear();
        acts.into_vec()
            .into_iter()
            .filter_map(|a| match a {
                Action::Open(view) => Some(view),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_page_has_its_key() {
        for page in Page::ALL {
            assert_eq!(Page::of(&page.view()), Some(page));
        }
        let keys: std::collections::HashSet<_> = Page::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(keys.len(), Page::ALL.len());
    }

    #[test]
    fn what_is_off_is_turned_away() {
        let t = app("crate=false\nhistory=false\nlyrics=false\nairplay=false\nhidden_pages=albums");
        let a = &t.app;
        assert!(!a.allows(&Action::SaveToCrate(Thing::Note, String::new())));
        assert!(!a.allows(&Action::Open(View::Crate)));
        assert!(!a.allows(&Action::ShowPanel(Some(SidePanel::Crate))));
        assert!(!a.allows(&Action::ShowPanel(Some(SidePanel::History))));
        assert!(!a.allows(&Action::Open(View::Stats)), "the Listening page is made from the history");
        assert!(!a.allows(&Action::OpenCollection(Coll::history(Smart::OnRepeat))));
        assert!(!a.allows(&Action::ToggleLyrics));
        assert!(!a.allows(&Action::FindSpeakers));
        assert!(!a.allows(&Action::Open(View::Albums)));
        assert!(a.allows(&Action::Open(View::Artists)) && a.allows(&Action::ShowPanel(Some(SidePanel::Queue))));
        assert!(!a.pages_on().contains(&Page::Albums) && !a.pages_on().contains(&Page::Listening));
    }

    #[test]
    fn turning_off_leaves_the_page_and_panel() {
        let mut t = app("");
        t.update(Action::Open(View::Crate));
        t.update(Action::ShowPanel(Some(SidePanel::Crate)));
        t.app.set_switch(&t.ctx.clone(), Switch::Crate, false);
        assert_eq!(t.app.browse.view, View::Home);
        t.update(Action::Back);
        assert_eq!(t.app.browse.view, View::Home, "Back skips the Crate, now off");
        assert!(!t.app.browse.history.can_go_back(), "and there's nothing else behind");
        assert!(t.app.side_panel == Some(SidePanel::Queue) && t.app.side_tab == SidePanel::Queue);
        t.update(Action::OpenCrate(None, None));
        assert_eq!(t.app.browse.view, View::Home, "the Crate stays shut");
        t.app.set_switch(&t.ctx.clone(), Switch::Crate, true);
        t.update(Action::OpenCrate(None, None));
        assert_eq!(t.app.browse.view, View::Crate);

        t.app.set_switch(&t.ctx.clone(), Switch::Page(Page::Albums), false);
        assert!(!t.app.switch_on(Switch::Page(Page::Albums)));
        assert_eq!(t.app.prefs.hidden_pages, ["albums"]);
        t.app.set_switch(&t.ctx.clone(), Switch::Page(Page::Albums), true);
        assert!(t.app.prefs.hidden_pages.is_empty());
    }

    #[test]
    fn with_the_history_off_nothing_is_recorded_or_told_to_the_player() {
        let mut t = logged_in("history=false\nflow_new_only=true");
        t.app.player.set_status(|s| s.played = vec![heard(1)]);
        t.app.record_plays();
        assert_eq!(t.app.plays.len(), 0, "not recorded");
        assert_eq!(t.app.prefs.new_only(), None, "no *new to me only* to offer");
        t.app.send_heard();
        assert!(matches!(t.sent()[..], [Cmd::Heard(None)]), "Flow plays everything, shuffle favours nothing");

        t.app.set_switch(&t.ctx.clone(), Switch::History, true);
        assert!(matches!(t.sent()[..], [Cmd::Heard(Some(_))]), "turned on, the player gets the history again");
        t.app.player.set_status(|s| s.played = vec![heard(2)]);
        t.app.record_plays();
        assert_eq!(t.app.plays.len(), 1);
        match &t.sent()[..] {
            [Cmd::Heard(Some(heard))] => assert_eq!(heard.get(&track(2, true).id), Some(&1_000_000), "with when"),
            _ => panic!("expected the history sent to the player"),
        }
    }

    #[test]
    fn new_releases_are_not_looked_for_with_the_crate_off() {
        let mut t = logged_in("crate=false");
        let jobs = t.jobs.waiting();
        t.app.release_tick(&t.ctx.clone());
        assert_eq!(t.jobs.waiting(), jobs, "no look started");
        t.app.set_switch(&t.ctx.clone(), Switch::Crate, true);
        t.app.release_tick(&t.ctx.clone());
        assert_eq!(t.jobs.waiting(), jobs + 1, "the Crate on, it looks");
    }

    #[test]
    fn the_palette_offers_nothing_turned_off() {
        let all = palette(&logged_in(""), Some(1));
        let off = palette(&logged_in("crate=false\nhistory=false\nlyrics=false\nhidden_pages=albums"), Some(1));
        let on_repeat = Smart::OnRepeat.title().to_string();
        for label in ["Crate", "Lyrics", "Show the play history", "Albums", "Listening", on_repeat.as_str()] {
            assert!(all.iter().any(|l| l == label), "{label} offered with everything on");
            assert!(!off.iter().any(|l| l == label), "{label} not offered when off");
        }
        assert!(all.iter().any(|l| l.contains("to the Crate")));
        assert!(!off.iter().any(|l| l.contains("Crate")), "not even saving what's playing");
        for label in ["Home", "Artists", "Show the queue", "Settings"] {
            assert!(off.iter().any(|l| l == label), "{label} stays");
        }
    }

    #[test]
    fn shortcuts_skip_hidden_pages() {
        let t = logged_in("hidden_pages=albums");
        assert!(shortcut(&t, Key::Num4).is_empty(), "⌘4 is Albums, hidden");
        assert_eq!(shortcut(&t, Key::Num5), [View::Artists], "the others keep their number");
        assert_eq!(shortcut(&t, Key::Num1), [View::Home]);
        assert_eq!(shortcut(&logged_in(""), Key::Num4), [View::Albums]);
        assert_eq!(shortcut(&logged_in(""), Key::Num9), [View::Channel(Box::new(ChannelView::discover()))]);
        assert!(shortcut(&logged_in("hidden_pages=discover"), Key::Num9).is_empty());
    }

    #[test]
    fn panel_tabs_and_marks_follow_the_switches() {
        let mut t = logged_in("");
        t.update(Action::ShowPanel(Some(SidePanel::History)));
        t.app.set_switch(&t.ctx.clone(), Switch::History, false);
        assert!(t.app.side_panel == Some(SidePanel::Queue), "the history tab gives way to the queue");
        t.update(Action::TogglePanel);
        t.update(Action::TogglePanel);
        assert!(t.app.side_panel == Some(SidePanel::Queue), "and opens there again");
        assert!(!t.app.allows(&Action::FlowNewOnly(true)));
        t.app.set_switch(&t.ctx.clone(), Switch::Lyrics, false);
        t.update(Action::ToggleLyrics);
        assert_ne!(t.app.browse.view, View::Lyrics);
    }

    #[test]
    fn turning_airplay_off_stops_the_search_and_the_speaker() {
        let mut t = logged_in("");
        t.app.player.set_status(|s| s.output = "Kitchen".into());
        t.app.restore_output = Some("Kitchen".into());
        t.app.set_switch(&t.ctx.clone(), Switch::AirPlay, false);
        assert!(t.app.discovery.is_none() && t.app.restore_output.is_none());
        assert!(matches!(t.sent()[..], [Cmd::Output(Output::Local)]), "back to this computer");
        assert!(!t.app.prefs.airplay);
    }
}
