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
    pub fn new() -> Result<Self, String> {
        let block = Block::new_terminal(24, 80)?;
        let (panes, first_pane) = pane_grid::State::new(block);

        Ok(Tab {
            title: "Terminal".to_string(),
            panes,
            focus: Some(first_pane),
        })
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
