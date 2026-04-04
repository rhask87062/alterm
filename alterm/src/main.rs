use std::time::Duration;

use iced::event::Status;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{
    button, column, container, opaque, pane_grid, row, scrollable, stack, text, text_input,
    Column,
};
use iced::window;
use iced::{Background, Border, Color, Element, Event, Fill, Length, Padding, Subscription, Task, Theme};

use gpu_renderer::widget::TerminalView;
use workspace::{
    match_shortcut, sidebar_view, tab_bar_view, Action, Block, CommandPalette, SidebarAction, Tab,
    TabBarAction, CELL_HEIGHT,
};

use ai::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAIProvider, Provider,
    ProviderConfig, StreamEvent,
};
use altermative_config::AppConfig;

fn main() -> iced::Result {
    env_logger::init();

    iced::application(Altermative::new, Altermative::update, Altermative::view)
        .title("Altermative")
        .theme(Theme::Dark)
        .window_size((900.0, 600.0))
        .subscription(Altermative::subscription)
        .run()
}

/// Estimated height consumed by the tab bar (padding + button + padding).
const TAB_BAR_HEIGHT: f32 = 33.0;
/// Estimated height consumed by each pane's title bar (padding 4 + text 12 + padding 4 + border).
const PANE_TITLE_BAR_HEIGHT: f32 = 28.0;
/// Width of the sidebar.
const SIDEBAR_WIDTH: f32 = 44.0;
/// Spacing between panes — must match `.spacing(2)` on the PaneGrid widget.
const PANE_GRID_SPACING: f32 = 2.0;
/// Minimum pane size — must match `.min_size(120)` on the PaneGrid widget.
const PANE_GRID_MIN_SIZE: f32 = 120.0;

struct Altermative {
    tabs: Vec<Tab>,
    active_tab: usize,
    palette: CommandPalette,
    /// Accumulated touchpad scroll pixels (touchpads send many tiny deltas)
    scroll_accumulator: f32,
    /// Current window dimensions in logical pixels.
    window_width: f32,
    window_height: f32,
    /// Application configuration (loaded from disk at startup).
    config: AppConfig,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    KeyboardInput(Key, Modifiers),
    MouseScroll(f32),
    ClipboardContent(Option<String>),
    PaneClicked(pane_grid::Pane),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    MaximizeToggle,
    // Per-pane title bar controls
    SplitPaneRight(pane_grid::Pane),
    SplitPaneDown(pane_grid::Pane),
    ClosePaneId(pane_grid::Pane),
    MaximizeTogglePane(pane_grid::Pane),
    // Tab management
    NewTab,
    CloseTab(usize),
    SelectTab(usize),
    TabBarAction(TabBarAction),
    // Sidebar
    SidebarAction(SidebarAction),
    SidebarNewTerminal,
    // Command palette
    PaletteQueryChanged(String),
    PaletteSubmit,
    // Window resize
    WindowResized(f32, f32),
    // AI chat messages
    AIInputChanged(pane_grid::Pane, String),
    AISendMessage(pane_grid::Pane),
    AIStreamToken(pane_grid::Pane, String),
    AIStreamDone(pane_grid::Pane),
    AIStreamError(pane_grid::Pane, String),
    ToggleAIChat,
}

