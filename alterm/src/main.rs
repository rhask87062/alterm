use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Max delay between two clicks on the same tab to count as a double-click.
const TAB_DOUBLE_CLICK: Duration = Duration::from_millis(400);

use iced::event::Status;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{
    button, column, container, mouse_area, opaque, pane_grid, pick_list, responsive, rich_text,
    row, scrollable, slider, span, stack, text, text_input, toggler, Column, Id as WidgetId,
};
use iced::widget::operation::focus as widget_focus;
use iced::window;
use iced::{Background, Border, Color, Element, Event, Fill, Length, Padding, Point, Subscription, Task, Theme};

use gpu_renderer::widget::TerminalView;
use workspace::{
    all_palette_actions, match_shortcut, sidebar_view, tab_bar_view, Action, Block, BrowserState,
    CommandPalette, PreviewState, SettingsField, SettingsSection, SidebarAction, Tab, TabBarAction,
    CELL_HEIGHT,
};
use workspace::chrome;
use workspace::grid;
use workspace::session;

// ---------------------------------------------------------------------------
// Theme helpers
// ---------------------------------------------------------------------------

/// Build an opaque `Color` from a `0xRRGGBB` literal.
const fn hex(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
    )
}

/// Theme names for the app's own brand themes (the "Living Terminal" palette
/// shared with the marketing site, `website/src/styles/global.css`).
const ALTERM_DARK_NAME: &str = "Alterm Dark";
const ALTERM_LIGHT_NAME: &str = "Alterm Light";

/// The default dark chrome theme — deep violet-black canvas with neon-magenta
/// accents, matching the terminal palette in `crates/config/src/theme.rs`.
static ALTERM_DARK: LazyLock<Theme> = LazyLock::new(|| {
    Theme::custom(
        ALTERM_DARK_NAME.to_string(),
        iced::theme::Palette {
            background: hex(0x0d0814), // --bg
            text: hex(0xece6f5),       // --text
            primary: hex(0xd450fc),    // --orchid
            success: hex(0x5ef2b0),    // --term-green
            warning: hex(0xffd56b),    // --term-yellow
            danger: hex(0xff6b9d),     // --term-red
        },
    )
});

/// The light chrome theme — same family favoring white and lighter tints.
static ALTERM_LIGHT: LazyLock<Theme> = LazyLock::new(|| {
    Theme::custom(
        ALTERM_LIGHT_NAME.to_string(),
        iced::theme::Palette {
            background: hex(0xfaf3ff), // lavender-white
            text: hex(0x1d1430),       // deep violet text
            primary: hex(0xa021d6),    // --purple-mid
            success: hex(0x1f9e6e),
            warning: hex(0xa9750a),
            danger: hex(0xd8336e),
        },
    )
});

/// Returns `true` when the current iced theme is a light variant.
fn is_light_theme(theme: &Theme) -> bool {
    // Derive from the theme's own palette so every theme works — built-in
    // light variants and custom themes (e.g. "Alterm Light") alike.
    !theme.extended_palette().is_dark
}

/// Returns `true` when the config theme string is a light variant.
fn is_config_light_theme(s: &str) -> bool {
    matches!(
        s,
        "light" | "Solarized Light" | "Gruvbox Light" | "Catppuccin Latte" | ALTERM_LIGHT_NAME
    )
}

/// Map a config theme string to an iced `Theme`.
fn theme_from_config(s: &str) -> Theme {
    match s {
        // The app's own brand themes (the website "Living Terminal" palette).
        // "dark"/"light" are aliases so the default config maps to the brand.
        ALTERM_DARK_NAME | "dark" => ALTERM_DARK.clone(),
        ALTERM_LIGHT_NAME | "light" => ALTERM_LIGHT.clone(),
        "Solarized Light" => Theme::SolarizedLight,
        "Solarized Dark" => Theme::SolarizedDark,
        "Gruvbox Light" => Theme::GruvboxLight,
        "Gruvbox Dark" => Theme::GruvboxDark,
        "Catppuccin Latte" => Theme::CatppuccinLatte,
        "Catppuccin Mocha" => Theme::CatppuccinMocha,
        _ => ALTERM_DARK.clone(), // any unrecognised value → brand dark
    }
}

/// Return the light↔dark partner for a theme string.
fn theme_partner(s: &str) -> &'static str {
    match s {
        ALTERM_DARK_NAME => ALTERM_LIGHT_NAME,
        ALTERM_LIGHT_NAME => ALTERM_DARK_NAME,
        "light" => "dark",
        "Solarized Light" => "Solarized Dark",
        "Gruvbox Light" => "Gruvbox Dark",
        "Catppuccin Latte" => "Catppuccin Mocha",
        "Solarized Dark" => "Solarized Light",
        "Gruvbox Dark" => "Gruvbox Light",
        "Catppuccin Mocha" => "Catppuccin Latte",
        _ => "light", // "dark" → "light", unknown → "light"
    }
}

use ai::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAIProvider, Provider,
    ProviderConfig, StreamEvent,
};
use alterm_config::{hooks::LuaHooks, AppConfig};
use browser::webview_manager;

fn main() -> iced::Result {
    // Set webkit2gtk env vars before any library initialization so they take
    // effect before the webkit subprocess is spawned.
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("GTK_THEME", "Adwaita:dark");

    env_logger::init();

    iced::application(Alterm::new, Alterm::update, Alterm::view)
        .title("Alterm")
        .theme(|app: &Alterm| theme_from_config(&app.config.appearance.theme))
        .window(window::Settings {
            size: iced::Size::new(900.0, 600.0),
            icon: load_window_icon(),
            ..window::Settings::default()
        })
        .subscription(Alterm::subscription)
        .exit_on_close_request(false)
        .run()
}

