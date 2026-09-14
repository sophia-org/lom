//! Xilem-owned views and a direct, windowless Masonry host.

mod driver;
mod views;

pub use driver::{ContentTargetLayout, PreviewDriver, PreviewScene};
pub use views::{UiState, calendar_view, panel_view};

/// Bundled font bytes, explicitly registered without system font discovery.
pub const MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
/// Bold companion with the same provenance as the regular face.
pub const MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf");
