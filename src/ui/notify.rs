//! A system notification when a new track starts while dust is out of sight:
//! macOS Notification Center (as dust.app; a bare binary has no bundle to notify
//! as) and the Linux desktop's notification service. Not on Windows yet: a toast
//! there needs an installed app identity.

use crate::model::TrackId;
use crate::player::{State, Status};
use eframe::egui::ViewportInfo;

/// What a notification says: a title and a line under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

/// The track seen playing last, to tell each new one once.
#[derive(Default)]
pub struct TrackNotices {
    seen: Option<TrackId>,
}

impl TrackNotices {
    /// The notice for a track that just started playing, when one is to be
    /// told now (`tell`: notices are on and dust is out of sight). A track that
    /// is only loading or paused waits until it plays; one that started while
    /// not told isn't told later.
    pub fn due(&mut self, st: &Status, tell: bool) -> Option<Notice> {
        let id = st.track.as_ref().map(|t| t.id.clone());
        if id == self.seen || (id.is_some() && st.state != State::Playing) {
            return None;
        }
        self.seen = id;
        let track = st.track.as_ref().filter(|_| tell)?;
        let body = [track.artist.as_str(), &track.album].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" — ");
        Some(Notice { title: track.title.clone(), body })
    }
}

/// Find out, in the background, whether notifications can be shown here (call
/// once, at start).
pub fn probe() {
    platform::probe();
}

/// Whether notifications can be shown here (the settings offer them only then).
pub fn available() -> bool {
    platform::available()
}

/// Show `notice`, replacing the one before; never waits. Nothing where
/// notifications can't show.
pub fn show(notice: Notice) {
    if available() {
        platform::show(notice);
    }
}

/// Ask the system to let dust notify (macOS asks the listener once); `answered`
/// runs once it has said, on a thread of its own.
pub fn ask(answered: impl Fn() + Send + 'static) {
    platform::ask(answered);
}

/// Why notifications don't show though they're on, in the system's own terms,
/// when it said no.
pub fn blocked() -> Option<&'static str> {
    platform::blocked()
}

/// Whether dust is out of sight: its window minimized, covered, or behind
/// another app's, and no mini player on top.
pub fn out_of_sight(window: &ViewportInfo, focused: bool, mini_player: bool) -> bool {
    let hidden = window.minimized == Some(true) || window.occluded == Some(true) || !focused;
    hidden && !mini_player
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Notice;
    use block2::{DynBlock, RcBlock};
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{AllocAnyThread, define_class, msg_send};
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::cell::OnceCell;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The listener said no (macOS asks once; they can change it in System
    /// Settings later, which the next ask finds out).
    static REFUSED: AtomicBool = AtomicBool::new(false);

    pub fn blocked() -> Option<&'static str> {
        REFUSED.load(Ordering::Relaxed).then_some("Turned off for dust in System Settings → Notifications")
    }

    define_class!(
        /// Shows a notice as a banner while dust is the active app (minimized,
        /// say): without a delegate saying so, the system only files it away.
        #[unsafe(super(NSObject))]
        #[name = "DustNoticePresenter"]
        struct Presenter;

        unsafe impl NSObjectProtocol for Presenter {}

        unsafe impl UNUserNotificationCenterDelegate for Presenter {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                present: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                present.call((UNNotificationPresentationOptions::Banner,));
            }
        }
    );

    impl Presenter {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(());
            unsafe { msg_send![super(this), init] }
        }
    }

    thread_local! {
        /// The center's delegate, kept here: the center holds it only weakly.
        static PRESENTER: OnceCell<Retained<Presenter>> = const { OnceCell::new() };
    }

    pub fn probe() {}

    /// Notifications come from an app: run as a bare binary, dust has no bundle
    /// identifier to send them as.
    pub fn available() -> bool {
        NSBundle::mainBundle().bundleIdentifier().is_some()
    }

    /// The notification center, with dust's delegate, when dust runs as an app
    /// (asking for it without a bundle throws). From the main thread only, which
    /// keeps the one delegate.
    fn center() -> Option<Retained<UNUserNotificationCenter>> {
        if !available() || MainThreadMarker::new().is_none() {
            return None;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        PRESENTER.with(|presenter| {
            presenter.get_or_init(|| {
                let presenter = Presenter::new();
                center.setDelegate(Some(ProtocolObject::from_ref(&*presenter)));
                presenter
            });
        });
        Some(center)
    }

    pub fn ask(answered: impl Fn() + Send + 'static) {
        let Some(center) = center() else { return };
        let done = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            // SAFETY: the system passes either null or a valid NSError for the call.
            match unsafe { error.as_ref() } {
                // Couldn't ask (not a refusal): say why in the log, keep what's known.
                Some(error) => log_debug!("notifications: {}", error.localizedDescription()),
                None => REFUSED.store(!granted.as_bool(), Ordering::Relaxed),
            }
            answered();
        });
        center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &done);
    }

    pub fn show(notice: Notice) {
        let Some(center) = center() else { return };
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notice.title));
        content.setBody(&NSString::from_str(&notice.body));
        // One identifier: each track's notice replaces the last.
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str("now-playing"), &content, None);
        center.addNotificationRequest_withCompletionHandler(&request, None);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::Notice;
    use dbus::arg::{PropMap, RefArg, Variant};
    use dbus::blocking::Connection;
    use std::sync::OnceLock;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::Duration;

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";
    const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
    const EXPIRE_MS: i32 = 5000;

    /// Notices for the thread that shows them.
    static NOTICES: OnceLock<Sender<Notice>> = OnceLock::new();

    /// Whether the session has a notification service, as [`probe`] found.
    static AVAILABLE: OnceLock<bool> = OnceLock::new();

    /// Ask the session bus about its notification service, on a thread of its
    /// own (the calls can wait on a slow bus); until it answers, notifications
    /// aren't offered.
    pub fn probe() {
        let asked = std::thread::Builder::new().name("notifications-probe".into()).spawn(|| {
            let found = Connection::new_session().and_then(|bus| has_service(&bus));
            let _ = AVAILABLE.set(found.inspect_err(|e| log_debug!("notifications: {e}")).unwrap_or(false));
        });
        if let Err(e) = asked {
            log_debug!("notifications: no thread: {e}");
        }
    }

    pub fn available() -> bool {
        AVAILABLE.get().copied().unwrap_or(false)
    }

    pub fn blocked() -> Option<&'static str> {
        None
    }

    /// Whether the notification service runs, or the bus starts it on demand.
    fn has_service(bus: &Connection) -> Result<bool, dbus::Error> {
        let dbus = bus.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", REPLY_TIMEOUT);
        let (running,): (bool,) = dbus.method_call("org.freedesktop.DBus", "NameHasOwner", (SERVICE,))?;
        if running {
            return Ok(true);
        }
        let (startable,): (Vec<String>,) = dbus.method_call("org.freedesktop.DBus", "ListActivatableNames", ())?;
        Ok(startable.iter().any(|name| name == SERVICE))
    }

    pub fn ask(_answered: impl Fn() + Send + 'static) {}

    pub fn show(notice: Notice) {
        let notices = NOTICES.get_or_init(|| {
            let (tx, rx) = channel();
            if let Err(e) = std::thread::Builder::new().name("notifications".into()).spawn(move || deliver(&rx)) {
                log_debug!("notifications: no thread: {e}");
            }
            tx
        });
        let _ = notices.send(notice);
    }

    /// Show each notice in turn over one session bus connection (made again
    /// after a failure), each replacing the one before.
    fn deliver(notices: &Receiver<Notice>) {
        let mut bus: Option<Connection> = None;
        let mut last = 0;
        for notice in notices {
            let conn = match bus.take().map_or_else(Connection::new_session, Ok) {
                Ok(conn) => bus.insert(conn),
                Err(e) => {
                    log_debug!("notifications: no session bus: {e}");
                    continue;
                }
            };
            match notify(conn, &notice, last) {
                Ok(id) => last = id,
                Err(e) => {
                    log_debug!("notification: {e}");
                    bus = None;
                }
            }
        }
    }

    /// `Notify(app_name, replaces_id, app_icon, summary, body, actions, hints,
    /// expire_timeout)`; the new notification's id.
    fn notify(bus: &Connection, notice: &Notice, replaces: u32) -> Result<u32, dbus::Error> {
        // Transient: shown, not kept in the desktop's notification history.
        let hints = PropMap::from([("transient".to_string(), Variant(Box::new(true) as Box<dyn RefArg>))]);
        let args = ("dust", replaces, "dust", notice.title.as_str(), notice.body.as_str(), Vec::<String>::new(), hints, EXPIRE_MS);
        let (id,): (u32,) = bus.with_proxy(SERVICE, PATH, REPLY_TIMEOUT).method_call(SERVICE, "Notify", args)?;
        Ok(id)
    }
}

