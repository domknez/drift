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

/// What the system lets drift do with notifications, for `drift notifications`.
pub fn settings() -> String {
    #[cfg(target_os = "macos")]
    {
        if !bundled() {
            return "not run from drift.app: macOS notifications need the app bundle".into();
        }
        center::settings().unwrap_or_else(|| "macOS didn't say".into())
    }
    #[cfg(not(target_os = "macos"))]
    "shown through the desktop's notification service".into()
}

/// Show a notification; `on_click` runs (on another thread) if the user clicks it.
pub fn show(title: &str, subtitle: Option<&str>, body: &str, on_click: impl FnOnce() + Send + 'static) {
    // macOS shows nothing for a notification without a body (a bot's card, say).
    let body = if body.trim().is_empty() { "New message" } else { body };
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
        // Some Linux notification servers read the body as markup.
        #[cfg(unix)]
        let (title, body) = (escape_markup(title), escape_markup(&body));
        let title = title.to_string();
        let spawned = std::thread::Builder::new().name("notification".into()).spawn(move || {
            let mut n = notify_rust::Notification::new();
            n.summary(&title).body(&body).appname("drift");
            // The desktop's own sound for a new message (freedesktop's sound theme).
            #[cfg(unix)]
            n.sound_name("message-new-instant");
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

/// drift's sound for a new message: its own, made by `scripts/chime.py`.
#[cfg(target_os = "macos")]
const CHIME: &[u8] = include_bytes!("../assets/sounds/message.wav");

/// Play the sound for a new message, for those shown in the window (system
/// notifications bring their own). Call from the main thread.
pub fn chime() {
    #[cfg(target_os = "macos")]
    {
        use objc2::AllocAnyThread;
        use objc2_app_kit::NSSound;
        use objc2_foundation::NSData;
        thread_local! {
            // Kept while it plays (and for the next time).
            static SOUND: Option<objc2::rc::Retained<NSSound>> =
                NSSound::initWithData(NSSound::alloc(), &NSData::with_bytes(CHIME));
        }
        SOUND.with(|sound| {
            if let Some(sound) = sound {
                sound.stop();
                sound.play();
            }
        });
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
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
    /// When this run began (Unix milliseconds), for identifiers no earlier run used.
    static STARTED: std::sync::OnceLock<u128> = std::sync::OnceLock::new();

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
        let done = RcBlock::new(|granted: Bool, error: *mut NSError| {
            if let Some(error) = unsafe { error.as_ref() } {
                log_warn!("couldn't ask to notify: {}", error.localizedDescription());
            } else if !granted.as_bool() {
                log_warn!("notifications are not allowed for drift");
            }
            log_info!("notifications: {}", settings().unwrap_or_else(|| "unknown".into()));
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
            &done,
        );
    }

    /// What macOS lets drift do, as System Settings → Notifications has it.
    pub fn settings() -> Option<String> {
        use objc2_user_notifications::{UNAlertStyle, UNAuthorizationStatus, UNNotificationSetting, UNNotificationSettings};
        let (tx, rx) = std::sync::mpsc::channel();
        let done = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
            let s = unsafe { settings.as_ref() };
            let status = match s.authorizationStatus() {
                UNAuthorizationStatus::NotDetermined => "not asked yet",
                UNAuthorizationStatus::Denied => "not allowed",
                UNAuthorizationStatus::Authorized => "allowed",
                UNAuthorizationStatus::Provisional => "allowed quietly (provisional)",
                _ => "other",
            };
            let style = match s.alertStyle() {
                UNAlertStyle::None => "none",
                UNAlertStyle::Banner => "banners",
                UNAlertStyle::Alert => "alerts",
                _ => "other",
            };
            let on = |setting: UNNotificationSetting| if setting == UNNotificationSetting::Enabled { "on" } else { "off" };
            let _ = tx.send(format!(
                "{status}; style: {style}; alerts {}, sound {}, in Notification Center {}",
                on(s.alertSetting()),
                on(s.soundSetting()),
                on(s.notificationCenterSetting())
            ));
        });
        UNUserNotificationCenter::currentNotificationCenter().getNotificationSettingsWithCompletionHandler(&done);
        rx.recv_timeout(std::time::Duration::from_secs(5)).ok()
    }

    pub fn show(title: &str, subtitle: Option<&str>, body: &str, on_click: OnClick) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        if let Some(subtitle) = subtitle {
            content.setSubtitle(&NSString::from_str(subtitle));
        }
        content.setBody(&NSString::from_str(body));
        // drift's own sound (in the app bundle), as Teams has its own.
        content.setSound(Some(&UNNotificationSound::soundNamed(&NSString::from_str("message.wav"))));
        // Unique across runs too: a notification with the identifier of one still in
        // Notification Center (from an earlier run) replaces it quietly, with no banner.
        let started =
            *STARTED.get_or_init(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()));
        let id = format!("drift-{started}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
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
