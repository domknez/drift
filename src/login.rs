//! "Sign in with Microsoft": a helper process (`drift --login`) shows Microsoft's own
//! sign-in page in the OS webview and prints the authorization code once Microsoft
//! redirects back with it. Running it out of process keeps the webview out of the
//! app's memory and gives it its own main thread / event loop, as macOS requires.

use crate::teams::auth;
use std::process::{Command, Stdio};

pub const ARG: &str = "--login";
const PREFIX: &str = "code=";

/// Spawn the sign-in window and block until the user signs in or closes it.
pub fn obtain_code() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out =
        Command::new(exe).arg(ARG).stdin(Stdio::null()).stderr(Stdio::inherit()).output().map_err(|e| format!("sign-in window: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix(PREFIX))
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
        .ok_or_else(|| "Sign-in window closed".into())
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
