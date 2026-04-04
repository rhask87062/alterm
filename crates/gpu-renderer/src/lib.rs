// GPU-accelerated renderer crate.
// Provides iced widget and glyphon/cosmic-text based text rendering.

pub mod colors;
pub mod grid;
pub mod widget;

/// Messages produced by the terminal renderer widget.
///
/// Currently empty; will be extended with mouse events in a later task.
#[derive(Debug, Clone)]
pub enum RendererMessage {}
