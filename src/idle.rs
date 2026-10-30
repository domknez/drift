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
