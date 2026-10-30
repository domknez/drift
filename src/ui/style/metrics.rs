//! Dimensions.

pub const WINDOW_SIZE: [f32; 2] = [1360.0, 820.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [860.0, 520.0];
pub const TOP_BAR: f32 = 56.0;
/// Room for the macOS traffic lights at the left of the top bar.
pub const LIGHTS: f32 = if cfg!(target_os = "macos") { 78.0 } else { 14.0 };
/// Space between the panels and around them.
pub const GAP: f32 = 10.0;
pub const PANEL_RADIUS: u8 = 16;
/// The list on the left.
pub const LIST_WIDTH: f32 = 300.0;
/// The thread on the right.
pub const THREAD_WIDTH: f32 = 380.0;
pub const HEADER: f32 = 64.0;
/// A list row: an avatar beside a name and a preview.
pub const ROW: f32 = 54.0;
pub const LIST_AVATAR: f32 = 36.0;
/// Avatars beside message bubbles.
pub const AVATAR: f32 = 32.0;
pub const PAGE_PAD: f32 = 22.0;
/// The widest a bubble gets.
pub const BUBBLE_MAX: f32 = 640.0;
pub const RADIUS: u8 = 10;
/// Messages by one author this close together share one bubble group (ms).
pub const GROUP_GAP: i64 = 5 * 60 * 1000;
