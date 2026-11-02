//! The macOS traffic lights, moved into the middle of drift's top bar: the system
//! puts them at the height of a usual title bar, above the bar's centre. The
//! container they're in is made taller and they sit at its bottom, as other apps
//! with their own title bar do; it's done again whenever the window's size changes.

/// Put the buttons' centres `center` points below the window's top, the first one
/// `left` points from its left edge.
#[cfg(target_os = "macos")]
pub fn place(frame: &eframe::Frame, center: f64, left: f64) {
    use objc2_app_kit::{NSView, NSWindowButton};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = frame.window_handle() else { return };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    // SAFETY: winit's AppKit handle is the window's content view, alive while the window is.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(window) = view.window() else { return };
    let buttons = [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton]
        .map(|b| window.standardWindowButton(b));
    let [Some(close), Some(minimize), Some(zoom)] = buttons else { return };
    // SAFETY: the buttons sit in the title bar view, inside its container.
    let Some(title_bar) = (unsafe { close.superview() }) else { return };
    let Some(container) = (unsafe { title_bar.superview() }) else { return };
    // Tall enough for the buttons' middle to be `center` down when they sit at its bottom.
    let height = close.frame().size.height;
    let wanted = center + height / 2.0;
    let mut bar = container.frame();
    let top = window.frame().size.height - wanted;
    if (bar.size.height - wanted).abs() > 0.5 || (bar.origin.y - top).abs() > 0.5 {
        bar.size.height = wanted;
        bar.origin.y = top;
        container.setFrame(bar);
    }
    let inner = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(bar.size.width, wanted));
    if title_bar.frame() != inner {
        title_bar.setFrame(inner);
    }
    let y = if title_bar.isFlipped() { wanted - height } else { 0.0 };
    let step = minimize.frame().origin.x - close.frame().origin.x;
    for (i, button) in [close, minimize, zoom].iter().enumerate() {
        let at = NSPoint::new(left + i as f64 * step, y);
        if button.frame().origin != at {
            button.setFrameOrigin(at);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn place(_frame: &eframe::Frame, _center: f64, _left: f64) {}
