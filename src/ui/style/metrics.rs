//! Dimensions.

pub const WINDOW_SIZE: [f32; 2] = [1360.0, 820.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [820.0, 500.0];
/// The icon rail on the far left.
pub const RAIL_WIDTH: f32 = 68.0;
pub const RAIL_BUTTON: f32 = 44.0;
/// The list beside the rail.
pub const LIST_WIDTH: f32 = 288.0;
/// The thread pane on the right.
pub const THREAD_WIDTH: f32 = 360.0;
/// Room above the rail for the macOS traffic lights.
pub const TITLE_BAR: f32 = if cfg!(target_os = "macos") { 40.0 } else { 12.0 };
pub const HEADER: f32 = 58.0;
pub const ROW: f32 = 32.0;
/// Rows' inset inside the list.
pub const SIDE_PAD: f32 = 8.0;
pub const PAGE_PAD: f32 = 20.0;
pub const AVATAR: f32 = 36.0;
pub const RADIUS: u8 = 8;
/// Messages by one author this close together share one header (ms).
pub const GROUP_GAP: i64 = 5 * 60 * 1000;
