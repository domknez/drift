//! The system's notifications for new messages, and the unread count on the Dock
//! icon (macOS).

/// The bundle id drift's macOS app carries; notifications show as coming from it.
#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "io.github.domknez.drift";

/// Show a notification; `on_click` runs (on another thread) if the user clicks it.
pub fn show(title: &str, subtitle: Option<&str>, body: &str, on_click: impl FnOnce() + Send + 'static) {
    let (title, subtitle, body) = (title.to_string(), subtitle.map(str::to_string), body.to_string());
    let spawned = std::thread::Builder::new().name("notification".into()).spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.summary(&title).body(&body).appname("drift");
        #[cfg(target_os = "macos")]
        {
            // Only an app bundle has an identity of its own to send from.
            let bundled = std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS"));
            if bundled {
                let _ = notify_rust::set_application(BUNDLE_ID);
            }
            if let Some(subtitle) = &subtitle {
                n.subtitle(subtitle);
            }
            // Sends it and waits for a click (or for it to be dismissed).
            notify_rust::NotificationHandle::new(n).wait_for_action(|action| {
                if action != "__closed" {
                    on_click();
                }
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(subtitle) = &subtitle {
                n.body(&format!("{subtitle}: {body}"));
            }
            match n.show() {
                #[cfg(all(unix, not(target_os = "macos")))]
                Ok(handle) => handle.wait_for_action(|action| {
                    if action == "default" {
                        on_click();
                    }
                }),
                #[cfg(windows)]
                Ok(_) => drop(on_click),
                Err(e) => log_warn!("could not show a notification: {e}"),
            }
        }
    });
    if let Err(e) = spawned {
        log_warn!("could not show a notification: {e}");
    }
}

/// The number on the Dock icon (nothing for 0). Call from the main thread.
pub fn badge(count: usize) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSString;
        let Some(main) = MainThreadMarker::new() else { return };
        let tile = NSApplication::sharedApplication(main).dockTile();
        let label = (count > 0).then(|| NSString::from_str(&if count > 99 { "99+".to_string() } else { count.to_string() }));
        tile.setBadgeLabel(label.as_deref());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = count;
}
