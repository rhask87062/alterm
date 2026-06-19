// Workspace crate — manages collections of Blocks inside pane_grid panes.
// Phase 2: introduces the Block abstraction, Tab system, and Tab bar UI.
// Phase 3: adds AI chat blocks alongside terminal blocks.

pub mod ai_chat;
pub mod block;
pub mod chrome;
pub mod command_palette;
pub mod grid;
pub mod keybindings;
pub mod session;
pub mod settings_panel;
pub mod sidebar;
pub mod tab;
pub mod tab_bar;

pub use ai_chat::AIChatState;
pub use block::{Block, CELL_HEIGHT, CELL_WIDTH};
pub use terminal::term::{build_search_pattern, SearchMatch};
pub use browser::BrowserState;
pub use preview::PreviewState;
pub use command_palette::CommandPalette;
pub use grid::grid_dims;
pub use keybindings::{all_palette_actions, match_shortcut, Action};
pub use settings_panel::{SettingsField, SettingsSection, SettingsState};
pub use sidebar::{sidebar_view, SidebarAction};
pub use tab::Tab;
pub use tab_bar::{tab_bar_view, TabBarAction};
