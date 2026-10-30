//! The app's icon for the window and the Dock, when drift runs without its app bundle.

use eframe::egui;

const PNG: &[u8] = include_bytes!("../assets/icon/drift-window-256.png");

/// The icon as RGBA pixels (None if it can't be read, which would be a build mistake).
pub fn window_icon() -> Option<egui::IconData> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(frame.buffer_size());
    (frame.color_type == png::ColorType::Rgba).then_some(egui::IconData { rgba, width: frame.width, height: frame.height })
}