impl Altermative {
    fn new() -> (Self, Task<Message>) {
        let window_width = 900.0_f32;
        let window_height = 600.0_f32;

        // Load config from default path.
        let config = AppConfig::load(&AppConfig::config_path()).unwrap_or_else(|e| {
            log::warn!("Failed to load config: {e}, using defaults");
            AppConfig::default()
        });

        // Initial size estimate for a single-pane tab at launch.
        // resize_all_panes() will correct this once the window opens.
        let grid_width = (window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (window_height - TAB_BAR_HEIGHT).max(40.0);
        let content_height = (grid_height - PANE_TITLE_BAR_HEIGHT).max(CELL_HEIGHT * 2.0);

        let first_tab = Tab::new_with_size(grid_width, content_height)
            .expect("Failed to create initial tab");

        let app = Altermative {
            tabs: vec![first_tab],
            active_tab: 0,
            palette: CommandPalette::new(),
            scroll_accumulator: 0.0,
            window_width,
            window_height,
            config,
        };

        (app, Task::none())
    }

    /// Get a reference to the active tab.
    fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// Get a mutable reference to the active tab.
    fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    /// Move focus to the adjacent pane in the given direction.
    fn focus_adjacent(&mut self, direction: pane_grid::Direction) {
        let tab = self.active_tab_mut();
        if let Some(focused) = tab.focus {
            if let Some(adjacent) = tab.panes.adjacent(focused, direction) {
                tab.focus = Some(adjacent);
            }
        }
    }

    /// Resize every terminal in the active tab to its exact pixel dimensions.
    fn resize_all_panes(&mut self) {
        use iced::Size;

        let grid_width = (self.window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (self.window_height - TAB_BAR_HEIGHT).max(40.0);
        let bounds = Size::new(grid_width, grid_height);

        let tab = self.active_tab_mut();
        let regions = tab.panes.layout().pane_regions(
            PANE_GRID_SPACING,
            PANE_GRID_MIN_SIZE,
            bounds,
        );

        for (pane, rect) in &regions {
            let content_width = rect.width;
            let content_height = (rect.height - PANE_TITLE_BAR_HEIGHT).max(CELL_HEIGHT * 2.0);
            let (rows, cols) = Block::size_from_pixels(content_width, content_height);
            if let Some(block) = tab.panes.get_mut(*pane) {
                let (cur_rows, cur_cols) = block.dimensions();
                if cur_rows != rows || cur_cols != cols {
                    block.resize(rows, cols);
                }
            }
        }
    }

    /// Scroll the focused pane by the given number of lines.
    fn scroll_focused(&mut self, lines: i32) {
        let tab = self.active_tab_mut();
        if let Some(focused) = tab.focus {
            if let Some(block) = tab.panes.get_mut(focused) {
                block.scroll(lines);
            }
        }
    }

    /// Get the recent terminal output from any terminal pane in the active tab.
    /// Prefers the focused pane if it's a terminal; otherwise finds the first terminal.
    fn terminal_context(&self, lines: usize) -> Option<String> {
        let tab = self.active_tab();

        // Try the focused pane first.
        if let Some(focused) = tab.focus {
            if let Some(block) = tab.panes.get(focused) {
                if let Some(output) = block.recent_output(lines) {
                    return Some(output);
                }
            }
        }

        // Fall back to any terminal pane in the tab.
        for (_pane, block) in tab.panes.iter() {
            if let Some(output) = block.recent_output(lines) {
                return Some(output);
            }
        }

        None
    }

    /// Build a `ProviderConfig` from the app config for the given provider name.
    fn provider_config(&self, provider_name: &str) -> Option<ProviderConfig> {
        let ai_cfg = &self.config.ai;
        let entry = match provider_name {
            "openai" => ai_cfg.providers.openai.as_ref(),
            "anthropic" => ai_cfg.providers.anthropic.as_ref(),
            "gemini" => ai_cfg.providers.gemini.as_ref(),
            "grok" => ai_cfg.providers.grok.as_ref(),
            "lmstudio" => ai_cfg.providers.lmstudio.as_ref(),
            "ollama" => ai_cfg.providers.ollama.as_ref(),
            _ => None,
        }?;

        Some(ProviderConfig {
            base_url: entry.resolved_base_url(provider_name),
            api_key: entry.api_key.clone(),
            model: entry.model.clone(),
            max_tokens: ai_cfg.max_tokens,
            temperature: ai_cfg.temperature,
            system_prompt: Some(ai_cfg.system_prompt.clone()),
        })
    }

    /// Dispatch a keybinding [`Action`] into the appropriate [`Message`].
    fn dispatch_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::NewTab => self.update(Message::NewTab),
            Action::CloseTab => {
                let idx = self.active_tab;
                self.update(Message::CloseTab(idx))
            }
            Action::NextTab => {
                if self.tabs.len() > 1 {
                    let next = (self.active_tab + 1) % self.tabs.len();
                    self.update(Message::SelectTab(next))
                } else {
                    Task::none()
                }
            }
            Action::PrevTab => {
                if self.tabs.len() > 1 {
                    let prev = if self.active_tab == 0 {
                        self.tabs.len() - 1
                    } else {
                        self.active_tab - 1
                    };
                    self.update(Message::SelectTab(prev))
                } else {
                    Task::none()
                }
            }
            Action::JumpToTab(n) => {
                let idx = n - 1;
                if idx < self.tabs.len() {
                    self.update(Message::SelectTab(idx))
                } else {
                    Task::none()
                }
            }
            Action::RenameTab => {
                log::debug!("RenameTab — not yet implemented");
                Task::none()
            }
            Action::SplitRight => self.update(Message::SplitHorizontal),
            Action::SplitDown => self.update(Message::SplitVertical),
            Action::ClosePane => self.update(Message::ClosePane),
            Action::MaximizeToggle => self.update(Message::MaximizeToggle),
            Action::FocusUp => {
                self.focus_adjacent(pane_grid::Direction::Up);
                Task::none()
            }
            Action::FocusDown => {
                self.focus_adjacent(pane_grid::Direction::Down);
                Task::none()
            }
            Action::FocusLeft => {
                self.focus_adjacent(pane_grid::Direction::Left);
                Task::none()
            }
            Action::FocusRight => {
                self.focus_adjacent(pane_grid::Direction::Right);
                Task::none()
            }
            Action::CommandPalette => {
                self.palette.toggle();
                Task::none()
            }
            Action::OpenSettings => {
                log::debug!("OpenSettings — not yet implemented");
                Task::none()
            }
            Action::ToggleAIChat => self.update(Message::ToggleAIChat),
            Action::Copy => {
                log::debug!("Copy — not yet implemented");
                Task::none()
            }
            Action::Paste => {
                iced::clipboard::read().map(Message::ClipboardContent)
            }
            Action::ScrollUp => {
                self.scroll_focused(3);
                Task::none()
            }
            Action::ScrollDown => {
                self.scroll_focused(-3);
                Task::none()
            }
            Action::ScrollPageUp => {
                let rows = self.active_tab().panes.iter().next()
                    .map(|(_, b)| b.dimensions().0 as i32 / 2)
                    .unwrap_or(12);
                self.scroll_focused(rows);
                Task::none()
            }
            Action::ScrollPageDown => {
                let rows = self.active_tab().panes.iter().next()
                    .map(|(_, b)| b.dimensions().0 as i32 / 2)
                    .unwrap_or(12);
                self.scroll_focused(-rows);
                Task::none()
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Tick all panes in all tabs.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        block.tick();
                    }
                }
            }
            Message::PaneClicked(pane) => {
                self.active_tab_mut().focus = Some(pane);
            }
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.active_tab_mut().panes.drop(pane, target);
                self.resize_all_panes();
            }
            Message::PaneDragged(_) => {
                // Picked / Canceled — nothing to do.
            }
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.active_tab_mut().panes.resize(split, ratio);
                self.resize_all_panes();
            }
            Message::WindowResized(width, height) => {
                self.window_width = width;
                self.window_height = height;
                self.resize_all_panes();
            }
            Message::SplitHorizontal => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    // Halve cols for a vertical-axis split (left|right).
                    let (rows, cols) = tab.panes.get(focused)
                        .map(|b| b.dimensions()).unwrap_or((24, 80));
                    let half_cols = (cols / 2).max(20);
                    if let Ok(block) = Block::new_terminal(rows, half_cols) {
                        if let Some((new_pane, _split)) =
                            tab.panes.split(pane_grid::Axis::Vertical, focused, block)
                        {
                            // Resize the original pane too.
                            if let Some(old_block) = tab.panes.get_mut(focused) {
                                old_block.resize(rows, half_cols);
                            }
                            tab.focus = Some(new_pane);
                        }
                    }
                }
                self.resize_all_panes();
            }
            Message::SplitVertical => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    // Halve rows for a horizontal-axis split (top/bottom).
                    let (rows, cols) = tab.panes.get(focused)
                        .map(|b| b.dimensions()).unwrap_or((24, 80));
                    let half_rows = (rows / 2).max(4);
                    if let Ok(block) = Block::new_terminal(half_rows, cols) {
                        if let Some((new_pane, _split)) =
                            tab.panes.split(pane_grid::Axis::Horizontal, focused, block)
                        {
                            if let Some(old_block) = tab.panes.get_mut(focused) {
                                old_block.resize(half_rows, cols);
                            }
                            tab.focus = Some(new_pane);
                        }
                    }
                }
                self.resize_all_panes();
            }
            Message::ClosePane => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if tab.panes.len() > 1 {
                        if let Some((_closed_block, sibling)) = tab.panes.close(focused) {
                            tab.focus = Some(sibling);
                        }
                    }
                }
                self.resize_all_panes();
            }
            Message::MaximizeToggle => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if tab.panes.maximized().is_some() {
                        tab.panes.restore();
                    } else {
                        tab.panes.maximize(focused);
                    }
                }
            }

            // Per-pane title bar controls (operate on a specific pane)
            Message::SplitPaneRight(pane) => {
                let tab = self.active_tab_mut();
                let (rows, cols) = tab.panes.get(pane)
                    .map(|b| b.dimensions()).unwrap_or((24, 80));
                let half_cols = (cols / 2).max(20);
                if let Ok(block) = Block::new_terminal(rows, half_cols) {
                    if let Some((new_pane, _split)) =
                        tab.panes.split(pane_grid::Axis::Vertical, pane, block)
                    {
                        if let Some(old_block) = tab.panes.get_mut(pane) {
                            old_block.resize(rows, half_cols);
                        }
                        tab.focus = Some(new_pane);
                    }
                }
                self.resize_all_panes();
            }
            Message::SplitPaneDown(pane) => {
                let tab = self.active_tab_mut();
                let (rows, cols) = tab.panes.get(pane)
                    .map(|b| b.dimensions()).unwrap_or((24, 80));
                let half_rows = (rows / 2).max(4);
                if let Ok(block) = Block::new_terminal(half_rows, cols) {
                    if let Some((new_pane, _split)) =
                        tab.panes.split(pane_grid::Axis::Horizontal, pane, block)
                    {
                        if let Some(old_block) = tab.panes.get_mut(pane) {
                            old_block.resize(half_rows, cols);
                        }
                        tab.focus = Some(new_pane);
                    }
                }
                self.resize_all_panes();
            }
            Message::ClosePaneId(pane) => {
                let tab = self.active_tab_mut();
                if tab.panes.len() > 1 {
                    if let Some((_closed_block, sibling)) = tab.panes.close(pane) {
                        tab.focus = Some(sibling);
                    }
                }
                self.resize_all_panes();
            }
            Message::MaximizeTogglePane(pane) => {
                let tab = self.active_tab_mut();
                if tab.panes.maximized().is_some() {
                    tab.panes.restore();
                } else {
                    tab.panes.maximize(pane);
                }
                self.resize_all_panes();
            }

            // -- Tab management --
            Message::NewTab => {
                if let Ok(new_tab) = Tab::new() {
                    self.tabs.push(new_tab);
                    self.active_tab = self.tabs.len() - 1;
                }
            }
            Message::CloseTab(index) => {
                if self.tabs.len() > 1 && index < self.tabs.len() {
                    self.tabs.remove(index);
                    // Adjust active_tab index after removal.
                    if self.active_tab >= self.tabs.len() {
                        self.active_tab = self.tabs.len() - 1;
                    } else if self.active_tab > index {
                        self.active_tab -= 1;
                    }
                }
            }
            Message::SelectTab(index) => {
                if index < self.tabs.len() {
                    self.active_tab = index;
                    self.resize_all_panes();
                }
            }
            Message::TabBarAction(action) => match action {
                TabBarAction::Select(i) => return self.update(Message::SelectTab(i)),
                TabBarAction::Close(i) => return self.update(Message::CloseTab(i)),
                TabBarAction::New => return self.update(Message::NewTab),
            },
            Message::SidebarAction(action) => match action {
                SidebarAction::NewTerminal => {
                    return self.update(Message::SidebarNewTerminal);
                }
                SidebarAction::NewAiChat => {
                    return self.update(Message::ToggleAIChat);
                }
            },
            Message::SidebarNewTerminal => {
                // Split the focused pane with a new terminal (right).
                return self.update(Message::SplitHorizontal);
            }

            // -- AI Chat --
            Message::ToggleAIChat => {
                let provider_name = self.config.ai.default_provider.clone();

                // Find the model for this provider.
                let model_name = match provider_name.as_str() {
                    "openai" => self.config.ai.providers.openai.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "gpt-4o".to_string()),
                    "anthropic" => self.config.ai.providers.anthropic.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
                    "gemini" => self.config.ai.providers.gemini.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "gemini-2.0-flash".to_string()),
                    "grok" => self.config.ai.providers.grok.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "grok-2".to_string()),
                    "lmstudio" => self.config.ai.providers.lmstudio.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "local-model".to_string()),
                    "ollama" => self.config.ai.providers.ollama.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "llama3.2".to_string()),
                    _ => "unknown".to_string(),
                };

                let block = Block::new_ai_chat(provider_name, model_name);
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if let Some((new_pane, _split)) =
                        tab.panes.split(pane_grid::Axis::Vertical, focused, block)
                    {
                        tab.focus = Some(new_pane);
                    }
                }
                self.resize_all_panes();
            }

            Message::AIInputChanged(pane, value) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.input = value;
                }
            }
            Message::AISendMessage(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    let txt = state.input.trim().to_string();
                    if txt.is_empty() {
                        return Task::none();
                    }
                    state.input.clear();
                    state.add_user_message(txt);
                    state.start_streaming();
                } else {
                    return Task::none();
                }

                // Check if provider is configured.
                let provider_name = if let Some(Block::AIChat { state }) = self.active_tab().panes.get(pane) {
                    state.provider_name.clone()
                } else {
                    return Task::none();
                };

                let provider_cfg = match self.provider_config(&provider_name) {
                    Some(c) => c,
                    None => {
                        return Task::done(Message::AIStreamError(
                            pane,
                            format!(
                                "No API key configured for '{provider_name}'. \
                                 Add one in Settings (Ctrl+Shift+,) or edit \
                                 ~/.config/altermative/config.toml"
                            ),
                        ));
                    }
                };

                // Build messages for the API.
                let api_messages = if let Some(Block::AIChat { state }) = self.active_tab().panes.get(pane) {
                    state.chat_messages_for_api()
                } else {
                    return Task::none();
                };

                // Inject terminal context into the system prompt.
                let mut config = provider_cfg;
                if let Some(context) = self.terminal_context(50) {
                    let system = config.system_prompt.unwrap_or_default();
                    config.system_prompt = Some(format!(
                        "{system}\n\nHere is the user's recent terminal output:\n```\n{context}\n```"
                    ));
                }

                // Spawn a streaming task.
                let pname = provider_name.clone();
                return Task::stream(async_stream(pane, pname, config, api_messages));
            }

            Message::AIStreamToken(pane, token) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.append_token(token);
                }
            }
            Message::AIStreamDone(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.finish_streaming();
                }
            }
            Message::AIStreamError(pane, err) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.set_error(err);
                }
            }

            // Command palette messages
            Message::PaletteQueryChanged(query) => {
                self.palette.update_query(query);
            }
            Message::PaletteSubmit => {
                if let Some(action) = self.palette.execute() {
                    return self.dispatch_action(action);
                }
            }

            Message::KeyboardInput(key, modifiers) => {
                // When the palette is open, intercept navigation keys.
                if self.palette.visible {
                    match &key {
                        Key::Named(Named::Escape) => {
                            self.palette.close();
                            return Task::none();
                        }
                        Key::Named(Named::ArrowUp) => {
                            self.palette.select_prev();
                            return Task::none();
                        }
                        Key::Named(Named::ArrowDown) => {
                            self.palette.select_next();
                            return Task::none();
                        }
                        Key::Named(Named::Enter) => {
                            return self.update(Message::PaletteSubmit);
                        }
                        _ => {
                            // Let Ctrl+Shift+P toggle the palette off.
                            if let Some(Action::CommandPalette) = match_shortcut(&key, &modifiers) {
                                self.palette.close();
                                return Task::none();
                            }
                            // All other keys are handled by the text_input widget.
                            return Task::none();
                        }
                    }
                }

                // Route through the keybinding registry.
                if let Some(action) = match_shortcut(&key, &modifiers) {
                    return self.dispatch_action(action);
                }

                // If the focused pane is an AI chat, don't forward keyboard input
                // to a PTY. The text_input widget handles it.
                {
                    let tab = self.active_tab();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get(focused) {
                            if block.is_ai_chat() {
                                return Task::none();
                            }
                        }
                    }
                }

                // Reset cursor blink on keypress.
                {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            block.reset_cursor_blink();
                        }
                    }
                }

                // Forward to focused terminal.
                if let Some(bytes) = key_to_bytes(&key, &modifiers) {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            block.write_input(&bytes);
                        }
                    }
                }
            }
            Message::ClipboardContent(content) => {
                if let Some(text) = content {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            let mut paste_bytes = Vec::new();
                            paste_bytes.extend_from_slice(b"\x1b[200~");
                            paste_bytes.extend_from_slice(text.as_bytes());
                            paste_bytes.extend_from_slice(b"\x1b[201~");
                            block.write_input(&paste_bytes);
                        }
                    }
                }
            }
            Message::MouseScroll(delta_y) => {
                // Accumulate small touchpad deltas until they reach a full line
                self.scroll_accumulator += delta_y;
                let lines = self.scroll_accumulator as i32;
                if lines != 0 {
                    self.scroll_accumulator -= lines as f32;
                    self.scroll_focused(lines);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let tab = self.active_tab();
        let focus = tab.focus;
        let total_panes = tab.panes.len();

        // Tab bar
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
        let tab_bar = tab_bar_view(&titles, self.active_tab, Message::TabBarAction);

        // Pane grid for the active tab
        let is_maximized = tab.panes.maximized().is_some();
        let pane_grid_widget =
            pane_grid::PaneGrid::new(&tab.panes, |pane, block, _maximized| {
                let is_focused = focus == Some(pane);

                // Build content based on block type.
                let content: Element<'_, Message> = match block {
                    Block::Terminal { .. } => {
                        let grid = block.render_grid();
                        let terminal_view = TerminalView::new(grid);
                        terminal_view.view()
                    }
                    Block::AIChat { state } => {
                        ai_chat_view(pane, state)
                    }
                };

                // Title bar with control buttons.
                let title = text(block.title()).size(12);

                // Build control buttons row
                let split_right_btn = title_bar_button("|", Message::SplitPaneRight(pane));
                let split_down_btn = title_bar_button("\u{2014}", Message::SplitPaneDown(pane));
                let maximize_label = if is_maximized { "\u{29C9}" } else { "\u{25A1}" };
                let maximize_btn = title_bar_button(maximize_label, Message::MaximizeTogglePane(pane));

                let controls: Element<'_, Message> = if total_panes > 1 {
                    let close_btn = title_bar_button("\u{00D7}", Message::ClosePaneId(pane));
                    row![split_right_btn, split_down_btn, maximize_btn, close_btn]
                        .spacing(2)
                        .align_y(iced::Alignment::Center)
                        .into()
                } else {
                    row![split_right_btn, split_down_btn, maximize_btn]
                        .spacing(2)
                        .align_y(iced::Alignment::Center)
                        .into()
                };

                let title_bar = pane_grid::TitleBar::new(title)
                    .controls(controls)
                    .padding(4)
                    .style(move |theme: &Theme| title_bar_style(theme, is_focused));

                pane_grid::Content::new(content)
                    .title_bar(title_bar)
                    .style(move |theme: &Theme| pane_content_style(theme, is_focused))
            })
            .on_click(Message::PaneClicked)
            .on_drag(Message::PaneDragged)
            .on_resize(10, Message::PaneResized)
            .spacing(2)
            .min_size(120)
            .width(Fill)
            .height(Fill);

        // Sidebar
        let sidebar = sidebar_view(Message::SidebarAction);

        // Layout: tab bar on top, then [pane_grid | sidebar] below
        let content_row = row![pane_grid_widget, sidebar];
        let layout = column![tab_bar, content_row];

        let base: Element<'_, Message> = container(layout)
            .width(Fill)
            .height(Fill)
            .into();

        // Command palette overlay
        if self.palette.visible {
            let overlay = self.palette_overlay();
            stack![base, opaque(overlay)].into()
        } else {
            base
        }
    }

    /// Build the command palette overlay widget.
    fn palette_overlay(&self) -> Element<'_, Message> {
        // Search input
        let input = text_input("Type a command...", &self.palette.query)
            .on_input(Message::PaletteQueryChanged)
            .on_submit(Message::PaletteSubmit)
            .size(14)
            .padding(8);

        // Command list
        let commands = self.palette.visible_commands();
        let selected = self.palette.selected;

        let mut items: Vec<Element<'_, Message>> = Vec::new();
        for (i, cmd) in commands.iter().enumerate() {
            let is_selected = i == selected;
            let bg_color = if is_selected {
                Color::from_rgb(0.20, 0.30, 0.50)
            } else {
                Color::from_rgb(0.12, 0.12, 0.15)
            };
            let text_color = if is_selected {
                Color::from_rgb(1.0, 1.0, 1.0)
            } else {
                Color::from_rgb(0.75, 0.75, 0.75)
            };

            let label = text(&cmd.label).size(13).color(text_color);
            let shortcut = text(&cmd.shortcut).size(11).color(
                if is_selected {
                    Color::from_rgb(0.7, 0.8, 1.0)
                } else {
                    Color::from_rgb(0.45, 0.45, 0.50)
                },
            );

            let item_row = row![label, iced::widget::space().width(Fill), shortcut]
                .spacing(8)
                .align_y(iced::Alignment::Center);

            let item_container: Element<'_, Message> = container(item_row)
                .width(Fill)
                .padding(6)
                .style(move |_theme: &Theme| iced::widget::container::Style {
                    background: Some(Background::Color(bg_color)),
                    ..Default::default()
                })
                .into();

            items.push(item_container);
        }

        let list = Column::from_vec(items).spacing(1);

        // Wrap the list in a scrollable-like container (limited height).
        let list_container = container(list)
            .max_height(300)
            .width(Fill);

        // The palette box
        let palette_box = column![input, list_container]
            .spacing(2)
            .width(Length::Fixed(450.0));

        let palette_styled = container(palette_box)
            .padding(4)
            .style(|_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgb(0.10, 0.10, 0.13))),
                border: Border {
                    color: Color::from_rgb(0.30, 0.45, 0.75),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });

        // Center horizontally, place near top
        container(
            container(palette_styled)
                .center_x(Fill)
                .padding(Padding { top: 60.0, right: 0.0, bottom: 0.0, left: 0.0 }),
        )
        .width(Fill)
        .height(Fill)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
            ..Default::default()
        })
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(8)).map(|_| Message::Tick);

        let events =
            iced::event::listen_with(|event, status, _window: window::Id| {
                match &event {
                    Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                        let y = match delta {
                            iced::mouse::ScrollDelta::Lines { y, .. } => *y * 3.0,
                            iced::mouse::ScrollDelta::Pixels { y, .. } => *y / 6.0, // touchpad: ~6px per line
                        };
                        if y.abs() > 0.01 {
                            return Some(Message::MouseScroll(y));
                        }
                    }
                    Event::Window(iced::window::Event::Resized(size)) => {
                        return Some(Message::WindowResized(size.width, size.height));
                    }
                    _ => {}
                }

                if status == Status::Captured {
                    return None;
                }
                match event {
                    Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        text: _,
                        ..
                    }) => Some(Message::KeyboardInput(key, modifiers)),
                    _ => None,
                }
            });

        Subscription::batch([tick, events])
    }
}

