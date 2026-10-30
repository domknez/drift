//! The system's notifications for new messages, and the unread count on the Dock
//! icon (macOS).

/// Whether drift runs from its app bundle, which macOS needs to send notifications
/// as an app of its own.
#[cfg(target_os = "macos")]
fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS"))
}

/// Ask for leave to notify (macOS asks the user once). Call at start.
pub fn init() {
    #[cfg(target_os = "macos")]
    if bundled() {
        center::init();
    }
}

/// Show a notification; `on_click` runs (on another thread) if the user clicks it.
pub fn show(title: &str, subtitle: Option<&str>, body: &str, on_click: impl FnOnce() + Send + 'static) {
    // Outside the app bundle drift has no identity of its own on macOS: a notification
    // would come from an app called "use_default", and a click on it would ask where
    // that app is. There, only the toasts in the window tell of new messages.
    #[cfg(target_os = "macos")]
    if bundled() {
        center::show(title, subtitle, body, Box::new(on_click));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let body = match subtitle {
            Some(subtitle) => format!("{subtitle}: {body}"),
            None => body.to_string(),
        };
        let title = title.to_string();
        let spawned = std::thread::Builder::new().name("notification".into()).spawn(move || {
            let mut n = notify_rust::Notification::new();
            n.summary(&title).body(&body).appname("drift");
            match n.show() {
                #[cfg(unix)]
                Ok(handle) => handle.wait_for_action(|action| {
                    if action == "default" {
                        on_click();
                    }
                }),
                #[cfg(windows)]
                Ok(_) => drop(on_click),
                Err(e) => log_warn!("could not show a notification: {e}"),
            }
        });
        if let Err(e) = spawned {
            log_warn!("could not show a notification: {e}");
        }
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
        tile.display();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = count;
}

/// Notifications through the user notification center, for the app bundle.
#[cfg(target_os = "macos")]
mod center {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{AllocAnyThread, define_class, msg_send};
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
        UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    type OnClick = Box<dyn FnOnce() + Send>;

    /// What a click on each notification still up does, by its identifier.
    static CLICKS: Mutex<Option<HashMap<String, OnClick>>> = Mutex::new(None);
    static NEXT: AtomicU64 = AtomicU64::new(0);

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "DriftNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                // drift only notifies while it's not in front; show those that land after it came forward too.
                handler.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                handler: &block2::DynBlock<dyn Fn()>,
            ) {
                let id = response.notification().request().identifier().to_string();
                let on_click = CLICKS.lock().ok().and_then(|mut clicks| clicks.as_mut()?.remove(&id));
                if let Some(on_click) = on_click {
                    on_click();
                }
                handler.call(());
            }
        }
    );

    pub fn init() {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The center holds its delegate weakly; this one lives as long as drift.
        std::mem::forget(delegate);
        let done = RcBlock::new(|granted: Bool, _error: *mut NSError| {
            if !granted.as_bool() {
                log_warn!("notifications are not allowed for drift");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
            &done,
        );
    }

    pub fn show(title: &str, subtitle: Option<&str>, body: &str, on_click: OnClick) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        if let Some(subtitle) = subtitle {
            content.setSubtitle(&NSString::from_str(subtitle));
        }
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let id = format!("drift-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        if let Ok(mut clicks) = CLICKS.lock() {
            let clicks = clicks.get_or_insert_default();
            // Old notifications' clicks are dropped once there are many.
            if clicks.len() > 200 {
                clicks.clear();
            }
            clicks.insert(id.clone(), on_click);
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&id), &content, None);
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(error) = unsafe { error.as_ref() } {
                log_warn!("could not show a notification: {}", error.localizedDescription());
            }
        });
        UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, Some(&done));
    }
}