fn load_window_icon() -> Option<window::Icon> {
    window::icon::from_file_data(
        include_bytes!("../../assets/icons/alterm-logo.png"),
        None,
    )
    .map_err(|error| {
        log::warn!("Failed to load window icon from assets/icons/alterm-logo.png: {error}");
        error
    })
    .ok()
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
/// Padding around the whole pane grid (between the panes and the surrounding
/// chrome). Must match the container padding wrapping the PaneGrid in `view`;
/// the browser-webview positioning math offsets by it to stay aligned.
const GRID_PADDING: f32 = 8.0;
/// Height of the browser nav bar (URL input + padding) in logical pixels.
const BROWSER_NAV_BAR_HEIGHT: f32 = 40.0;

/// Extract a numeric ID from a `pane_grid::Pane` for use as a webview key.
///
/// `Pane` wraps a `usize` but the field is `pub(super)`. We parse it
/// from the Debug output (`Pane(N)`).
fn pane_to_id(pane: pane_grid::Pane) -> u64 {
    let dbg = format!("{pane:?}");
    dbg.trim_start_matches("Pane(")
        .trim_end_matches(')')
        .parse::<u64>()
        .unwrap_or(0)
}

/// Compose a tab-unique webview map key from a tab id and a pane index.
///
/// Pane ids restart at 0 in every tab, so the bare pane id collides across
/// tabs; namespacing with the (stable) tab id keeps webview keys unique.
fn compose_key(tab_id: u64, pane_index: u64) -> u64 {
    (tab_id << 32) | (pane_index & 0xFFFF_FFFF)
}

/// Compose a tab-unique webview map key from a tab id and a pane.
///
/// Pane ids restart at 0 in every tab, so the bare pane id collides across
/// tabs; namespacing with the (stable) tab id keeps webview keys unique.
fn webview_key(tab_id: u64, pane: pane_grid::Pane) -> u64 {
    compose_key(tab_id, pane_to_id(pane))
}

struct Alterm {
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
    /// Optional Lua hooks (loaded from hooks.lua if present).
    hooks: LuaHooks,
    /// Native window handle (NSView on macOS, XID on X11, HWND on Windows).
    parent_xid: Option<u64>,
    /// Available monospace font families for the settings dropdown.
    available_fonts: Vec<String>,
    /// Leaked font family name for use with iced's `Font::Family::Name(&'static str)`.
    /// Updated when settings are saved.
    terminal_font_family: &'static str,
    /// State of the right-click context menu, if any.
    context_menu: Option<ContextMenuState>,
    /// Most recent text selection finalized in a terminal pane. Empty if none.
    last_selection: String,
    /// In-progress inline rename of a tab or pane title, if any.
    rename: Option<RenameTarget>,
    /// Live text of the in-progress rename field.
    rename_buffer: String,
    /// Last (tab index, time) a tab was clicked, for double-click rename detection.
    last_tab_click: Option<(usize, Instant)>,
    /// Last (pane, time) a pane title was clicked, for double-click rename detection.
    last_pane_click: Option<(pane_grid::Pane, Instant)>,
}

/// What an in-progress inline rename is targeting.
#[derive(Clone, Copy)]
enum RenameTarget {
    Tab(usize),
    Pane(pane_grid::Pane),
}

/// Widget id of the inline rename text field, so it can be focused on open.
fn rename_input_id() -> WidgetId {
    WidgetId::from("tab-pane-rename-input".to_string())
}

#[derive(Clone, Copy)]
struct ContextMenuState {
    /// Absolute window-relative position where the menu should anchor its
    /// top-left corner. The right-clicked pane is identified separately via
    /// `Alterm::active_tab().focus`, which `ContextMenuOpen` sets.
    position: Point,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    KeyboardInput(Key, Key, Modifiers),
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
    // Inline rename of tab / pane titles
    PaneTitleClicked(pane_grid::Pane),
    BeginPaneRename(pane_grid::Pane),
    RenameInputChanged(String),
    RenameCommit,
    RenameCancel,
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
    AIProviderChanged(pane_grid::Pane, String),
    AIModelChanged(pane_grid::Pane, String),
    AIFetchModels(pane_grid::Pane),
    AIModelsFetched(pane_grid::Pane, Vec<String>),
    AICopyMessage(String),
    TerminalSelected(String),
    // Right-click context menu on a terminal pane.
    ContextMenuOpen(pane_grid::Pane, Point),
    ContextMenuClose,
    ContextMenuCopy,
    ContextMenuPaste,
    ContextMenuSelectAll,
    ContextMenuClear,
    ToggleAIChat,
    // Settings panel
    OpenSettings,
    SettingsChanged(pane_grid::Pane, SettingsField),
    SettingsSave(pane_grid::Pane),
    SettingsSectionChanged(pane_grid::Pane, SettingsSection),
    // Browser
    OpenBrowser,
    BrowserNavigate(pane_grid::Pane, String),
    BrowserBack(pane_grid::Pane),
    BrowserForward(pane_grid::Pane),
    BrowserReload(pane_grid::Pane),
    BrowserUrlChanged(pane_grid::Pane, String),
    // Preview
    OpenPreview,
    PreviewNavigate(pane_grid::Pane, String),
    PreviewParent(pane_grid::Pane),
    PptxSlidesReady(pane_grid::Pane, Vec<std::path::PathBuf>, std::path::PathBuf),
    PptxConversionFailed(pane_grid::Pane, String),
    PreviewSlidePrev(pane_grid::Pane),
    PreviewSlideNext(pane_grid::Pane),
    // Hotkey info
    ShowHotkeyInfo,
    // Theme toggle
    ToggleTheme,
    // Window handle (X11 XID) ready
    WindowHandleReady(u64),
    // Session persistence
    SaveSession,
    WindowCloseRequested,
}

impl Alterm {
    fn new() -> (Self, Task<Message>) {
        let window_width = 900.0_f32;
        let window_height = 600.0_f32;

        // Initialize GTK early (required by webkit2gtk before any webview creation).
        webview_manager::init_gtk();

        // Load config from default path.
        let config = AppConfig::load(&AppConfig::config_path()).unwrap_or_else(|e| {
            log::warn!("Failed to load config: {e}, using defaults");
            AppConfig::default()
        });

        // Load optional Lua hooks from ~/.config/alterm/hooks.lua.
        let mut hooks = LuaHooks::new();
        match hooks.load_file(&AppConfig::hooks_path()) {
            Ok(true) => log::info!("Lua hooks loaded from {:?}", AppConfig::hooks_path()),
            Ok(false) => log::debug!("No hooks.lua found; Lua hooks disabled"),
            Err(e) => log::warn!("Failed to load hooks.lua: {e}"),
        }

        // Restore session or build a default tab.
        let (tabs, active_tab, window_width, window_height) = {
            let default_session = || {
                // Initial size estimate for a single-pane tab at launch.
                // resize_all_panes() will correct this once the window opens.
                let grid_width = (window_width - SIDEBAR_WIDTH).max(80.0);
                let grid_height = (window_height - TAB_BAR_HEIGHT).max(40.0);
                let content_height = (grid_height - PANE_TITLE_BAR_HEIGHT).max(CELL_HEIGHT * 2.0);
                let first_tab = Tab::new_with_size(grid_width, content_height)
                    .expect("Failed to create initial tab");
                (vec![first_tab], 0usize, window_width, window_height)
            };
            let restored = if config.session.restore {
                session::load_from_path(&session::session_path())
                    .map(|state| session::restore(state, &config))
                    .filter(|r| !r.tabs.is_empty())
            } else {
                None
            };
            match restored {
                Some(r) => {
                    let active = r.active_tab.min(r.tabs.len() - 1); // clamp to valid range
                    (r.tabs, active, r.window.width, r.window.height)
                }
                None => default_session(),
            }
        };

        // Enumerate available monospace fonts for the settings dropdown.
        let available_fonts = enumerate_monospace_fonts();

        // Leak the font family name so it can be used as &'static str with iced's Font API.
        let terminal_font_family: &'static str =
            Box::leak(config.appearance.font_family.clone().into_boxed_str());

        let app = Alterm {
            tabs,
            active_tab,
            palette: CommandPalette::new(),
            scroll_accumulator: 0.0,
            window_width,
            window_height,
            config,
            hooks,
            parent_xid: None,
            available_fonts,
            terminal_font_family,
            context_menu: None,
            last_selection: String::new(),
            rename: None,
            rename_buffer: String::new(),
            last_tab_click: None,
            last_pane_click: None,
        };

        // Request the native window handle from iced — fires WindowHandleReady.
        // window::run() gives us &dyn HasWindowHandle so we can extract the
        // correct platform handle: NSView on macOS, XID on X11, HWND on Windows.
        let fetch_handle = window::oldest().then(|opt_id| {
            if let Some(id) = opt_id {
                window::run(id, extract_native_window_handle).map(Message::WindowHandleReady)
            } else {
                Task::none()
            }
        });

        (app, fetch_handle)
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
    /// Also repositions any browser webviews to match their pane regions.
    fn resize_all_panes(&mut self) {
        use iced::{Rectangle, Size};

        let grid_width = (self.window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (self.window_height - TAB_BAR_HEIGHT).max(40.0);

        // The grid is inset by GRID_PADDING on all sides, so pane regions are
        // computed against the smaller padded area.
        let bounds = Size::new(
            (grid_width - GRID_PADDING * 2.0).max(40.0),
            (grid_height - GRID_PADDING * 2.0).max(40.0),
        );

        let tab = self.active_tab_mut();
        let tab_id = tab.id;
        let maximized_pane = tab.panes.maximized();

        // When maximized, the focused pane gets the full grid area.
        // pane_regions() returns the original split tree, not the maximized view.
        let regions = tab.panes.layout().pane_regions(
            PANE_GRID_SPACING,
            PANE_GRID_MIN_SIZE,
            bounds,
        );

        // Full-grid rectangle for maximized panes (within the padded area).
        let full_rect = Rectangle {
            x: 0.0, y: 0.0,
            width: bounds.width, height: bounds.height,
        };

        for (pane, default_rect) in &regions {
            // Use full rect if this pane is maximized, otherwise use the layout region
            let rect = if maximized_pane == Some(*pane) {
                &full_rect
            } else if maximized_pane.is_some() {
                // Another pane is maximized — this one is hidden.
                // Hide its webview if it's a browser.
                if let Some(block) = tab.panes.get(*pane) {
                    if block.is_browser() {
                        webview_manager::set_visible(webview_key(tab_id, *pane), false);
                    }
                }
                continue;
            } else {
                default_rect
            };

            let content_width = rect.width;
            let content_height = (rect.height - PANE_TITLE_BAR_HEIGHT).max(CELL_HEIGHT * 2.0);

            if let Some(block) = tab.panes.get_mut(*pane) {
                if block.is_terminal() {
                    let (rows, cols) = Block::size_from_pixels(content_width, content_height);
                    let (cur_rows, cur_cols) = block.dimensions();
                    if cur_rows != rows || cur_cols != cols {
                        block.resize(rows, cols);
                    }
                }

                if block.is_browser() {
                    let pane_id = webview_key(tab_id, *pane);
                    if webview_manager::exists(pane_id) {
                        let wv_x = (GRID_PADDING + rect.x) as f64;
                        let wv_y = (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64;
                        let wv_w = rect.width as f64;
                        // The native webview is a plain rectangle and fills to
                        // the pane bottom, so browser panes have square bottom
                        // corners (no rounded clipping for native windows).
                        let wv_h = (rect.height - PANE_TITLE_BAR_HEIGHT - BROWSER_NAV_BAR_HEIGHT).max(10.0) as f64;
                        webview_manager::set_bounds(pane_id, wv_x, wv_y, wv_w, wv_h);
                        webview_manager::set_visible(pane_id, true);
                    }
                }
            }
        }
    }

    /// Add a new window (pane) to the active tab as a wide-first balanced grid.
    ///
    /// Rebuilds the active tab's layout from its existing windows plus `block`,
    /// re-keys any browser webviews to their new pane ids, focuses the new
    /// window, and returns its pane. All "new window" actions funnel through here.
    fn add_window(&mut self, block: Block) -> pane_grid::Pane {
        let tab = self.active_tab_mut();
        let tab_id = tab.id;
        // Compute the grid against the full layout, not a maximized view.
        if tab.panes.maximized().is_some() {
            tab.panes.restore();
        }

        let info = grid::rebuild_with_new(&mut tab.panes, block, || Block::HotkeyInfo);
        tab.focus = Some(info.new_pane);

        // Carry existing webviews across to their new pane ids.
        let remap_ids: Vec<(u64, u64)> = info
            .remap
            .iter()
            .map(|(old, new)| (webview_key(tab_id, *old), webview_key(tab_id, *new)))
            .collect();
        webview_manager::remap(&remap_ids);

        self.resize_all_panes();
        info.new_pane
    }

    /// Create a real wry webview for a browser pane.
    fn create_browser_webview(&self, pane: pane_grid::Pane, url: &str) {
        let Some(xid) = self.parent_xid else {
            log::warn!("Cannot create webview: parent XID not yet available");
            return;
        };

        let tab_id = self.active_tab().id;
        let pane_id = webview_key(tab_id, pane);

        // Calculate initial bounds for this pane.
        use iced::Size;
        let grid_width = (self.window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (self.window_height - TAB_BAR_HEIGHT).max(40.0);
        let bounds = Size::new(
            (grid_width - GRID_PADDING * 2.0).max(40.0),
            (grid_height - GRID_PADDING * 2.0).max(40.0),
        );

        let tab = self.active_tab();
        let regions = tab.panes.layout().pane_regions(
            PANE_GRID_SPACING,
            PANE_GRID_MIN_SIZE,
            bounds,
        );

        let (x, y, w, h) = if let Some(rect) = regions.get(&pane) {
            let wv_x = (GRID_PADDING + rect.x) as f64;
            let wv_y = (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64;
            let wv_w = rect.width as f64;
            let wv_h = (rect.height - PANE_TITLE_BAR_HEIGHT - BROWSER_NAV_BAR_HEIGHT).max(10.0) as f64;
            (wv_x, wv_y, wv_w, wv_h)
        } else {
            // Fallback: reasonable defaults.
            (0.0, (TAB_BAR_HEIGHT + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64, 600.0, 400.0)
        };

        if let Err(e) = webview_manager::create_webview(pane_id, xid, url, (x, y, w, h)) {
            log::error!("Failed to create webview for pane {pane_id}: {e}");
        }
    }

    /// Show webviews in the active tab, hide webviews in all other tabs.
    fn update_webview_visibility(&self) {
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            let is_active = tab_idx == self.active_tab;
            for (pane, block) in tab.panes.iter() {
                if block.is_browser() {
                    webview_manager::set_visible(webview_key(tab.id, *pane), is_active);
                }
            }
        }
    }

    /// Abandon any in-progress inline rename without applying it.
    fn cancel_rename(&mut self) {
        self.rename = None;
        self.rename_buffer.clear();
    }

    /// Save the current session to disk.
    fn save_session(&self) {
        let window = session::WindowState { width: self.window_width, height: self.window_height };
        let state = session::capture(&self.tabs, self.active_tab, window);
        if let Err(e) = session::save_to_path(&state, &session::session_path()) {
            log::warn!("Failed to save session: {e}");
        }
    }

    /// Create a webview for a browser pane in any tab, keyed by the explicit tab id.
    /// Non-active tabs' panes won't appear in the active layout so fall back to default bounds.
    fn create_browser_webview_for(&self, tab_id: u64, pane: pane_grid::Pane, url: &str) {
        let Some(xid) = self.parent_xid else {
            log::warn!("Cannot create webview: parent XID not yet available");
            return;
        };

        let pane_id = webview_key(tab_id, pane);

        // Try to get real bounds from the pane layout (works for active tab panes).
        use iced::Size;
        let grid_width = (self.window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (self.window_height - TAB_BAR_HEIGHT).max(40.0);
        let bounds = Size::new(
            (grid_width - GRID_PADDING * 2.0).max(40.0),
            (grid_height - GRID_PADDING * 2.0).max(40.0),
        );

        // Find the tab by id to look up its pane layout.
        let regions = self.tabs.iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.panes.layout().pane_regions(PANE_GRID_SPACING, PANE_GRID_MIN_SIZE, bounds));

        let (x, y, w, h) = if let Some(regions) = regions {
            if let Some(rect) = regions.get(&pane) {
                let wv_x = (GRID_PADDING + rect.x) as f64;
                let wv_y = (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64;
                let wv_w = rect.width as f64;
                let wv_h = (rect.height - PANE_TITLE_BAR_HEIGHT - BROWSER_NAV_BAR_HEIGHT).max(10.0) as f64;
                (wv_x, wv_y, wv_w, wv_h)
            } else {
                // Non-active tab pane — use default bounds; resize_all_panes() corrects when tab is selected.
                (0.0, (TAB_BAR_HEIGHT + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64, 600.0, 400.0)
            }
        } else {
            (0.0, (TAB_BAR_HEIGHT + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64, 600.0, 400.0)
        };

        if let Err(e) = webview_manager::create_webview(pane_id, xid, url, (x, y, w, h)) {
            log::error!("Failed to create webview for pane {pane_id}: {e}");
        }
    }

    /// Create webviews for all browser panes across all tabs that don't yet have one.
    /// Called once parent_xid becomes available (WindowHandleReady). Idempotent.
    fn ensure_browser_webviews(&mut self) {
        // Collect tab/pane/url tuples to avoid borrow conflicts.
        let browser_panes: Vec<(u64, pane_grid::Pane, String)> = self.tabs.iter()
            .flat_map(|tab| {
                let tab_id = tab.id;
                tab.panes.iter().filter_map(move |(pane, block)| {
                    if let Block::Browser { state } = block {
                        if !webview_manager::exists(webview_key(tab_id, *pane)) {
                            return Some((tab_id, *pane, state.url.clone()));
                        }
                    }
                    None
                })
            })
            .collect();

        for (tab_id, pane, url) in browser_panes {
            self.create_browser_webview_for(tab_id, pane, &url);
        }
        self.update_webview_visibility();
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
            "google" => ai_cfg.providers.google.as_ref(),
            "xai" => ai_cfg.providers.xai.as_ref(),
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
                // F2 renames the active tab using the same inline editor as a
                // double-click on the tab.
                let i = self.active_tab;
                if let Some(tab) = self.tabs.get(i) {
                    self.rename = Some(RenameTarget::Tab(i));
                    self.rename_buffer = tab.title.clone();
                    return widget_focus(rename_input_id());
                }
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
            Action::OpenSettings => self.update(Message::OpenSettings),
            Action::ToggleAIChat => self.update(Message::ToggleAIChat),
            Action::Copy => {
                if self.last_selection.is_empty() {
                    Task::none()
                } else {
                    iced::clipboard::write(self.last_selection.clone())
                }
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
            Action::NewTerminal => self.update(Message::SidebarNewTerminal),
            Action::NewBrowser => self.update(Message::OpenBrowser),
            Action::NewPreview => self.update(Message::OpenPreview),
            Action::ShowHotkeyInfo => self.update(Message::ShowHotkeyInfo),
            Action::ToggleTheme => self.update(Message::ToggleTheme),
            Action::Search => {
                log::debug!("Search — not yet implemented");
                Task::none()
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Pump GTK events so webkit2gtk can process network/rendering.
                webview_manager::pump_gtk_events();

                // Tick all panes in all tabs.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        block.tick();
                    }
                }
            }
            Message::PaneClicked(pane) => {
                self.active_tab_mut().focus = Some(pane);
                // If the clicked pane is an AI chat or browser, focus its text_input
                // so keyboard events are captured by it (not routed to the PTY).
                if let Some(block) = self.active_tab().panes.get(pane) {
                    if block.is_ai_chat() {
                        return widget_focus(WidgetId::from(
                            format!("ai-chat-input-{:?}", pane),
                        ));
                    }
                    if block.is_browser() {
                        return widget_focus(WidgetId::from(
                            format!("browser-url-input-{:?}", pane),
                        ));
                    }
                }
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
            // Split Right / Split Down now both add a window to the balanced grid.
            Message::SplitHorizontal | Message::SplitVertical => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
            Message::ClosePane => {
                let tab = self.active_tab_mut();
                let tab_id = tab.id;
                if let Some(focused) = tab.focus {
                    // Destroy any webview associated with this pane.
                    webview_manager::destroy(webview_key(tab_id, focused));

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
                let tab_id = tab.id;
                if let Some(focused) = tab.focus {
                    if tab.panes.maximized().is_some() {
                        tab.panes.restore();
                        // Show all browser webviews in this tab.
                        for (pane, block) in tab.panes.iter() {
                            if block.is_browser() {
                                webview_manager::set_visible(webview_key(tab_id, *pane), true);
                            }
                        }
                    } else {
                        // Hide all non-focused browser webviews before maximizing.
                        for (pane, block) in tab.panes.iter() {
                            if block.is_browser() && *pane != focused {
                                webview_manager::set_visible(webview_key(tab_id, *pane), false);
                            }
                        }
                        tab.panes.maximize(focused);
                    }
                }
                self.resize_all_panes();
            }

            // Per-pane title-bar split buttons also add a window to the grid.
            Message::SplitPaneRight(_) | Message::SplitPaneDown(_) => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
            Message::ClosePaneId(pane) => {
                // Destroy any webview associated with this pane before removing it.
                let tab_id = self.active_tab().id;
                webview_manager::destroy(webview_key(tab_id, pane));

                let tab = self.active_tab_mut();
                tab.pane_labels.remove(&pane);
                if tab.panes.len() > 1 {
                    if let Some((_closed_block, sibling)) = tab.panes.close(pane) {
                        tab.focus = Some(sibling);
                    }
                }
                if matches!(self.rename, Some(RenameTarget::Pane(p)) if p == pane) {
                    self.cancel_rename();
                }
                self.resize_all_panes();
            }
            Message::MaximizeTogglePane(pane) => {
                let tab = self.active_tab_mut();
                let tab_id = tab.id;
                if tab.panes.maximized().is_some() {
                    tab.panes.restore();
                    // Show all browser webviews in this tab.
                    for (p, block) in tab.panes.iter() {
                        if block.is_browser() {
                            webview_manager::set_visible(webview_key(tab_id, *p), true);
                        }
                    }
                } else {
                    // Hide non-target browser webviews.
                    for (p, block) in tab.panes.iter() {
                        if block.is_browser() && *p != pane {
                            webview_manager::set_visible(webview_key(tab_id, *p), false);
                        }
                    }
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
                    // Destroy all webviews in the tab being closed.
                    let closing_tab_id = self.tabs[index].id;
                    for (pane, block) in self.tabs[index].panes.iter() {
                        if block.is_browser() {
                            webview_manager::destroy(webview_key(closing_tab_id, *pane));
                        }
                    }

                    self.tabs.remove(index);
                    // Adjust active_tab index after removal.
                    if self.active_tab >= self.tabs.len() {
                        self.active_tab = self.tabs.len() - 1;
                    } else if self.active_tab > index {
                        self.active_tab -= 1;
                    }

                    // Update webview visibility for the newly active tab.
                    self.update_webview_visibility();
                }
            }
            Message::SelectTab(index) => {
                if index < self.tabs.len() {
                    self.active_tab = index;
                    self.resize_all_panes();
                    self.update_webview_visibility();
                }
            }
            Message::TabBarAction(action) => match action {
                TabBarAction::Select(i) => {
                    // A quick second click on the same tab renames it.
                    let now = Instant::now();
                    let double = matches!(
                        self.last_tab_click,
                        Some((j, t)) if j == i && now.duration_since(t) < TAB_DOUBLE_CLICK
                    );
                    self.last_tab_click = Some((i, now));
                    if double {
                        if let Some(tab) = self.tabs.get(i) {
                            self.rename = Some(RenameTarget::Tab(i));
                            self.rename_buffer = tab.title.clone();
                            return widget_focus(rename_input_id());
                        }
                    }
                    self.cancel_rename();
                    return self.update(Message::SelectTab(i));
                }
                TabBarAction::Close(i) => {
                    self.cancel_rename();
                    return self.update(Message::CloseTab(i));
                }
                TabBarAction::New => {
                    self.cancel_rename();
                    return self.update(Message::NewTab);
                }
                TabBarAction::RenameInput(s) => self.rename_buffer = s,
                TabBarAction::RenameSubmit => return self.update(Message::RenameCommit),
            },
            Message::PaneTitleClicked(pane) => {
                // Single click focuses the pane; a quick second click renames it.
                let now = Instant::now();
                let double = matches!(
                    self.last_pane_click,
                    Some((p, t)) if p == pane && now.duration_since(t) < TAB_DOUBLE_CLICK
                );
                self.last_pane_click = Some((pane, now));
                self.active_tab_mut().focus = Some(pane);
                if double {
                    return self.update(Message::BeginPaneRename(pane));
                }
            }
            Message::BeginPaneRename(pane) => {
                let tab = self.active_tab();
                let current = tab
                    .pane_labels
                    .get(&pane)
                    .cloned()
                    .or_else(|| tab.panes.get(pane).map(|b| b.title()))
                    .unwrap_or_default();
                self.rename = Some(RenameTarget::Pane(pane));
                self.rename_buffer = current;
                return widget_focus(rename_input_id());
            }
            Message::RenameInputChanged(s) => self.rename_buffer = s,
            Message::RenameCommit => {
                if let Some(target) = self.rename.take() {
                    let value = self.rename_buffer.trim().to_string();
                    match target {
                        RenameTarget::Tab(i) => {
                            if let Some(tab) = self.tabs.get_mut(i) {
                                if !value.is_empty() {
                                    tab.title = value;
                                }
                            }
                        }
                        RenameTarget::Pane(pane) => {
                            let tab = self.active_tab_mut();
                            if value.is_empty() {
                                // Empty label reverts the pane to its default title.
                                tab.pane_labels.remove(&pane);
                            } else {
                                tab.pane_labels.insert(pane, value);
                            }
                        }
                    }
                    self.rename_buffer.clear();
                    self.save_session();
                }
            }
            Message::RenameCancel => self.cancel_rename(),
            Message::SidebarAction(action) => match action {
                SidebarAction::NewTerminal => {
                    return self.update(Message::SidebarNewTerminal);
                }
                SidebarAction::NewAiChat => {
                    return self.update(Message::ToggleAIChat);
                }
                SidebarAction::NewBrowser => {
                    return self.update(Message::OpenBrowser);
                }
                SidebarAction::NewPreview => {
                    return self.update(Message::OpenPreview);
                }
                SidebarAction::OpenSettings => {
                    return self.update(Message::OpenSettings);
                }
                SidebarAction::ShowHotkeyInfo => {
                    return self.update(Message::ShowHotkeyInfo);
                }
                SidebarAction::ToggleTheme => {
                    return self.update(Message::ToggleTheme);
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
                    "google" => self.config.ai.providers.google.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "gemini-2.0-flash".to_string()),
                    "xai" => self.config.ai.providers.xai.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "grok-2".to_string()),
                    "lmstudio" => self.config.ai.providers.lmstudio.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "local-model".to_string()),
                    "ollama" => self.config.ai.providers.ollama.as_ref()
                        .map(|e| e.model.clone()).unwrap_or_else(|| "llama3.2".to_string()),
                    _ => "unknown".to_string(),
                };

                let block = Block::new_ai_chat(provider_name, model_name);
                let new_pane = self.add_window(block);
                let focus_task = widget_focus(WidgetId::from(
                    format!("ai-chat-input-{:?}", new_pane),
                ));
                let fetch_task = self.update(Message::AIFetchModels(new_pane));
                return Task::batch([focus_task, fetch_task]);
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
                                 ~/.config/alterm/config.toml"
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
            Message::AIProviderChanged(pane, provider) => {
                let new_model = self.config.ai.provider_model(&provider);
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.provider_name = provider;
                    state.model_name = new_model;
                    state.available_models.clear();
                }
                // Trigger a model list fetch for the new provider.
                return self.update(Message::AIFetchModels(pane));
            }
            Message::AIModelChanged(pane, model) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.model_name = model;
                }
            }
            Message::AIFetchModels(pane) => {
                // Look up provider info for the AI chat pane.
                let (provider_name, base_url, api_key) = {
                    let tab = self.active_tab();
                    if let Some(Block::AIChat { state }) = tab.panes.get(pane) {
                        let pname = state.provider_name.clone();
                        let entry = self.config.ai.providers.get(&pname);
                        let base_url = entry
                            .map(|e| e.resolved_base_url(&pname))
                            .unwrap_or_else(|| {
                                alterm_config::default_base_url(&pname)
                                    .unwrap_or("")
                                    .to_string()
                            });
                        let api_key = entry.and_then(|e| e.api_key.clone());
                        (pname, base_url, api_key)
                    } else {
                        return Task::none();
                    }
                };

                // Mark as loading.
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.models_loading = true;
                }

                // Spawn async fetch.
                let ptype = provider_name.clone();
                return Task::perform(
                    async move {
                        ai::fetch_models(
                            &base_url,
                            api_key.as_deref(),
                            &ptype,
                        )
                        .await
                    },
                    move |models| Message::AIModelsFetched(pane, models),
                );
            }
            Message::AICopyMessage(content) => {
                return iced::clipboard::write(content);
            }
            Message::TerminalSelected(text) => {
                // Always remember the latest selection so Ctrl+Shift+C and the
                // right-click menu's Copy can act on it. Only auto-write to
                // the system clipboard when the user has opted in via the
                // `terminal.copy_on_select` config (xterm-style behavior).
                self.last_selection = text.clone();
                if self.config.terminal.copy_on_select {
                    return iced::clipboard::write(text);
                }
            }
            Message::ContextMenuOpen(pane, position) => {
                self.active_tab_mut().focus = Some(pane);
                self.context_menu = Some(ContextMenuState { position });
            }
            Message::ContextMenuClose => {
                self.context_menu = None;
            }
            Message::ContextMenuCopy => {
                self.context_menu = None;
                if !self.last_selection.is_empty() {
                    return iced::clipboard::write(self.last_selection.clone());
                }
            }
            Message::ContextMenuPaste => {
                self.context_menu = None;
                return iced::clipboard::read().map(Message::ClipboardContent);
            }
            Message::ContextMenuSelectAll => {
                self.context_menu = None;
                // Copy the entire visible terminal buffer to the clipboard.
                let light_mode = is_config_light_theme(&self.config.appearance.theme);
                let tab = self.active_tab();
                if let Some(focused) = tab.focus {
                    if let Some(block) = tab.panes.get(focused) {
                        let grid = block.render_grid(light_mode);
                        let mut buf = String::new();
                        for row in &grid.cells {
                            let line: String = row.iter().map(|c| c.c).collect();
                            buf.push_str(line.trim_end());
                            buf.push('\n');
                        }
                        let text = buf.trim_end_matches('\n').to_string();
                        if !text.is_empty() {
                            self.last_selection = text.clone();
                            return iced::clipboard::write(text);
                        }
                    }
                }
            }
            Message::ContextMenuClear => {
                self.context_menu = None;
                // Send Ctrl+L to the focused terminal — readline interprets this
                // as clear-screen, the standard shortcut every shell understands.
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if let Some(block) = tab.panes.get_mut(focused) {
                        block.write_input(&[0x0c]);
                    }
                }
            }
            Message::AIModelsFetched(pane, models) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.models_loading = false;
                    state.available_models = models;
                    // If the current model is not in the list, and we have models, keep it
                    // (the user may have typed a custom model).
                }
            }

            // -- Settings panel --
            Message::OpenSettings => {
                // Check if a settings pane already exists — focus it instead of creating a duplicate.
                let tab = self.active_tab_mut();
                let existing = tab.panes.iter()
                    .find(|(_p, b)| b.is_settings())
                    .map(|(p, _)| *p);
                if let Some(settings_pane) = existing {
                    tab.focus = Some(settings_pane);
                    return Task::none();
                }
                // Open a new settings pane.
                let block = Block::new_settings(self.config.clone());
                self.add_window(block);
            }
            Message::SettingsChanged(pane, field) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Settings { state }) = tab.panes.get_mut(pane) {
                    state.apply_field(field);
                }
            }
            Message::SettingsSectionChanged(pane, section) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Settings { state }) = tab.panes.get_mut(pane) {
                    state.active_section = section;
                }
            }
            Message::SettingsSave(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Settings { state }) = tab.panes.get_mut(pane) {
                    if let Err(e) = state.save() {
                        log::error!("Failed to save settings: {e}");
                    } else {
                        self.config = state.config.clone();
                        // Update the leaked font family string for the terminal renderer.
                        self.terminal_font_family =
                            Box::leak(self.config.appearance.font_family.clone().into_boxed_str());
                        log::info!("Settings saved and applied");
                    }
                }
                // Update all existing AI chat blocks with the new provider/model
                let new_provider = self.config.ai.default_provider.clone();
                let new_model = self.config.ai.provider_model(&new_provider);
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::AIChat { state } = block {
                            state.provider_name = new_provider.clone();
                            state.model_name = new_model.clone();
                        }
                    }
                }
                // Recalculate terminal dimensions in case font size changed.
                self.resize_all_panes();
            }

            // -- Browser --
            Message::OpenBrowser => {
                let url = "https://www.google.com";
                let block = Block::new_browser(url);
                let new_pane = self.add_window(block);
                // Create the webview against the final (post-rebuild) pane id.
                self.create_browser_webview(new_pane, url);
                webview_manager::pump_gtk_events();
                self.resize_all_panes();
                return widget_focus(WidgetId::from(
                    format!("browser-url-input-{:?}", new_pane),
                ));
            }
            Message::BrowserNavigate(pane, url) => {
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.navigate(&url);
                    // Also navigate the real webview.
                    webview_manager::navigate(pane_id, &state.url);
                }
            }
            Message::BrowserBack(pane) => {
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.go_back();
                    webview_manager::navigate(pane_id, &state.url);
                }
            }
            Message::BrowserForward(pane) => {
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.go_forward();
                    webview_manager::navigate(pane_id, &state.url);
                }
            }
            Message::BrowserReload(pane) => {
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.reload();
                    webview_manager::reload(pane_id);
                }
            }
            Message::BrowserUrlChanged(pane, url) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.input_url = url;
                }
            }

            // -- Preview --
            Message::OpenPreview => {
                let start_path = std::env::current_dir()
                    .ok()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("/"));
                let path_str = start_path.to_string_lossy().to_string();
                let block = Block::new_preview(&path_str);
                self.add_window(block);
            }
            Message::PreviewNavigate(pane, path) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                    state.navigate_to(&path);
                    if matches!(state.file_type, preview::FileType::Pptx) {
                        let pptx_path = state.path.clone();
                        return iced::Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    preview::pptx::render_slides(&pptx_path)
                                })
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                            },
                            move |result| match result {
                                Ok((images, temp_dir)) => {
                                    Message::PptxSlidesReady(pane, images, temp_dir)
                                }
                                Err(e) => Message::PptxConversionFailed(pane, e),
                            },
                        );
                    }
                }
            }
            Message::PreviewParent(pane) => {
                let parent = {
                    let tab = self.active_tab();
                    if let Some(Block::Preview { state }) = tab.panes.get(pane) {
                        state.parent_dir()
                    } else {
                        None
                    }
                };
                if let Some(parent_path) = parent {
                    let tab = self.active_tab_mut();
                    if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                        state.navigate_to(&parent_path);
                    }
                }
            }

            Message::PptxSlidesReady(pane, images, temp_dir) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                    if matches!(state.file_type, preview::FileType::Pptx) {
                        state.content = preview::PreviewContent::Slides { images, temp_dir };
                        state.scroll_offset = 0;
                    } else {
                        // Pane navigated away before conversion finished — drop temp dir.
                        let _ = std::fs::remove_dir_all(temp_dir);
                    }
                } else {
                    let _ = std::fs::remove_dir_all(temp_dir);
                }
            }
            Message::PptxConversionFailed(pane, err) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                    state.content = preview::PreviewContent::Unsupported(format!(
                        "Slide conversion failed:\n\n{err}"
                    ));
                }
            }
            Message::PreviewSlidePrev(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                    if state.scroll_offset > 0 {
                        state.scroll_offset -= 1;
                    }
                }
            }
            Message::PreviewSlideNext(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Preview { state }) = tab.panes.get_mut(pane) {
                    if let preview::PreviewContent::Slides { images, .. } = &state.content {
                        if state.scroll_offset + 1 < images.len() {
                            state.scroll_offset += 1;
                        }
                    }
                }
            }

            // -- Hotkey Info --
            Message::ShowHotkeyInfo => {
                // Check if a hotkey info pane already exists — focus it instead.
                let tab = self.active_tab_mut();
                let existing = tab.panes.iter()
                    .find(|(_p, b)| b.is_hotkey_info())
                    .map(|(p, _)| *p);
                if let Some(info_pane) = existing {
                    tab.focus = Some(info_pane);
                    return Task::none();
                }
                // Open a new hotkey info pane.
                let block = Block::new_hotkey_info();
                self.add_window(block);
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

            Message::KeyboardInput(key, modified_key, modifiers) => {
                // While an inline rename is active, the text field owns the
                // keyboard: Escape cancels, everything else goes to the field
                // (don't run shortcuts or forward to a PTY).
                if self.rename.is_some() {
                    if matches!(key, Key::Named(Named::Escape)) {
                        return self.update(Message::RenameCancel);
                    }
                    return Task::none();
                }

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

                // If the focused pane is an AI chat or settings panel, don't
                // forward keyboard input to a PTY. Their iced widgets handle it.
                {
                    let tab = self.active_tab();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get(focused) {
                            if block.is_ai_chat() || block.is_settings() || block.is_browser() || block.is_preview() || block.is_hotkey_info() {
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
                if let Some(bytes) = key_to_bytes(&key, &modified_key, &modifiers) {
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
                            // Send raw text directly. Wrapping with bracketed-
                            // paste markers (\x1b[200~ … \x1b[201~) causes bash
                            // to insert the text but defer redrawing the prompt
                            // until the next input event, so the paste appears
                            // invisible until the user hits a key. Sending raw
                            // bytes makes the paste show up immediately.
                            //
                            // Caveat: multi-line pastes will execute each line
                            // as a separate command (legacy pre-bracketed-paste
                            // behavior).
                            block.write_input(text.as_bytes());
                            block.reset_cursor_blink();
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

            // -- Theme toggle --
            Message::ToggleTheme => {
                let new_theme = theme_partner(&self.config.appearance.theme).to_string();
                self.config.appearance.theme = new_theme;
                if let Err(e) = self.config.save(&AppConfig::config_path()) {
                    log::error!("Failed to save theme: {e}");
                }
                // Sync any open settings panes so their working copy matches.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::Settings { state } = block {
                            state.config.appearance.theme = self.config.appearance.theme.clone();
                        }
                    }
                }
            }

            // -- Window handle --
            Message::WindowHandleReady(xid) => {
                log::debug!("[WINDOW] Got raw window ID: {xid} (hex: {xid:#x})");
                self.parent_xid = Some(xid);
                // Create any browser webviews for restored panes now that the XID is available.
                self.ensure_browser_webviews();
            }

            // -- Session persistence --
            Message::SaveSession => {
                if self.config.session.restore {
                    self.save_session();
                }
            }
            Message::WindowCloseRequested => {
                if self.config.session.restore {
                    self.save_session();
                }
                return iced::exit();
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
        let editing_tab = match self.rename {
            Some(RenameTarget::Tab(i)) => Some(i),
            _ => None,
        };
        let tab_bar = tab_bar_view(
            &titles,
            self.active_tab,
            editing_tab,
            &self.rename_buffer,
            rename_input_id(),
            Message::TabBarAction,
        );

        // Pane grid for the active tab
        let light_mode = is_config_light_theme(&self.config.appearance.theme);
        let is_maximized = tab.panes.maximized().is_some();
        let has_terminal_context = self.terminal_context(1).is_some();
        // Inline pane-rename state captured for the pane-grid closure below.
        let editing_pane = match self.rename {
            Some(RenameTarget::Pane(p)) => Some(p),
            _ => None,
        };
        let rename_buffer = self.rename_buffer.as_str();
        let pane_labels = &tab.pane_labels;
        let rename_id = rename_input_id();
        // The active iced theme, so panes (e.g. the hotkey panel) can style
        // themselves to it during view construction.
        let current_theme = theme_from_config(&self.config.appearance.theme);
        let pane_grid_widget =
            pane_grid::PaneGrid::new(&tab.panes, |pane, block, _maximized| {
                let is_focused = focus == Some(pane);

                // Build content based on block type.
                let content: Element<'_, Message> = match block {
                    Block::Terminal { .. } => {
                        let grid = block.render_grid(light_mode);
                        let terminal_view = TerminalView::new(grid)
                            .with_font_size(self.config.appearance.font_size)
                            .with_font_family(self.terminal_font_family);
                        terminal_view.view(
                            Message::TerminalSelected,
                            move |pos| Message::ContextMenuOpen(pane, pos),
                        )
                    }
                    Block::AIChat { state } => {
                        ai_chat_view(pane, state, has_terminal_context)
                    }
                    Block::Settings { state } => {
                        settings_view(pane, state, &self.available_fonts)
                    }
                    Block::Browser { state } => {
                        browser_view(pane, state)
                    }
                    Block::Preview { state } => {
                        preview_view(pane, state)
                    }
                    Block::HotkeyInfo => {
                        hotkey_info_view(&current_theme)
                    }
                };

                // Title bar label: a custom pane label if set, otherwise the
                // block's default title (e.g. "Terminal"). Double-click to edit;
                // while editing this pane, show an inline text field instead.
                let title: Element<'_, Message> = if editing_pane == Some(pane) {
                    text_input("", rename_buffer)
                        .id(rename_id.clone())
                        .on_input(Message::RenameInputChanged)
                        .on_submit(Message::RenameCommit)
                        .size(12)
                        .padding(Padding::from([2, 6]))
                        .width(Length::Fixed(180.0))
                        .into()
                } else {
                    let label = pane_labels
                        .get(&pane)
                        .cloned()
                        .unwrap_or_else(|| block.title());
                    // A flat button (rather than a plain label) so it captures
                    // the press — otherwise the pane-grid title bar treats the
                    // area as a drag handle and the click never registers. The
                    // app turns a quick second click into a rename.
                    // No wrapping: a long title stays one line so this pane's
                    // title bar can't grow taller than its neighbors'. We use
                    // `responsive` to learn the title slot's width and truncate
                    // with an ellipsis that ends *before* the right edge instead
                    // of running off the title bar.
                    responsive(move |size| {
                        let shown = truncate_to_width(&label, size.width);
                        button(text(shown).size(12).wrapping(text::Wrapping::None))
                            .padding(Padding::from([0, 2]))
                            .on_press(Message::PaneTitleClicked(pane))
                            .style(move |theme: &Theme, _status| iced::widget::button::Style {
                                background: None,
                                text_color: if is_focused {
                                    chrome::accent_text(theme)
                                } else {
                                    chrome::text_muted(theme)
                                },
                                border: Border::default(),
                                ..Default::default()
                            })
                            .into()
                    })
                    .height(Length::Shrink)
                    .into()
                };

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
        let sidebar = sidebar_view(Message::SidebarAction, light_mode);

        // Pad the grid so the panes sit inset from the surrounding chrome.
        let padded_grid = container(pane_grid_widget)
            .width(Fill)
            .height(Fill)
            .padding(GRID_PADDING);

        // Layout: tab bar on top, then [pane_grid | sidebar] below
        let content_row = row![padded_grid, sidebar];
        let layout = column![tab_bar, content_row];

        let base: Element<'_, Message> = container(layout)
            .width(Fill)
            .height(Fill)
            // Fill the canvas behind the panes with the same color as the tab
            // bar and sidebar so the gaps between panes blend with the chrome
            // instead of showing the bare (black) app background.
            .style(|theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(chrome::bg_subtle(theme))),
                ..Default::default()
            })
            .into();

        // Overlays: command palette and context menu can stack on top of base.
        let mut layered = base;
        if let Some(menu) = self.context_menu {
            let overlay = self.context_menu_overlay(menu);
            layered = stack![layered, opaque(overlay)].into();
        }
        if self.palette.visible {
            let overlay = self.palette_overlay();
            layered = stack![layered, opaque(overlay)].into();
        }
        layered
    }

    /// Build the right-click context menu overlay.
    fn context_menu_overlay(&self, menu: ContextMenuState) -> Element<'_, Message> {
        let has_selection = !self.last_selection.is_empty();

        let item = |label: &str, message: Option<Message>| -> Element<'_, Message> {
            let enabled = message.is_some();
            let btn = button(
                text(label.to_string())
                    .size(13)
                    .color(if enabled {
                        Color::from_rgb(0.92, 0.92, 0.94)
                    } else {
                        Color::from_rgb(0.50, 0.50, 0.55)
                    }),
            )
            .width(Fill)
            .padding(Padding { top: 5.0, right: 10.0, bottom: 5.0, left: 10.0 })
            .style(|_theme: &Theme, status| iced::widget::button::Style {
                background: Some(Background::Color(match status {
                    iced::widget::button::Status::Hovered => Color::from_rgb(0.22, 0.28, 0.42),
                    _ => Color::TRANSPARENT,
                })),
                text_color: Color::from_rgb(0.92, 0.92, 0.94),
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..iced::widget::button::Style::default()
            });
            let btn = if let Some(msg) = message {
                btn.on_press(msg)
            } else {
                btn
            };
            btn.into()
        };

        let menu_items = column![
            item("Copy", has_selection.then_some(Message::ContextMenuCopy)),
            item("Paste", Some(Message::ContextMenuPaste)),
            item("Select All", Some(Message::ContextMenuSelectAll)),
            item("Clear", Some(Message::ContextMenuClear)),
        ]
        .spacing(1)
        .width(Length::Fixed(150.0));

        let menu_box = container(menu_items)
            .padding(4)
            .style(|_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgb(0.13, 0.14, 0.18))),
                border: Border {
                    color: Color::from_rgb(0.30, 0.34, 0.45),
                    width: 1.0,
                    radius: 5.0.into(),
                },
                ..Default::default()
            });

        // Clamp the menu position so it stays within the window. The menu is
        // roughly 150 px wide and a few rows tall; using 160 / 140 as a
        // conservative footprint keeps it on-screen near right/bottom edges.
        let menu_w = 160.0_f32;
        let menu_h = 140.0_f32;
        let left = menu
            .position
            .x
            .min((self.window_width - menu_w).max(0.0))
            .max(0.0);
        let top = menu
            .position
            .y
            .min((self.window_height - menu_h).max(0.0))
            .max(0.0);

        let positioned = container(menu_box)
            .padding(Padding { top, left, right: 0.0, bottom: 0.0 });

        // Wrap in a Fill mouse_area so any click outside the menu closes it.
        // The button widgets inside still receive their own clicks first.
        mouse_area(
            container(positioned)
                .width(Fill)
                .height(Fill),
        )
        .on_press(Message::ContextMenuClose)
        .into()
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
            let bg_color_light = if is_selected {
                Color::from_rgb(0.70, 0.80, 0.95)
            } else {
                Color::from_rgb(0.92, 0.92, 0.94)
            };
            let text_color = if is_selected {
                Color::from_rgb(1.0, 1.0, 1.0)
            } else {
                Color::from_rgb(0.75, 0.75, 0.75)
            };
            let text_color_light = if is_selected {
                Color::from_rgb(0.05, 0.05, 0.10)
            } else {
                Color::from_rgb(0.25, 0.25, 0.30)
            };

            // Text color is set via container's text_color override (theme-aware).
            let label = text(&cmd.label).size(13);
            let shortcut = text(&cmd.shortcut).size(11);

            let item_row = row![label, iced::widget::space().width(Fill), shortcut]
                .spacing(8)
                .align_y(iced::Alignment::Center);

            let item_container: Element<'_, Message> = container(item_row)
                .width(Fill)
                .padding(6)
                .style(move |theme: &Theme| {
                    let light = is_light_theme(theme);
                    iced::widget::container::Style {
                        background: Some(Background::Color(
                            if light { bg_color_light } else { bg_color }
                        )),
                        text_color: Some(
                            if light { text_color_light } else { text_color }
                        ),
                        ..Default::default()
                    }
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
            .style(|theme: &Theme| {
                let light = is_light_theme(theme);
                iced::widget::container::Style {
                    background: Some(Background::Color(if light {
                        Color::from_rgb(0.94, 0.94, 0.96)
                    } else {
                        Color::from_rgb(0.10, 0.10, 0.13)
                    })),
                    border: Border {
                        color: if light {
                            Color::from_rgb(0.60, 0.70, 0.90)
                        } else {
                            Color::from_rgb(0.30, 0.45, 0.75)
                        },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            });

        // Center horizontally, place near top
        container(
            container(palette_styled)
                .center_x(Fill)
                .padding(Padding { top: 60.0, right: 0.0, bottom: 0.0, left: 0.0 }),
        )
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| {
            let alpha = if is_light_theme(theme) { 0.3 } else { 0.5 };
            iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, alpha))),
                ..Default::default()
            }
        })
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(8)).map(|_| Message::Tick);

        let save = iced::time::every(Duration::from_secs(30)).map(|_| Message::SaveSession);

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
                    Event::Window(iced::window::Event::CloseRequested) => {
                        return Some(Message::WindowCloseRequested);
                    }
                    _ => {}
                }

                if status == Status::Captured {
                    return None;
                }
                match event {
                    Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modified_key,
                        modifiers,
                        ..
                    }) => Some(Message::KeyboardInput(key, modified_key, modifiers)),
                    _ => None,
                }
            });

        Subscription::batch([tick, events, save])
    }
}