#[cfg(not(unix))]
mod platform {
    use super::Notice;

    pub fn probe() {}

    pub fn blocked() -> Option<&'static str> {
        None
    }

    pub fn available() -> bool {
        false
    }

    pub fn ask(_answered: impl Fn() + Send + 'static) {}

    pub fn show(_notice: Notice) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing;

    fn playing(id: u64, state: State) -> Status {
        let mut t = testing::track(id, true);
        (t.artist, t.album) = ("Daft Punk".into(), "Discovery".into());
        Status { track: Some(t), state, ..Default::default() }
    }

    #[test]
    fn each_new_track_is_told_once_when_it_plays() {
        let mut n = TrackNotices::default();
        let notice = n.due(&playing(1, State::Playing), true);
        assert_eq!(notice, Some(Notice { title: "Song 1".into(), body: "Daft Punk — Discovery".into() }));
        assert_eq!(n.due(&playing(1, State::Playing), true), None, "the same track again");
        assert_eq!(n.due(&playing(2, State::Loading), true), None, "loading: wait until it plays");
        assert!(n.due(&playing(2, State::Playing), true).is_some());
    }

    #[test]
    fn a_track_started_while_not_told_stays_untold() {
        let mut n = TrackNotices::default();
        assert_eq!(n.due(&playing(1, State::Playing), false), None);
        assert_eq!(n.due(&playing(1, State::Playing), true), None, "dust went out of sight mid-track");
        assert!(n.due(&playing(2, State::Playing), true).is_some());
    }

    #[test]
    fn out_of_sight_is_hidden_or_behind_and_no_mini_player() {
        let shown = ViewportInfo { minimized: Some(false), occluded: Some(false), ..Default::default() };
        let minimized = ViewportInfo { minimized: Some(true), ..Default::default() };
        let covered = ViewportInfo { occluded: Some(true), ..Default::default() };
        assert!(!out_of_sight(&shown, true, false), "in front");
        assert!(out_of_sight(&shown, false, false), "behind another app");
        assert!(out_of_sight(&minimized, true, false), "minimized while still the active app");
        assert!(out_of_sight(&covered, true, false));
        assert!(!out_of_sight(&minimized, false, true), "the mini player is on top");
    }

    #[test]
    fn the_body_names_what_is_known() {
        let mut n = TrackNotices::default();
        let mut st = playing(1, State::Playing);
        st.track.as_mut().unwrap().album.clear();
        assert_eq!(n.due(&st, true).unwrap().body, "Daft Punk");
    }
}
