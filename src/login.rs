//! "Sign in with Microsoft": a helper process (`drift --login`) shows Microsoft's own
//! sign-in page in the OS webview and prints the authorization code once Microsoft
//! redirects back with it. Running it out of process keeps the webview out of the
//! app's memory and gives it its own main thread / event loop, as macOS requires.

use crate::teams::auth;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub const ARG: &str = "--login";
const PREFIX: &str = "code=";

/// The sign-in window while it's open, for [`cancel`].
static WINDOW: Mutex<Option<Child>> = Mutex::new(None);

/// Spawn the sign-in window and block until the user signs in, closes it, or
/// [`cancel`] closes it.
pub fn obtain_code() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .arg(ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("sign-in window: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("sign-in window: no output")?;
    *WINDOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    // The code is a line or two; reading ends when the window's process does.
    let mut out = String::new();
    let _ = stdout.read_to_string(&mut out);
    let child = WINDOW.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(mut child) = child {
        let _ = child.wait();
    }
    out.lines()
        .find_map(|l| l.strip_prefix(PREFIX))
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
        .ok_or_else(|| "Sign-in window closed".into())
}

/// Close the sign-in window, if it's open (the user gave up waiting).
pub fn cancel() {
    let child = WINDOW.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Entry point of the helper process.
pub fn run_window() {
    use std::io::Write;
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Sign in to Microsoft Teams")
        .with_inner_size(LogicalSize::new(480.0, 700.0))
        .build(&event_loop)
        .expect("sign-in window");
    // Incognito: the sign-in lives only in drift's keychain entry, not in a webview profile.
    let builder = WebViewBuilder::new().with_url(auth::authorize_url()).with_incognito(true).with_navigation_handler(|url: String| {
        if !url.starts_with(auth::REDIRECT) {
            return true;
        }
        match auth::code_from_redirect(&url) {
            Some(code) => {
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{PREFIX}{code}");
                let _ = out.flush();
            }
            None => eprintln!("drift: sign-in ended without a code"),
        }
        std::process::exit(0);
    });
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    let _webview = builder.build(&window).expect("webview");
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("gtk vbox")).expect("webview")
    };

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *flow = ControlFlow::Exit;
        }
    });
}
