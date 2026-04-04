// Workspace crate — manages collections of Blocks inside pane_grid panes.
// Phase 2: introduces the Block abstraction, Tab system, and Tab bar UI.

pub mod block;
pub mod tab;
pub mod tab_bar;

pub use block::Block;
pub use tab::Tab;
pub use tab_bar::{tab_bar_view, TabBarAction};