// ---------------------------------------------------------------------------
// AI Chat view
// ---------------------------------------------------------------------------

/// Build the AI chat view for a pane.
fn ai_chat_view<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::AIChatState,
) -> Element<'a, Message> {
    // Header: provider / model
    let header_text = text(format!(
        "Provider: {} / {}",
        state.provider_name, state.model_name
    ))
    .size(11)
    .color(Color::from_rgb(0.55, 0.60, 0.70));

    let header = container(header_text)
        .width(Fill)
        .padding(Padding::from([6, 10]))
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgb(0.08, 0.08, 0.11))),
            border: Border {
                color: Color::from_rgb(0.15, 0.15, 0.20),
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

    // Chat messages
    let mut message_elements: Vec<Element<'a, Message>> = Vec::new();

    if state.messages.is_empty() && !state.streaming {
        // Empty state — show a helpful hint.
        let hint = text("Ask a question about your terminal output, get help with commands, \
                         or chat about anything. The AI can see your recent terminal output for context.")
            .size(12)
            .color(Color::from_rgb(0.40, 0.40, 0.45));
        message_elements.push(
            container(hint)
                .width(Fill)
                .padding(Padding::from([20, 12]))
                .into(),
        );
    }

    for msg in &state.messages {
        let (label, label_color, content_color, bg) = match msg.role.as_str() {
            "user" => (
                "You",
                Color::from_rgb(0.40, 0.70, 1.0),
                Color::from_rgb(0.85, 0.87, 0.90),
                Color::from_rgb(0.10, 0.12, 0.16),
            ),
            "assistant" => (
                "AI",
                Color::from_rgb(0.40, 0.85, 0.55),
                Color::from_rgb(0.82, 0.84, 0.88),
                Color::from_rgb(0.08, 0.10, 0.12),
            ),
            "error" => (
                "Error",
                Color::from_rgb(0.95, 0.40, 0.35),
                Color::from_rgb(0.90, 0.55, 0.50),
                Color::from_rgb(0.15, 0.08, 0.08),
            ),
            _ => (
                "System",
                Color::from_rgb(0.60, 0.60, 0.65),
                Color::from_rgb(0.70, 0.70, 0.75),
                Color::from_rgb(0.10, 0.10, 0.12),
            ),
        };

        let role_label = text(format!("{label}:"))
            .size(12)
            .color(label_color);
        let content = text(&msg.content)
            .size(13)
            .color(content_color);

        let msg_widget = column![role_label, content].spacing(2);

        let msg_container: Element<'a, Message> = container(msg_widget)
            .width(Fill)
            .padding(Padding::from([8, 12]))
            .style(move |_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color::from_rgb(0.12, 0.12, 0.15),
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into();

        message_elements.push(msg_container);
    }

    // Show in-progress streaming response.
    if state.streaming && !state.current_response.is_empty() {
        let role_label = text("AI:")
            .size(12)
            .color(Color::from_rgb(0.40, 0.85, 0.55));
        let content = text(format!("{}\u{2588}", state.current_response))
            .size(13)
            .color(Color::from_rgb(0.82, 0.84, 0.88));

        let msg_widget = column![role_label, content].spacing(2);

        let msg_container: Element<'a, Message> = container(msg_widget)
            .width(Fill)
            .padding(Padding::from([8, 12]))
            .style(|_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgb(0.08, 0.10, 0.12))),
                ..Default::default()
            })
            .into();

        message_elements.push(msg_container);
    } else if state.streaming {
        // Streaming but no tokens yet — show a waiting indicator.
        let waiting = text("AI is thinking...")
            .size(12)
            .color(Color::from_rgb(0.50, 0.50, 0.55));
        message_elements.push(
            container(waiting)
                .width(Fill)
                .padding(Padding::from([8, 12]))
                .into(),
        );
    }

    let messages_column = Column::from_vec(message_elements).spacing(2);

    let chat_scroll = scrollable(messages_column)
        .width(Fill)
        .height(Fill);

    // Input area
    let input_field = text_input("Type a message...", &state.input)
        .on_input(move |val| Message::AIInputChanged(pane, val))
        .on_submit(Message::AISendMessage(pane))
        .size(13)
        .padding(Padding::from([8, 10]));

    let send_enabled = !state.input.trim().is_empty() && !state.streaming;

    let mut send_btn = button(text("Send").size(12).center())
        .padding(Padding::from([8, 14]))
        .style(|_theme: &Theme, status: button::Status| {
            let bg = match status {
                button::Status::Hovered => Color::from_rgb(0.25, 0.55, 0.85),
                button::Status::Pressed => Color::from_rgb(0.20, 0.45, 0.75),
                _ => Color::from_rgb(0.22, 0.50, 0.80),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::WHITE,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        });

    if send_enabled {
        send_btn = send_btn.on_press(Message::AISendMessage(pane));
    }

    let input_row = row![input_field, send_btn]
        .spacing(4)
        .align_y(iced::Alignment::Center);

    let input_container = container(input_row)
        .width(Fill)
        .padding(Padding::from([6, 8]))
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgb(0.07, 0.07, 0.09))),
            border: Border {
                color: Color::from_rgb(0.15, 0.15, 0.20),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

    // Assemble: header + scrollable messages + input
    let layout = column![header, chat_scroll, input_container];

    container(layout)
        .width(Fill)
        .height(Fill)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgb(0.06, 0.06, 0.08))),
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// AI streaming helper
// ---------------------------------------------------------------------------

