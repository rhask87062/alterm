// Workspace crate — manages collections of Blocks inside pane_grid panes.
// Phase 2: introduces the Block abstraction, Tab system, and Tab bar UI.
// Phase 3: adds AI chat blocks alongside terminal blocks.

pub mod ai_chat;
pub mod block;
pub mod command_palette;
pub mod keybindings;
pub mod sidebar;
pub mod tab;
pub mod tab_bar;

pub use ai_chat::AIChatState;
pub use block::{Block, CELL_HEIGHT, CELL_WIDTH};
pub use command_palette::CommandPalette;
pub use keybindings::{match_shortcut, Action};
pub use sidebar::{sidebar_view, SidebarAction};
pub use tab::Tab;
pub use tab_bar::{tab_bar_view, TabBarAction};