// ---------------------------------------------------------------------------
// AI Chat view
// ---------------------------------------------------------------------------

/// Build the AI chat view for a pane.
fn ai_chat_view<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::AIChatState,
    has_terminal_context: bool,
) -> Element<'a, Message> {
    // ── Header: provider + model selectors ──
    let providers: Vec<String> = vec![
        "openai".into(), "anthropic".into(), "google".into(),
        "xai".into(), "lmstudio".into(), "ollama".into(),
    ];
    let provider_picker = pick_list(
        providers,
        Some(state.provider_name.clone()),
        move |s| Message::AIProviderChanged(pane, s),
    ).text_size(11).padding(Padding::from([2, 6]));

    let model_selector: Element<'a, Message> = if !state.available_models.is_empty() {
        // Dropdown with fetched models
        let mut models = state.available_models.clone();
        // Ensure current model is in the list even if not returned by API
        if !state.model_name.is_empty() && !models.contains(&state.model_name) {
            models.insert(0, state.model_name.clone());
        }
        pick_list(
            models,
            Some(state.model_name.clone()),
            move |selected| Message::AIModelChanged(pane, selected),
        )
        .text_size(11)
        .padding(Padding::from([2, 6]))
        .width(Length::Fixed(220.0))
        .into()
    } else if state.models_loading {
        container(
            text("Loading models...").size(10).color(Color::from_rgb(0.50, 0.50, 0.55))
        )
        .padding(Padding::from([4, 8]))
        .into()
    } else {
        // Fallback: text input if fetch failed or hasn't run yet
        text_input("model name", &state.model_name)
            .on_input(move |v| Message::AIModelChanged(pane, v))
            .size(11)
            .padding(Padding::from([2, 6]))
            .width(Length::Fixed(180.0))
            .into()
    };

    let context_text = if has_terminal_context {
        text("Context: Terminal").size(10).color(Color::from_rgb(0.40, 0.70, 0.50))
    } else {
        text("Context: none").size(10).color(Color::from_rgb(0.40, 0.40, 0.45))
    };

    let header: Element<'a, Message> = container(
        row![provider_picker, model_selector, iced::widget::space().width(Fill), context_text]
            .spacing(6).align_y(iced::Alignment::Center)
    )
    .width(Fill)
    .padding(Padding::from([4, 8]))
    .style(|theme: &Theme| iced::widget::container::Style {
        background: Some(Background::Color(chrome::bg_subtle(theme))),
        ..Default::default()
    })
    .into();

    // ── Messages ──
    let mut msg_widgets: Vec<Element<'a, Message>> = Vec::new();

    for msg in &state.messages {
        // Use the model name stored WITH the message, not the current model
        let msg_model = msg.model.as_deref().unwrap_or(&state.model_name);
        let (label, color) = match msg.role.as_str() {
            "user" => ("You:".to_string(), Color::from_rgb(0.40, 0.70, 1.0)),
            "assistant" => (format!("{}:", friendly_model_name(msg_model)), Color::from_rgb(0.40, 0.85, 0.55)),
            "error" => ("Error:".to_string(), Color::from_rgb(0.95, 0.40, 0.35)),
            _ => ("System:".to_string(), Color::from_rgb(0.60, 0.60, 0.65)),
        };
        let copy_btn = button(text("Copy").size(9))
            .on_press(Message::AICopyMessage(msg.content.clone()))
            .padding(Padding::from([2, 6]))
            .style(|theme: &Theme, status: button::Status| {
                let light = is_light_theme(theme);
                let bg = match (light, status) {
                    (true, button::Status::Hovered) => Color::from_rgb(0.85, 0.85, 0.90),
                    (false, button::Status::Hovered) => Color::from_rgb(0.20, 0.20, 0.25),
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: if light {
                        Color::from_rgb(0.40, 0.40, 0.50)
                    } else {
                        Color::from_rgb(0.45, 0.45, 0.50)
                    },
                    border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 3.0.into() },
                    ..Default::default()
                }
            });
        let header_row = row![
            text(label).size(11).color(color),
            iced::widget::space().width(Fill),
            copy_btn
        ].align_y(iced::Alignment::Center);

        msg_widgets.push(
            column![
                header_row,
                text(&msg.content).size(13),
            ]
            .spacing(2)
            .padding(Padding::from([6, 10]))
            .into(),
        );
    }

    if state.streaming && !state.current_response.is_empty() {
        let streaming_name = friendly_model_name(
            state.streaming_model.as_deref().unwrap_or(&state.model_name)
        );
        msg_widgets.push(
            column![
                text(format!("{}:", streaming_name)).size(11).color(Color::from_rgb(0.40, 0.85, 0.55)),
                text(format!("{}\u{2588}", state.current_response)).size(13),
            ]
            .spacing(2)
            .padding(Padding::from([6, 10]))
            .into(),
        );
    } else if state.streaming {
        let waiting: Element<'a, Message> = container(
            text("AI is thinking...").size(12)
        ).center_x(Fill).padding(10).into();
        msg_widgets.push(waiting);
    }

    if msg_widgets.is_empty() {
        let hint: Element<'a, Message> = container(
            text("Type a message to start chatting. AI can see your terminal output.").size(12)
        ).center_x(Fill).padding(20).into();
        msg_widgets.push(hint);
    }

    let messages_area: Element<'a, Message> = scrollable(
        Column::from_vec(msg_widgets).spacing(4).width(Fill).padding(4)
    )
    .width(Fill)
    .into();

    // ── Input ──
    let input_field = text_input("Type a message...", &state.input)
        .on_input(move |v| Message::AIInputChanged(pane, v))
        .on_submit(Message::AISendMessage(pane))
        .size(13)
        .padding(Padding::from([8, 10]))
        .id(WidgetId::from(format!("ai-chat-input-{:?}", pane)));

    let mut send_btn = button(text("Send").size(12).center())
        .padding(Padding::from([8, 14]))
        .style(|theme: &Theme, status: button::Status| {
            let light = is_light_theme(theme);
            let bg = match (light, status) {
                (true, button::Status::Hovered) => Color::from_rgb(0.15, 0.45, 0.75),
                (true, button::Status::Pressed) => Color::from_rgb(0.12, 0.38, 0.68),
                (true, _) => Color::from_rgb(0.18, 0.48, 0.78),
                (false, button::Status::Hovered) => Color::from_rgb(0.25, 0.55, 0.85),
                (false, button::Status::Pressed) => Color::from_rgb(0.20, 0.45, 0.75),
                (false, _) => Color::from_rgb(0.22, 0.50, 0.80),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::WHITE,
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 4.0.into() },
                ..Default::default()
            }
        });
    if !state.input.trim().is_empty() && !state.streaming {
        send_btn = send_btn.on_press(Message::AISendMessage(pane));
    }

    let input_area: Element<'a, Message> = container(
        row![input_field, send_btn].spacing(4).align_y(iced::Alignment::Center)
    )
    .width(Fill)
    .padding(Padding::from([4, 6]))
    .style(|theme: &Theme| {
        let light = is_light_theme(theme);
        iced::widget::container::Style {
            background: Some(Background::Color(if light {
                Color::from_rgb(0.93, 0.93, 0.95)
            } else {
                Color::from_rgb(0.07, 0.07, 0.09)
            })),
            border: Border {
                color: if light {
                    Color::from_rgb(0.80, 0.80, 0.85)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.20)
                },
                width: 1.0,
                // Round the bottom corners to match the pane's rounded border.
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
            },
            ..Default::default()
        }
    })
    .into();

    // ── Layout: header on top, messages fill middle, input at bottom ──
    // Key fix: use container with height=Fill for the messages area
    // so the scrollable gets proper space allocation
    let middle = container(messages_area).width(Fill).height(Fill);

    let layout: Element<'a, Message> = column![header, middle, input_area]
        .width(Fill)
        .height(Fill)
        .into();
    layout
        .into()
}

