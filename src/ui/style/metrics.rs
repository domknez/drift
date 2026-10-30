//! Dimensions.

pub const WINDOW_SIZE: [f32; 2] = [1240.0, 780.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [760.0, 480.0];
pub const SIDEBAR_WIDTH: f32 = 296.0;
/// Room above the sidebar for the macOS traffic lights.
pub const TITLE_BAR: f32 = if cfg!(target_os = "macos") { 40.0 } else { 12.0 };
pub const HEADER: f32 = 56.0;
pub const ROW: f32 = 32.0;
/// Inset of rows inside the sidebar.
pub const SIDE_PAD: f32 = 8.0;
/// The widest the messages get, centred on wide windows.
pub const READING_WIDTH: f32 = 880.0;
pub const PAGE_PAD: f32 = 24.0;
pub const AVATAR: f32 = 34.0;
pub const REPLY_AVATAR: f32 = 26.0;
pub const RADIUS: u8 = 8;
pub const POST_RADIUS: u8 = 12;
/// Replies shown under a post before "Show earlier replies".
pub const REPLIES_SHOWN: usize = 2;
/// Messages by one author this close together share one header (ms).
pub const GROUP_GAP: i64 = 5 * 60 * 1000;
