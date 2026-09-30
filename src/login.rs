//! "Log in with Deezer": a helper process (`dust --login`) shows deezer.com's own
//! login page in the OS webview and prints the session's `arl` cookie once the user
//! is signed in. Running it out-of-process keeps the webview out of the player's
//! memory and gives it its own main thread / event loop, as macOS requires.

use std::process::{Command, Stdio};

pub const ARG: &str = "--login";
const SAFARI_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15";
const PREFIX: &str = "arl=";

/// Spawn the login window and block until the user signs in or closes it.
pub fn obtain_arl() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .arg(ARG)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("login window: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix(PREFIX))
        .map(|arl| arl.trim().to_string())
        .filter(|arl| !arl.is_empty())
        .ok_or_else(|| "Login window closed".into())
}

/// Entry point of the helper process.
pub fn run_window() {
    use std::io::Write;
    use std::time::{Duration, Instant};
    use tao::dpi::LogicalSize;
    use tao::event::{Event, StartCause, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    const URL: &str = "https://www.deezer.com/login";
    const POLL: Duration = Duration::from_millis(500);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Log in to Deezer")
        .with_inner_size(LogicalSize::new(480.0, 760.0))
        .build(&event_loop)
        .expect("login window");
    // Incognito: the session lives only in dust's keychain entry, not in a webview profile.
    let mut builder = WebViewBuilder::new().with_url(URL).with_incognito(true);
    // Embedded WebKit omits "Version/.. Safari/.." from its user agent and Deezer's web
    // app then refuses to run ("browser not supported"). Present as the Safari it is.
    // WebView2 on Windows already sends a full Edge user agent.
    if !cfg!(windows) {
        builder = builder.with_user_agent(SAFARI_UA);
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    let webview = builder.build(&window).expect("webview");
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("gtk vbox")).expect("webview")
    };

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + POLL);
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                // Not cookies_for_url(): it wants an exact domain match, but arl lives on
                // ".deezer.com" while the page is www.deezer.com.
                let cookies = webview.cookies().unwrap_or_default();
                let is_deezer = |c: &wry::cookie::Cookie| c.domain().is_some_and(|d| d.trim_start_matches('.').ends_with("deezer.com"));
                if let Some(arl) = cookies.iter().find(|c| c.name() == "arl" && !c.value().is_empty() && is_deezer(c)) {
                    let mut out = std::io::stdout().lock();
                    let _ = writeln!(out, "{PREFIX}{}", arl.value());
                    let _ = out.flush();
                    std::process::exit(0);
                }
            }
            _ => {}
        }
    });
}
