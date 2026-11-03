//! How long since the user last touched the keyboard or mouse, anywhere (not only in
//! drift): Teams shows someone idle as away.

/// Seconds since the last input, where the system says.
#[cfg(target_os = "macos")]
pub fn seconds() -> Option<f64> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event: u32) -> f64;
    }
    /// `kCGEventSourceStateHIDSystemState`, `kCGAnyInputEventType`.
    const HID_SYSTEM: i32 = 1;
    const ANY_INPUT: u32 = u32::MAX;
    // SAFETY: a pure query with plain arguments.
    let idle = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM, ANY_INPUT) };
    idle.is_finite().then_some(idle)
}

#[cfg(not(target_os = "macos"))]
pub fn seconds() -> Option<f64> {
    None
}

/// Keep macOS from napping drift while its window is out of sight (App Nap holds back
/// a hidden app's timers, and the status would go away). The system may still sleep.
#[cfg(target_os = "macos")]
pub fn no_app_nap() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let reason = NSString::from_str("Keeping the chat and the status up to date");
    let activity =
        NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep, &reason);
    // For as long as drift runs.
    std::mem::forget(activity);
}

#[cfg(not(target_os = "macos"))]
pub fn no_app_nap() {}