/// Create a stream of Messages from an AI provider streaming response.
fn async_stream(
    pane: pane_grid::Pane,
    provider_name: String,
    config: ProviderConfig,
    messages: Vec<ai::ChatMessage>,
) -> impl futures_util::Stream<Item = Message> {
    iced::stream::channel(64, move |mut sender: futures::channel::mpsc::Sender<Message>| async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

        // Spawn the provider call in a background task.
        let cfg = config;
        let msgs = messages;
        tokio::spawn(async move {
            match provider_name.as_str() {
                "anthropic" => {
                    let p = AnthropicProvider::new();
                    p.stream_chat(&cfg, &msgs, tx).await;
                }
                "gemini" => {
                    let p = GeminiProvider::new();
                    p.stream_chat(&cfg, &msgs, tx).await;
                }
                _ => {
                    // OpenAI-compatible: openai, grok, lmstudio, ollama
                    let p = OpenAIProvider::new();
                    p.stream_chat(&cfg, &msgs, tx).await;
                }
            }
        });

        // Forward events from the mpsc channel to the iced stream.
        while let Some(event) = rx.recv().await {
            let msg = match event {
                StreamEvent::Token(t) => Message::AIStreamToken(pane, t),
                StreamEvent::Done => Message::AIStreamDone(pane),
                StreamEvent::Error(e) => Message::AIStreamError(pane, e),
            };
            if sender.try_send(msg).is_err() {
                break;
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Title bar button helper
// ---------------------------------------------------------------------------

/// Build a small, styled button for the pane title bar.
fn title_bar_button(label: &str, on_press: Message) -> Element<'_, Message> {
    button(text(label).size(12).center())
        .on_press(on_press)
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(20.0))
        .padding(0)
        .style(|_theme: &Theme, status: button::Status| {
            let bg = match status {
                button::Status::Hovered => Color::from_rgb(0.25, 0.25, 0.35),
                button::Status::Pressed => Color::from_rgb(0.30, 0.30, 0.40),
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::from_rgb(0.70, 0.70, 0.75),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn title_bar_style(
    _theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    let bg = if is_focused {
        Color::from_rgb(0.14, 0.16, 0.24)
    } else {
        Color::from_rgb(0.08, 0.08, 0.10)
    };

    let text_color = if is_focused {
        Color::from_rgb(0.90, 0.92, 0.96)
    } else {
        Color::from_rgb(0.50, 0.50, 0.52)
    };

    iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(text_color),
        border: Border {
            color: if is_focused {
                Color::from_rgb(0.35, 0.55, 0.90)
            } else {
                Color::from_rgb(0.12, 0.12, 0.14)
            },
            width: if is_focused { 1.0 } else { 0.0 },
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn pane_content_style(
    _theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb(0.05, 0.05, 0.05))),
        border: Border {
            color: if is_focused {
                Color::from_rgb(0.35, 0.55, 0.90)
            } else {
                Color::from_rgb(0.12, 0.12, 0.14)
            },
            width: if is_focused { 2.0 } else { 1.0 },
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Key mapping
// ---------------------------------------------------------------------------

/// Convert an iced keyboard key press into the bytes that should be sent to the PTY.
fn key_to_bytes(key: &Key, modifiers: &Modifiers) -> Option<Vec<u8>> {
    match key {
        Key::Character(c) => {
            let s = c.as_str();
            // Handle Ctrl+<letter> sequences.
            if modifiers.control() {
                if let Some(ch) = s.chars().next() {
                    let lower = ch.to_ascii_lowercase();
                    if lower >= 'a' && lower <= 'z' {
                        // Ctrl+A = 0x01, ..., Ctrl+Z = 0x1A
                        let ctrl_byte = (lower as u8) - b'a' + 1;
                        return Some(vec![ctrl_byte]);
                    }
                }
            }
            Some(s.as_bytes().to_vec())
        }
        Key::Named(named) => named_key_to_bytes(named, modifiers),
        Key::Unidentified => None,
    }
}

/// Convert a named key to the corresponding byte sequence for the PTY.
fn named_key_to_bytes(named: &Named, _modifiers: &Modifiers) -> Option<Vec<u8>> {
    match named {
        Named::Enter => Some(b"\r".to_vec()),
        Named::Backspace => Some(vec![0x7f]),
        Named::Tab => Some(b"\t".to_vec()),
        Named::Escape => Some(vec![0x1b]),
        Named::Space => Some(b" ".to_vec()),

        // Arrow keys -- standard ANSI escape sequences.
        Named::ArrowUp => Some(b"\x1b[A".to_vec()),
        Named::ArrowDown => Some(b"\x1b[B".to_vec()),
        Named::ArrowRight => Some(b"\x1b[C".to_vec()),
        Named::ArrowLeft => Some(b"\x1b[D".to_vec()),

        // Navigation keys.
        Named::Home => Some(b"\x1b[H".to_vec()),
        Named::End => Some(b"\x1b[F".to_vec()),
        Named::PageUp => Some(b"\x1b[5~".to_vec()),
        Named::PageDown => Some(b"\x1b[6~".to_vec()),
        Named::Delete => Some(b"\x1b[3~".to_vec()),
        Named::Insert => Some(b"\x1b[2~".to_vec()),

        // Modifier keys themselves should not produce output.
        Named::Shift | Named::Control | Named::Alt | Named::Super | Named::Meta => None,

        _ => None,
    }
}
