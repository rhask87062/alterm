/// Tab — an independent workspace with its own pane layout.
///
/// Each tab owns a `pane_grid::State<Block>` so tabs have completely
/// independent tiling arrangements.
use iced::widget::pane_grid;

use crate::Block;

/// A single tab in the tab bar.
pub struct Tab {
    /// Human-readable title shown on the tab button.
    pub title: String,
    /// The pane-grid state that holds all blocks (panes) in this tab.
    pub panes: pane_grid::State<Block>,
    /// Which pane inside this tab is focused (if any).
    pub focus: Option<pane_grid::Pane>,
}

impl Tab {
    /// Create a new tab containing a single terminal pane.
    ///
    /// `pane_width` and `pane_height` are the estimated pixel dimensions
    /// available for the terminal content area.  If you don't know yet,
    /// use `Tab::new_default()`.
    pub fn new_with_size(pane_width: f32, pane_height: f32) -> Result<Self, String> {
        let (rows, cols) = Block::size_from_pixels(pane_width, pane_height);
        let block = Block::new_terminal(rows, cols)?;
        let (panes, first_pane) = pane_grid::State::new(block);

        Ok(Tab {
            title: "Terminal".to_string(),
            panes,
            focus: Some(first_pane),
        })
    }

    /// Create a new tab with a reasonable default size derived from a
    /// typical 900x600 window.
    pub fn new() -> Result<Self, String> {
        // 900 - 52 (sidebar) - 4 (borders/spacing)
        // 600 - 30 (tab bar) - 28 (pane title bar) - 4 (borders)
        Self::new_with_size(844.0, 538.0)
    }

    /// Create a new tab with a custom title.
    pub fn with_title(title: impl Into<String>) -> Result<Self, String> {
        let mut tab = Self::new()?;
        tab.title = title.into();
        Ok(tab)
    }

    /// Number of panes in this tab.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }
}