// ---------------------------------------------------------------------------
// Settings view
// ---------------------------------------------------------------------------

/// Build the settings panel view for a pane.
fn settings_view<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::SettingsState,
    available_fonts: &[String],
) -> Element<'a, Message> {
    // ── Header ──
    let title_label = text("Settings").size(16);
    let dirty_indicator = if state.dirty {
        text(" (unsaved)").size(12).color(Color::from_rgb(0.90, 0.65, 0.30))
    } else {
        text("").size(12)
    };

    let mut save_btn = button(text("Save").size(12).center())
        .padding(Padding::from([6, 16]))
        .style(|theme: &Theme, status: button::Status| {
            let light = is_light_theme(theme);
            let bg = match (light, status) {
                (true, button::Status::Hovered) => Color::from_rgb(0.20, 0.55, 0.35),
                (true, button::Status::Pressed) => Color::from_rgb(0.16, 0.46, 0.30),
                (true, _) => Color::from_rgb(0.18, 0.50, 0.33),
                (false, button::Status::Hovered) => Color::from_rgb(0.25, 0.60, 0.40),
                (false, button::Status::Pressed) => Color::from_rgb(0.20, 0.50, 0.35),
                (false, _) => Color::from_rgb(0.22, 0.55, 0.38),
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

    if state.dirty {
        save_btn = save_btn.on_press(Message::SettingsSave(pane));
    }

    let header = container(
        row![title_label, dirty_indicator, iced::widget::space().width(Fill), save_btn]
            .spacing(4)
            .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .padding(Padding::from([8, 12]))
    .style(|theme: &Theme| {
        let light = is_light_theme(theme);
        iced::widget::container::Style {
            background: Some(Background::Color(if light {
                Color::from_rgb(0.92, 0.92, 0.94)
            } else {
                Color::from_rgb(0.08, 0.08, 0.11)
            })),
            text_color: Some(if light {
                Color::from_rgb(0.10, 0.10, 0.15)
            } else {
                Color::from_rgb(0.90, 0.92, 0.96)
            }),
            border: Border {
                color: if light {
                    Color::from_rgb(0.80, 0.80, 0.85)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.20)
                },
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    });

    // ── Section navigation ──
    let sections = [
        (SettingsSection::Appearance, "App"),
        (SettingsSection::AI, "AI"),
        (SettingsSection::Terminal, "Term"),
    ];

    let mut nav_buttons: Vec<Element<'a, Message>> = Vec::new();
    for (section, label) in &sections {
        let is_active = state.active_section == *section;
        let sec = *section;
        let lbl = *label;

        let btn: Element<'a, Message> = button(text(lbl).size(12).center())
            .on_press(Message::SettingsSectionChanged(pane, sec))
            .width(Fill)
            .padding(Padding::from([8, 4]))
            .style(move |theme: &Theme, _status: button::Status| {
                let light = is_light_theme(theme);
                let bg_color = match (light, is_active) {
                    (true, true) => Color::from_rgb(0.85, 0.88, 0.95),
                    (true, false) => Color::from_rgb(0.92, 0.92, 0.94),
                    (false, true) => Color::from_rgb(0.18, 0.22, 0.32),
                    (false, false) => Color::from_rgb(0.10, 0.10, 0.13),
                };
                let text_color = match (light, is_active) {
                    (true, true) => Color::from_rgb(0.10, 0.10, 0.20),
                    (true, false) => Color::from_rgb(0.40, 0.40, 0.50),
                    (false, true) => Color::from_rgb(0.85, 0.90, 1.0),
                    (false, false) => Color::from_rgb(0.55, 0.55, 0.60),
                };
                let border_color = match (light, is_active) {
                    (true, true) => Color::from_rgb(0.40, 0.55, 0.85),
                    (false, true) => Color::from_rgb(0.30, 0.45, 0.75),
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(Background::Color(bg_color)),
                    text_color,
                    border: Border {
                        color: border_color,
                        width: if is_active { 1.0 } else { 0.0 },
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into();

        nav_buttons.push(btn);
    }

    let nav_col = Column::from_vec(nav_buttons)
        .spacing(2)
        .width(Length::Fixed(60.0));

    let nav_panel = container(nav_col)
        .padding(Padding::from([8, 4]))
        .height(Fill)
        .style(|theme: &Theme| {
            let light = is_light_theme(theme);
            iced::widget::container::Style {
                background: Some(Background::Color(if light {
                    Color::from_rgb(0.92, 0.92, 0.94)
                } else {
                    Color::from_rgb(0.07, 0.07, 0.09)
                })),
                border: Border {
                    color: if light {
                        Color::from_rgb(0.82, 0.82, 0.86)
                    } else {
                        Color::from_rgb(0.15, 0.15, 0.18)
                    },
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

    // ── Section content ──
    let section_content: Element<'a, Message> = match state.active_section {
        SettingsSection::Appearance => settings_appearance_section(pane, state, available_fonts),
        SettingsSection::AI => settings_ai_section(pane, state),
        SettingsSection::Terminal => settings_terminal_section(pane, state),
    };

    let content_panel = container(
        scrollable(section_content).width(Fill).height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .padding(Padding::from([12, 16]));

    // ── Assemble: header on top, nav + content side by side below ──
    let body = row![nav_panel, content_panel];

    container(column![header, body])
        .width(Fill)
        .height(Fill)
        // Bottom inset so overflowing scroll content / scrollbar doesn't paint
        // over the rounded bottom corners (iced only clips rectangles).
        .padding(Padding { top: 0.0, right: 0.0, bottom: PANE_CORNER_RADIUS, left: 0.0 })
        .clip(true)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(chrome::bg_base(theme))),
            border: Border {
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Appearance section fields.
fn settings_appearance_section<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::SettingsState,
    available_fonts: &[String],
) -> Element<'a, Message> {
    // Text colors are inherited from the iced theme (Light/Dark) automatically.
    // App version (compiled in via CARGO_PKG_VERSION) shown muted at the top.
    let version_line = text(format!("Alterm v{}", env!("CARGO_PKG_VERSION")))
        .size(11)
        .style(|_theme: &iced::Theme| iced::widget::text::Style {
            color: Some(Color::from_rgb(0.55, 0.55, 0.60)),
        });

    let heading = text("Appearance").size(14);

    // Font size
    let font_size_label = text("Font size").size(12);
    let font_size_input = text_input("14.0", &state.font_size_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::FontSize(val)))
        .size(13)
        .padding(6)
        .width(Length::Fixed(120.0));

    // Theme
    let theme_label = text("Theme").size(12);
    let theme_options: Vec<String> = vec![
        ALTERM_DARK_NAME.to_string(),
        ALTERM_LIGHT_NAME.to_string(),
        "Solarized Dark".to_string(),
        "Solarized Light".to_string(),
        "Gruvbox Dark".to_string(),
        "Gruvbox Light".to_string(),
        "Catppuccin Mocha".to_string(),
        "Catppuccin Latte".to_string(),
    ];
    let selected_theme: Option<String> = Some(state.config.appearance.theme.clone());
    let theme_picker = pick_list(
        theme_options,
        selected_theme,
        move |val: String| Message::SettingsChanged(pane, SettingsField::Theme(val)),
    )
    .text_size(13)
    .padding(6)
    .width(Length::Fixed(120.0));

    // Font family
    let font_family_label = text("Font family").size(12);
    let font_list: Vec<String> = available_fonts.to_vec();
    let current_font = Some(state.config.appearance.font_family.clone());
    let font_family_picker = pick_list(
        font_list,
        current_font,
        move |val: String| Message::SettingsChanged(pane, SettingsField::FontFamily(val)),
    )
    .text_size(13)
    .padding(6)
    .width(Length::Fixed(250.0));

    column![
        version_line,
        iced::widget::space().height(Length::Fixed(6.0)),
        heading,
        iced::widget::space().height(Length::Fixed(12.0)),
        font_size_label,
        font_size_input,
        iced::widget::space().height(Length::Fixed(8.0)),
        theme_label,
        theme_picker,
        iced::widget::space().height(Length::Fixed(8.0)),
        font_family_label,
        font_family_picker,
    ]
    .spacing(4)
    .into()
}

/// AI section fields.
fn settings_ai_section<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::SettingsState,
) -> Element<'a, Message> {
    let heading = text("AI Provider Settings").size(14);

    // Default provider
    let provider_label = text("Default provider").size(12);
    let provider_options: Vec<String> = vec![
        "openai".to_string(),
        "anthropic".to_string(),
        "google".to_string(),
        "xai".to_string(),
        "lmstudio".to_string(),
        "ollama".to_string(),
    ];
    let selected_provider: Option<String> = Some(state.config.ai.default_provider.clone());
    let provider_picker = pick_list(
        provider_options,
        selected_provider,
        move |val: String| Message::SettingsChanged(pane, SettingsField::DefaultProvider(val)),
    )
    .text_size(13)
    .padding(6)
    .width(Length::Fixed(160.0));

    // Model
    let model_label = text("Model").size(12);
    let model_input = text_input("model name", &state.model_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::AIModel(val)))
        .size(13)
        .padding(6)
        .width(Length::Fixed(200.0));

    // API Key
    let api_key_label = text("API key").size(12);
    let api_key_input = text_input("sk-...", &state.api_key_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::AIApiKey(val)))
        .secure(true)
        .size(13)
        .padding(6)
        .width(Length::Fixed(280.0));

    // Temperature
    let temp_label = text(format!("Temperature: {:.2}", state.config.ai.temperature))
        .size(12);
    let temp_slider = slider(
        0.0..=2.0,
        state.config.ai.temperature,
        move |val| Message::SettingsChanged(pane, SettingsField::Temperature(val)),
    )
    .step(0.05)
    .width(Length::Fixed(200.0));

    // Max tokens
    let max_tokens_label = text("Max tokens").size(12);
    let max_tokens_input = text_input("4096", &state.max_tokens_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::MaxTokens(val)))
        .size(13)
        .padding(6)
        .width(Length::Fixed(120.0));

    // System prompt
    let sys_prompt_label = text("System prompt").size(12);
    let sys_prompt_input = text_input("You are a helpful...", &state.system_prompt_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::SystemPrompt(val)))
        .size(13)
        .padding(6)
        .width(Fill);

    column![
        heading,
        iced::widget::space().height(Length::Fixed(12.0)),
        provider_label,
        provider_picker,
        iced::widget::space().height(Length::Fixed(8.0)),
        model_label,
        model_input,
        iced::widget::space().height(Length::Fixed(8.0)),
        api_key_label,
        api_key_input,
        iced::widget::space().height(Length::Fixed(8.0)),
        temp_label,
        temp_slider,
        iced::widget::space().height(Length::Fixed(8.0)),
        max_tokens_label,
        max_tokens_input,
        iced::widget::space().height(Length::Fixed(8.0)),
        sys_prompt_label,
        sys_prompt_input,
    ]
    .spacing(4)
    .into()
}

/// Terminal section fields.
fn settings_terminal_section<'a>(
    pane: pane_grid::Pane,
    state: &'a workspace::SettingsState,
) -> Element<'a, Message> {
    let heading = text("Terminal Settings").size(14);

    // Scrollback lines
    let scrollback_label = text("Scrollback lines").size(12);
    let scrollback_input = text_input("10000", &state.scrollback_text)
        .on_input(move |val| Message::SettingsChanged(pane, SettingsField::ScrollbackLines(val)))
        .size(13)
        .padding(6)
        .width(Length::Fixed(120.0));

    // Cursor blink
    let cursor_blink_toggle = toggler(state.config.terminal.cursor_blink)
        .on_toggle(move |val| Message::SettingsChanged(pane, SettingsField::CursorBlink(val)))
        .label("Cursor blink")
        .text_size(12);

    // Copy on select
    let copy_on_select_toggle = toggler(state.config.terminal.copy_on_select)
        .on_toggle(move |val| Message::SettingsChanged(pane, SettingsField::CopyOnSelect(val)))
        .label("Copy on select")
        .text_size(12);

    column![
        heading,
        iced::widget::space().height(Length::Fixed(12.0)),
        scrollback_label,
        scrollback_input,
        iced::widget::space().height(Length::Fixed(12.0)),
        cursor_blink_toggle,
        iced::widget::space().height(Length::Fixed(8.0)),
        copy_on_select_toggle,
    ]
    .spacing(4)
    .into()
}

// ---------------------------------------------------------------------------
// Browser view
// ---------------------------------------------------------------------------

/// Build the browser view for a pane.
fn browser_view<'a>(
    pane: pane_grid::Pane,
    state: &'a BrowserState,
) -> Element<'a, Message> {
    // ── Navigation bar ──
    let back_label = text("\u{25C0}").size(14).center();
    let mut back_btn = button(back_label)
        .padding(Padding::from([4, 8]))
        .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));
    if state.can_go_back {
        back_btn = back_btn.on_press(Message::BrowserBack(pane));
    }

    let fwd_label = text("\u{25B6}").size(14).center();
    let mut fwd_btn = button(fwd_label)
        .padding(Padding::from([4, 8]))
        .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));
    if state.can_go_forward {
        fwd_btn = fwd_btn.on_press(Message::BrowserForward(pane));
    }

    let reload_label = text("\u{21BB}").size(14).center();
    let reload_btn = button(reload_label)
        .on_press(Message::BrowserReload(pane))
        .padding(Padding::from([4, 8]))
        .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));

    let url_input = text_input("Enter URL...", &state.input_url)
        .on_input(move |v| Message::BrowserUrlChanged(pane, v))
        .on_submit(Message::BrowserNavigate(pane, state.input_url.clone()))
        .size(13)
        .padding(Padding::from([6, 10]))
        .id(WidgetId::from(format!("browser-url-input-{:?}", pane)));

    let nav_bar: Element<'a, Message> = container(
        row![back_btn, fwd_btn, reload_btn, url_input]
            .spacing(4)
            .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .padding(Padding::from([4, 8]))
    .style(|theme: &Theme| {
        let light = is_light_theme(theme);
        iced::widget::container::Style {
            background: Some(Background::Color(if light {
                Color::from_rgb(0.92, 0.92, 0.94)
            } else {
                Color::from_rgb(0.08, 0.08, 0.11)
            })),
            border: Border {
                color: if light {
                    Color::from_rgb(0.80, 0.80, 0.85)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.20)
                },
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    })
    .into();

    // ── Content area ──
    // The real wry webview is rendered as an X11 child window that overlays this area.
    // We render a transparent placeholder so iced reserves the space.
    let webview_area: Element<'a, Message> = container(
        iced::widget::space().width(Fill).height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(|_: &Theme| iced::widget::container::Style {
        // Transparent so the native webview shows through.
        background: Some(Background::Color(Color::TRANSPARENT)),
        ..Default::default()
    })
    .into();

    // ── Layout: nav bar on top, webview area fills the rest ──
    container(column![nav_bar, webview_area])
        .width(Fill)
        .height(Fill)
        .clip(true)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(chrome::bg_base(theme))),
            border: Border {
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Style for browser navigation buttons (back, forward, reload).
fn nav_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let light = is_light_theme(theme);
    let bg = match (light, status) {
        (true, button::Status::Hovered) => Color::from_rgb(0.82, 0.82, 0.88),
        (true, button::Status::Pressed) => Color::from_rgb(0.78, 0.78, 0.84),
        (true, button::Status::Disabled) => Color::from_rgb(0.92, 0.92, 0.94),
        (true, _) => Color::from_rgb(0.88, 0.88, 0.92),
        (false, button::Status::Hovered) => Color::from_rgb(0.20, 0.20, 0.28),
        (false, button::Status::Pressed) => Color::from_rgb(0.25, 0.25, 0.32),
        (false, button::Status::Disabled) => Color::from_rgb(0.08, 0.08, 0.10),
        (false, _) => Color::from_rgb(0.12, 0.12, 0.16),
    };
    let text_color = match (light, status) {
        (true, button::Status::Disabled) => Color::from_rgb(0.65, 0.65, 0.70),
        (true, _) => Color::from_rgb(0.20, 0.20, 0.25),
        (false, button::Status::Disabled) => Color::from_rgb(0.30, 0.30, 0.35),
        (false, _) => Color::from_rgb(0.80, 0.80, 0.85),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: if light {
                Color::from_rgb(0.78, 0.78, 0.82)
            } else {
                Color::from_rgb(0.18, 0.18, 0.22)
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// File Preview view
// ---------------------------------------------------------------------------

/// Build the file preview view for a pane.
fn preview_view<'a>(
    pane: pane_grid::Pane,
    state: &'a PreviewState,
) -> Element<'a, Message> {
    // ── Path bar ──
    // Truncate from the front so the current directory name is always visible.
    let path_full = state.path.display().to_string();
    let path_display = {
        let chars: Vec<char> = path_full.chars().collect();
        if chars.len() > 42 {
            // Keep last 41 chars; try to break on a '/' boundary.
            let raw_start = chars.len() - 41;
            let start = chars[raw_start..]
                .iter()
                .position(|&c| c == '/')
                .map(|i| raw_start + i)
                .unwrap_or(raw_start);
            format!("\u{2026}{}", chars[start..].iter().collect::<String>())
        } else {
            path_full
        }
    };
    let path_label = text(format!("  {path_display}"))
        .size(13)
        .color(Color::from_rgb(0.75, 0.80, 0.90))
        .width(Fill);

    let parent_btn = button(
        text("\u{2191} Up").size(12).center(),
    )
    .on_press(Message::PreviewParent(pane))
    .padding(Padding::from([3, 8]))
    .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));

    let path_bar: Element<'a, Message> = container(
        row![path_label, parent_btn]
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .padding(Padding::from([4, 8]))
    .style(|theme: &Theme| {
        let light = is_light_theme(theme);
        iced::widget::container::Style {
            background: Some(Background::Color(if light {
                Color::from_rgb(0.92, 0.92, 0.94)
            } else {
                Color::from_rgb(0.08, 0.08, 0.11)
            })),
            border: Border {
                color: if light {
                    Color::from_rgb(0.80, 0.80, 0.85)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.20)
                },
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    })
    .into();

    // ── Content area ──
    let content_area: Element<'a, Message> = match &state.content {
        preview::PreviewContent::HighlightedCode(lines) => {
            let mut code_rows: Vec<Element<'a, Message>> = Vec::with_capacity(lines.len());
            for line in lines {
                let line_num = text(format!("{:>4} ", line.line_number))
                    .size(13)
                    .color(Color::from_rgb(0.35, 0.35, 0.42))
                    .font(iced::Font::MONOSPACE);

                let mut span_elements: Vec<Element<'a, Message>> = Vec::new();
                for span in &line.spans {
                    let fg = Color::from_rgb(
                        span.fg.0 as f32 / 255.0,
                        span.fg.1 as f32 / 255.0,
                        span.fg.2 as f32 / 255.0,
                    );
                    let mut t = text(&span.text)
                        .size(13)
                        .color(fg)
                        .font(iced::Font::MONOSPACE);
                    if span.bold {
                        t = t.font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..iced::Font::MONOSPACE
                        });
                    }
                    span_elements.push(t.into());
                }

                let spans_row: Element<'a, Message> = iced::widget::Row::from_vec(span_elements)
                    .into();

                let line_row: Element<'a, Message> = row![line_num, spans_row]
                    .align_y(iced::Alignment::Start)
                    .into();

                code_rows.push(line_row);
            }

            let code_column = Column::from_vec(code_rows).spacing(0);

            scrollable(
                container(code_column)
                    .width(Fill)
                    .padding(Padding::from([4, 8])),
            )
            .width(Fill)
            .into()
        }

        preview::PreviewContent::Text(content) => {
            let text_widget = text(content)
                .size(13)
                .color(Color::from_rgb(0.80, 0.80, 0.82))
                .font(iced::Font::MONOSPACE);

            scrollable(
                container(text_widget)
                    .width(Fill)
                    .padding(Padding::from([8, 12])),
            )
            .width(Fill)
            .into()
        }

        preview::PreviewContent::Directory(entries) => {
            let mut entry_rows: Vec<Element<'a, Message>> = Vec::new();

            for entry in entries {
                let icon = if entry.is_dir { "\u{1F4C1}" } else { "\u{1F4C4}" };
                let icon_text = text(icon).size(13);

                let name_color = if entry.is_dir {
                    Color::from_rgb(0.40, 0.65, 0.95)
                } else {
                    Color::from_rgb(0.80, 0.80, 0.82)
                };
                let name_text = text(&entry.name)
                    .size(13)
                    .color(name_color);

                let size_label = if entry.is_dir {
                    text("[dir]").size(11).color(Color::from_rgb(0.45, 0.45, 0.50))
                } else {
                    text(format_size(entry.size)).size(11).color(Color::from_rgb(0.45, 0.45, 0.50))
                };

                let entry_path = state.path.join(&entry.name).to_string_lossy().to_string();
                let entry_row = button(
                    row![icon_text, name_text, iced::widget::space().width(Fill), size_label]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                )
                .on_press(Message::PreviewNavigate(pane, entry_path))
                .width(Fill)
                .padding(Padding::from([4, 8]))
                .style(|theme: &Theme, status: button::Status| {
                    let light = is_light_theme(theme);
                    let bg = match (light, status) {
                        (true, button::Status::Hovered) => Color::from_rgb(0.88, 0.90, 0.95),
                        (true, button::Status::Pressed) => Color::from_rgb(0.84, 0.86, 0.92),
                        (false, button::Status::Hovered) => Color::from_rgb(0.12, 0.14, 0.20),
                        (false, button::Status::Pressed) => Color::from_rgb(0.15, 0.17, 0.24),
                        _ => Color::TRANSPARENT,
                    };
                    button::Style {
                        background: Some(Background::Color(bg)),
                        text_color: if light {
                            Color::from_rgb(0.15, 0.15, 0.20)
                        } else {
                            Color::from_rgb(0.80, 0.80, 0.82)
                        },
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 2.0.into(),
                        },
                        ..Default::default()
                    }
                });

                entry_rows.push(entry_row.into());
            }

            let dir_column = Column::from_vec(entry_rows).spacing(1).width(Fill);

            scrollable(
                container(dir_column)
                    .width(Fill)
                    .padding(Padding::from([4, 4])),
            )
            .width(Fill)
            .height(Fill)
            .into()
        }

        preview::PreviewContent::Image => {
            let handle = iced::widget::image::Handle::from_path(&state.path);
            container(
                iced::widget::image(handle)
                    .content_fit(iced::ContentFit::Contain)
                    .width(Fill)
                    .height(Fill),
            )
            .width(Fill)
            .height(Fill)
            .into()
        }

        preview::PreviewContent::Svg => {
            let handle = iced::widget::svg::Handle::from_path(&state.path);
            container(
                iced::widget::svg(handle)
                    .content_fit(iced::ContentFit::Contain)
                    .width(Fill)
                    .height(Fill),
            )
            .width(Fill)
            .height(Fill)
            .into()
        }

        preview::PreviewContent::Converting => {
            container(
                column![
                    text("Converting slides\u{2026}").size(15).color(Color::from_rgb(0.65, 0.75, 0.95)),
                    text("LibreOffice is rendering your presentation.")
                        .size(12)
                        .color(Color::from_rgb(0.50, 0.50, 0.55)),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
        }

        preview::PreviewContent::Slides { images, .. } => {
            let total = images.len();
            let idx = state.scroll_offset.min(total.saturating_sub(1));

            // Navigation bar
            let slide_label = text(format!("Slide {} / {}", idx + 1, total))
                .size(13)
                .color(Color::from_rgb(0.80, 0.82, 0.90));

            let mut prev_btn = button(text("\u{25C0}").size(13).center())
                .padding(Padding::from([3, 10]))
                .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));
            if idx > 0 {
                prev_btn = prev_btn.on_press(Message::PreviewSlidePrev(pane));
            }

            let mut next_btn = button(text("\u{25B6}").size(13).center())
                .padding(Padding::from([3, 10]))
                .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));
            if idx + 1 < total {
                next_btn = next_btn.on_press(Message::PreviewSlideNext(pane));
            }

            let slide_nav: Element<'a, Message> = container(
                row![prev_btn, slide_label, next_btn]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
            )
            .width(Fill)
            .padding(Padding::from([4, 8]))
            .style(|theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(chrome::bg_subtle(theme))),
                ..Default::default()
            })
            .into();

            // Slide image
            let handle = iced::widget::image::Handle::from_path(&images[idx]);
            let slide_img: Element<'a, Message> = container(
                iced::widget::image(handle)
                    .content_fit(iced::ContentFit::Contain)
                    .width(Fill)
                    .height(Fill),
            )
            .width(Fill)
            .height(Fill)
            .into();

            column![slide_nav, slide_img]
                .width(Fill)
                .height(Fill)
                .into()
        }

        preview::PreviewContent::Unsupported(msg) => {
            container(
                text(msg)
                    .size(14)
                    .color(Color::from_rgb(0.70, 0.40, 0.40)),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
        }
    };

    // ── Wrap content ──
    // Round the bottom corners to match the pane's rounded border (this is the
    // element that reaches the bottom of the pane). A small bottom inset keeps
    // overflowing content and the scrollbar from painting over the rounded
    // corners (iced only supports rectangular clipping, so the rounded
    // background would otherwise be covered in tiled/overflow layouts).
    let content_styled: Element<'a, Message> = container(content_area)
        .width(Fill)
        .height(Fill)
        .padding(Padding { top: 0.0, right: 0.0, bottom: PANE_CORNER_RADIUS, left: 0.0 })
        .clip(true)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(chrome::bg_base(theme))),
            border: Border {
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

    // ── Layout: path bar on top, content fills the rest ──
    container(column![path_bar, content_styled].width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .clip(true)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(chrome::bg_base(theme))),
            border: Border {
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Format a file size in human-readable form.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ---------------------------------------------------------------------------
// Hotkey Info view
// ---------------------------------------------------------------------------

/// Build the hotkey info reference pane showing all keyboard shortcuts.
/// Linear interpolation between two colors.
fn lerp_color(a: Color, b: Color, f: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * f,
        g: a.g + (b.g - a.g) * f,
        b: a.b + (b.b - a.b) * f,
        a: 1.0,
    }
}

fn hotkey_info_view<'a>(theme: &Theme) -> Element<'a, Message> {
    // Colors derived from the active theme so the panel is readable on any
    // theme — the brand orchid/violet on the Alterm themes, palette-correct
    // colors elsewhere (and dark-on-light text on light themes).
    let accent = chrome::accent(theme); // section headings + key combos
    let heading_color = chrome::text(theme); // panel title
    let shortcut_color = chrome::accent(theme); // key combos
    let label_color = chrome::text(theme); // descriptions
    let dim_color = chrome::text_muted(theme); // mouse / secondary
    // Logo gradient: accent → readable text color, so it always contrasts with
    // the background (light-ward on dark themes, dark-ward on light themes).
    let logo_start = accent;
    let logo_end = heading_color;

    // ── ASCII logo header, painted with the same left→right dark-to-light
    //    gradient as the terminal startup logo (one shade per column). ──
    let logo_text = include_str!("../../assets/ascii_logo.txt");
    let max_cols = logo_text
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(2);
    let mut logo_spans: Vec<iced::widget::text::Span<'a, Message, iced::Font>> = Vec::new();
    for line in logo_text.lines() {
        for (col, ch) in line.chars().enumerate() {
            let t = col as f32 / (max_cols - 1) as f32;
            logo_spans.push(
                span(ch.to_string())
                    .font(iced::Font::MONOSPACE)
                    .size(11)
                    .color(lerp_color(logo_start, logo_end, t)),
            );
        }
        logo_spans.push(span("\n").font(iced::Font::MONOSPACE).size(11));
    }
    let logo: Element<'a, Message> = container(rich_text(logo_spans))
        .width(Fill)
        .padding(Padding { top: 12.0, right: 16.0, bottom: 4.0, left: 16.0 })
        .into();

    // ── Title ──
    let title: Element<'a, Message> = container(
        text("Keyboard Shortcuts").size(16).color(heading_color),
    )
    .width(Fill)
    .padding(Padding { top: 4.0, right: 16.0, bottom: 8.0, left: 16.0 })
    .into();

    // ── Build shortcut rows from the keybinding registry ──
    let all_actions = all_palette_actions();

    // Categorize actions
    let tab_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::NewTab | Action::CloseTab | Action::NextTab | Action::PrevTab | Action::RenameTab
    )).collect();
    // Also include JumpToTab as a hardcoded entry
    let pane_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::SplitRight | Action::SplitDown | Action::ClosePane | Action::MaximizeToggle |
        Action::FocusUp | Action::FocusDown | Action::FocusLeft | Action::FocusRight
    )).collect();
    let tool_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::ToggleAIChat | Action::CommandPalette | Action::OpenSettings
    )).collect();
    let windows_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::NewTerminal | Action::NewBrowser | Action::NewPreview |
        Action::ShowHotkeyInfo | Action::ToggleTheme
    )).collect();
    let terminal_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::Copy | Action::Paste | Action::ScrollPageUp | Action::ScrollPageDown | Action::Search
    )).collect();

    let mut items: Vec<Element<'a, Message>> = Vec::new();

    // Helper to build a category section
    fn build_section<'a>(
        category: &'a str,
        actions: &[&Action],
        extra_rows: &[(&'a str, &'a str)],
        accent: Color,
        shortcut_color: Color,
        label_color: Color,
    ) -> Vec<Element<'a, Message>> {
        let mut elems: Vec<Element<'a, Message>> = Vec::new();

        // Category heading
        elems.push(
            container(
                text(category).size(13).color(accent).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::MONOSPACE
                }),
            )
            .padding(Padding { top: 10.0, right: 16.0, bottom: 4.0, left: 16.0 })
            .into(),
        );

        // Action rows from registry
        for action in actions {
            let shortcut_str = action.shortcut_hint();
            let label_str = action.label();
            let row_widget = row![
                text(shortcut_str)
                    .size(12)
                    .color(shortcut_color)
                    .font(iced::Font::MONOSPACE)
                    .width(Length::Fixed(160.0)),
                text(label_str)
                    .size(12)
                    .color(label_color),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center);

            elems.push(
                container(row_widget)
                    .padding(Padding { top: 2.0, right: 16.0, bottom: 2.0, left: 24.0 })
                    .into(),
            );
        }

        // Extra hardcoded rows
        for (shortcut, desc) in extra_rows {
            let row_widget = row![
                text(*shortcut)
                    .size(12)
                    .color(shortcut_color)
                    .font(iced::Font::MONOSPACE)
                    .width(Length::Fixed(160.0)),
                text(*desc)
                    .size(12)
                    .color(label_color),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center);

            elems.push(
                container(row_widget)
                    .padding(Padding { top: 2.0, right: 16.0, bottom: 2.0, left: 24.0 })
                    .into(),
            );
        }

        elems
    }

    // Tabs section (add JumpToTab as extra since it's excluded from palette)
    items.extend(build_section(
        "TABS",
        &tab_actions,
        &[("Ctrl+1-9", "Jump to Tab")],
        accent, shortcut_color, label_color,
    ));

    // Panes section (add Navigate Panes summary)
    items.extend(build_section(
        "PANES",
        &pane_actions,
        &[],
        accent, shortcut_color, label_color,
    ));

    // AI & Tools section
    items.extend(build_section(
        "AI & TOOLS",
        &tool_actions,
        &[],
        accent, shortcut_color, label_color,
    ));

    // Windows section (new-block / tool-window shortcuts)
    items.extend(build_section(
        "WINDOWS",
        &windows_actions,
        &[],
        accent, shortcut_color, label_color,
    ));

    // Terminal section (add ScrollUp/Down as extras)
    items.extend(build_section(
        "TERMINAL",
        &terminal_actions,
        &[
            ("Shift+Up", "Scroll Up"),
            ("Shift+Down", "Scroll Down"),
        ],
        accent, shortcut_color, label_color,
    ));

    // Mouse section (all hardcoded)
    items.push(
        container(
            text("MOUSE").size(13).color(accent).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..iced::Font::MONOSPACE
            }),
        )
        .padding(Padding { top: 10.0, right: 16.0, bottom: 4.0, left: 16.0 })
        .into(),
    );
    let mouse_rows: Vec<(&str, &str)> = vec![
        ("Double-click tab", "Rename tab"),
        ("Double-click pane title", "Rename / label pane"),
        ("Drag title bar", "Rearrange panes"),
        ("Drag split edge", "Resize panes"),
        ("Scroll wheel", "Terminal scrollback"),
    ];
    for (shortcut, desc) in mouse_rows {
        let row_widget = row![
            text(shortcut)
                .size(12)
                .color(dim_color)
                .font(iced::Font::MONOSPACE)
                .width(Length::Fixed(160.0)),
            text(desc)
                .size(12)
                .color(label_color),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        items.push(
            container(row_widget)
                .padding(Padding { top: 2.0, right: 16.0, bottom: 2.0, left: 24.0 })
                .into(),
        );
    }

    // Bottom spacing
    items.push(iced::widget::space().height(Length::Fixed(20.0)).into());

    let content_column = Column::from_vec(items).spacing(0).width(Fill);

    let scrollable_content: Element<'a, Message> = scrollable(
        content_column,
    )
    .width(Fill)
    .height(Fill)
    .into();

    // ── Full layout ──
    let layout = column![logo, title, scrollable_content]
        .width(Fill)
        .height(Fill);

    container(layout)
        .width(Fill)
        .height(Fill)
        // Bottom inset so overflowing scroll content / scrollbar doesn't paint
        // over the rounded bottom corners (iced only clips rectangles).
        .padding(Padding { top: 0.0, right: 0.0, bottom: PANE_CORNER_RADIUS, left: 0.0 })
        .clip(true)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(chrome::bg_base(theme))),
            border: Border {
                radius: iced::border::bottom(PANE_CORNER_RADIUS),
                ..Default::default()
            },
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
                "google" => {
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

/// Truncate a pane title to fit `width` pixels, appending an ellipsis so it
/// ends before the right edge of the title bar instead of running off it.
/// Uses an estimated average glyph width for the size-12 title font; the
/// estimate runs slightly wide so the result errs on the side of fitting.
fn truncate_to_width(label: &str, width: f32) -> String {
    // ~7px per glyph at size 12, minus the button's horizontal padding (2+2)
    // and a little slack for the ellipsis glyph.
    const AVG_GLYPH_PX: f32 = 7.0;
    let usable = (width - 6.0).max(0.0);
    let max_chars = (usable / AVG_GLYPH_PX).floor() as usize;

    let len = label.chars().count();
    if len <= max_chars {
        return label.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let mut out: String = label.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

/// Build a small, styled button for the pane title bar.
fn title_bar_button(label: &str, on_press: Message) -> Element<'_, Message> {
    button(text(label).size(12).center())
        .on_press(on_press)
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(20.0))
        .padding(0)
        .style(|theme: &Theme, status: button::Status| {
            let light = is_light_theme(theme);
            let bg = match (light, status) {
                (true, button::Status::Hovered) => Color::from_rgb(0.82, 0.82, 0.88),
                (true, button::Status::Pressed) => Color::from_rgb(0.78, 0.78, 0.84),
                (true, _) => Color::TRANSPARENT,
                (false, button::Status::Hovered) => Color::from_rgb(0.25, 0.25, 0.35),
                (false, button::Status::Pressed) => Color::from_rgb(0.30, 0.30, 0.40),
                (false, _) => Color::TRANSPARENT,
            };
            let text_color = if light {
                Color::from_rgb(0.30, 0.30, 0.40)
            } else {
                Color::from_rgb(0.70, 0.70, 0.75)
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color,
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

/// Corner radius applied to panes (the outer pane container and its title bar).
const PANE_CORNER_RADIUS: f32 = 8.0;

fn title_bar_style(
    theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    // The active (focused) pane's title bar is highlighted with the theme's
    // accent; the inactive title bar sits quietly on the chrome. All colors
    // come from the theme palette so each theme highlights appropriately.
    let (bg, text_color, border_color) = if is_focused {
        (chrome::accent_subtle(theme), chrome::accent_text(theme), chrome::accent(theme))
    } else {
        (chrome::bg_pane_title(theme), chrome::text_muted(theme), chrome::line(theme))
    };

    iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(text_color),
        border: Border {
            color: border_color,
            width: if is_focused { 1.0 } else { 0.0 },
            // Round only the top corners — the title bar caps the top of the pane.
            radius: iced::border::top(PANE_CORNER_RADIUS),
        },
        ..Default::default()
    }
}

fn pane_content_style(
    theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    // Match the app background so the rounded corners blend with the
    // surrounding canvas instead of showing a mismatched square fill.
    let bg = chrome::bg_base(theme);

    // The focused pane gets an accent border; unfocused panes a subtle line.
    let border_color = if is_focused {
        chrome::accent(theme)
    } else {
        chrome::line(theme)
    };

    iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_color,
            width: if is_focused { 2.0 } else { 1.0 },
            radius: PANE_CORNER_RADIUS.into(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Key mapping
// ---------------------------------------------------------------------------

/// Convert an iced keyboard key press into the bytes that should be sent to the PTY.
/// Convert a raw model ID into a friendly display name.
/// e.g. "claude-sonnet-4-20250514" → "Claude Sonnet 4"
///      "gpt-5.4" → "GPT 5.4"
///      "grok-3" → "Grok 3"
///      "gemini-2.0-flash" → "Gemini 2.0 Flash"
///      "llama3.2" → "Llama3.2"
/// Enumerate monospace fonts available on the system using fc-list.
fn enumerate_monospace_fonts() -> Vec<String> {
    // Try fc-list for monospace fonts on Linux
    if let Ok(output) = std::process::Command::new("fc-list")
        .args([":spacing=mono", "family"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut fonts: Vec<String> = stdout
                .lines()
                .map(|line| {
                    // fc-list output: "Family Name" or "Family Name,Alias"
                    line.split(',').next().unwrap_or(line).trim().to_string()
                })
                .filter(|name| !name.is_empty())
                .collect();
            fonts.sort();
            fonts.dedup();
            if !fonts.is_empty() {
                return fonts;
            }
        }
    }

    // Fallback list of common monospace fonts
    vec![
        "monospace".to_string(),
        "Courier New".to_string(),
        "DejaVu Sans Mono".to_string(),
        "Fira Code".to_string(),
        "Hack".to_string(),
        "Inconsolata".to_string(),
        "JetBrains Mono".to_string(),
        "Liberation Mono".to_string(),
        "Noto Sans Mono".to_string(),
        "Source Code Pro".to_string(),
        "Ubuntu Mono".to_string(),
    ]
}

fn friendly_model_name(model_id: &str) -> String {
    if model_id.is_empty() {
        return "AI".to_string();
    }

    // Known model family mappings
    let id = model_id.to_lowercase();

    // Claude models: "claude-opus-4-5-20250414" → "Claude Opus 4.5"
    if id.starts_with("claude-") {
        let without_prefix = &model_id[7..]; // skip "claude-"
        // Strip date suffix (e.g. "-20250514")
        let base = if let Some(pos) = without_prefix.rfind("-20") {
            &without_prefix[..pos]
        } else {
            without_prefix
        };
        // Capitalize parts and join
        let parts: Vec<String> = base.split('-')
            .map(|p| {
                let mut c = p.chars();
                match c.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect();
        return format!("Claude {}", parts.join(" "));
    }

    // GPT models: "gpt-5.4" → "GPT 5.4", "gpt-5.4-mini" → "GPT 5.4 Mini"
    if id.starts_with("gpt-") {
        let version = &model_id[4..];
        let parts: Vec<String> = version.split('-')
            .map(|p| {
                let mut c = p.chars();
                match c.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect();
        return format!("GPT {}", parts.join(" "));
    }

    // Grok models: "grok-3" → "Grok 3"
    if id.starts_with("grok-") {
        let version = &model_id[5..];
        return format!("Grok {}", version);
    }

    // Gemini models: "gemini-2.0-flash" → "Gemini 2.0 Flash"
    if id.starts_with("gemini-") {
        let rest = &model_id[7..];
        let parts: Vec<String> = rest.split('-')
            .map(|p| {
                let mut c = p.chars();
                match c.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect();
        return format!("Gemini {}", parts.join(" "));
    }

    // O-series: "o1" → "O1", "o3-mini" → "O3 Mini"
    if id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") {
        let parts: Vec<String> = model_id.split('-')
            .map(|p| {
                let mut c = p.chars();
                match c.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                    None => String::new(),
                }
            })
            .collect();
        return parts.join(" ");
    }

    // Fallback: capitalize first letter
    let mut c = model_id.chars();
    match c.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
        None => "AI".to_string(),
    }
}

fn key_to_bytes(key: &Key, modified_key: &Key, modifiers: &Modifiers) -> Option<Vec<u8>> {
    match key {
        Key::Character(c) => {
            // Handle Ctrl+<letter> sequences.
            if modifiers.control() {
                if let Some(ch) = c.chars().next() {
                    let lower = ch.to_ascii_lowercase();
                    if lower >= 'a' && lower <= 'z' {
                        // Ctrl+A = 0x01, ..., Ctrl+Z = 0x1A
                        let ctrl_byte = (lower as u8) - b'a' + 1;
                        return Some(vec![ctrl_byte]);
                    }
                }
            }
            match modified_key {
                Key::Character(text) if !text.is_empty() => Some(text.as_bytes().to_vec()),
                Key::Named(named) => named_key_to_bytes(named, modifiers),
                Key::Unidentified => None,
                _ => Some(c.as_bytes().to_vec()),
            }
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

/// Extract the native parent-window handle that wry needs for child webviews.
///
/// On each platform, iced's `window::run()` gives us `&dyn HasWindowHandle`.
/// The `raw_id()` API returns an opaque winit `WindowId` which is only a valid
/// X11 XID on Linux — on macOS it's a `WindowDelegate` pointer, not an NSView.
fn extract_native_window_handle(w: &dyn iced::window::Window) -> u64 {
    use iced::window::raw_window_handle::RawWindowHandle;
    match w.window_handle().map(|h| h.as_raw()) {
        #[cfg(target_os = "linux")]
        Ok(RawWindowHandle::Xlib(h)) => h.window as u64,
        #[cfg(target_os = "linux")]
        Ok(RawWindowHandle::Xcb(h)) => h.window.get() as u64,
        #[cfg(target_os = "macos")]
        Ok(RawWindowHandle::AppKit(h)) => h.ns_view.as_ptr() as u64,
        #[cfg(target_os = "windows")]
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get() as u64,
        Ok(_) => {
            log::warn!("Browser embedding not supported for this window handle type");
            0
        }
        Err(e) => {
            log::warn!("Failed to get native window handle: {e}");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::compose_key;

    #[test]
    fn same_pane_index_distinct_across_tabs() {
        // Pane(0) in tab 0 vs tab 1 must not collide.
        assert_ne!(compose_key(0, 0), compose_key(1, 0));
        // Distinct panes within a tab stay distinct.
        assert_ne!(compose_key(7, 0), compose_key(7, 1));
        // Low bits preserve the pane index.
        assert_eq!(compose_key(3, 5) & 0xFFFF_FFFF, 5);
    }
}
