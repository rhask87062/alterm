/// Keybinding registry — maps keyboard shortcuts to workspace actions.
///
/// Centralises all shortcut matching so that main.rs and the command palette
/// can share a single source of truth for bindings.
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// Every action that can be triggered by a keyboard shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    JumpToTab(usize),
    RenameTab,

    SplitRight,
    SplitDown,
    ClosePane,
    MaximizeToggle,

    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,

    CommandPalette,
    OpenSettings,
    ToggleAIChat,
    NewTerminal,
    NewBrowser,
    NewPreview,
    ShowHotkeyInfo,
    ToggleTheme,
    Search,
    Copy,
    Paste,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
}

impl Action {
    /// Human-readable label for the command palette / menus.
    pub fn label(&self) -> &'static str {
        match self {
            Action::NewTab => "New Tab",
            Action::CloseTab => "Close Tab",
            Action::NextTab => "Next Tab",
            Action::PrevTab => "Previous Tab",
            Action::JumpToTab(_) => "Jump to Tab",
            Action::RenameTab => "Rename Tab",
            Action::SplitRight => "Split Right",
            Action::SplitDown => "Split Down",
            Action::ClosePane => "Close Pane",
            Action::MaximizeToggle => "Toggle Maximize",
            Action::FocusUp => "Focus Up",
            Action::FocusDown => "Focus Down",
            Action::FocusLeft => "Focus Left",
            Action::FocusRight => "Focus Right",
            Action::CommandPalette => "Command Palette",
            Action::OpenSettings => "Open Settings",
            Action::ToggleAIChat => "Toggle AI Chat",
            Action::NewTerminal => "New Terminal",
            Action::NewBrowser => "New Browser",
            Action::NewPreview => "New File Preview",
            Action::ShowHotkeyInfo => "Keyboard Shortcuts",
            Action::ToggleTheme => "Toggle Theme",
            Action::Search => "Search",
            Action::Copy => "Copy",
            Action::Paste => "Paste",
            Action::ScrollUp => "Scroll Up",
            Action::ScrollDown => "Scroll Down",
            Action::ScrollPageUp => "Scroll Page Up",
            Action::ScrollPageDown => "Scroll Page Down",
        }
    }

    /// Shortcut hint string for display (e.g. "Ctrl+Shift+T").
    pub fn shortcut_hint(&self) -> &'static str {
        match self {
            Action::NewTab => "Ctrl+Shift+T",
            Action::CloseTab => "Ctrl+Shift+W",
            Action::NextTab => "Ctrl+Tab",
            Action::PrevTab => "Ctrl+Shift+Tab",
            Action::JumpToTab(_) => "Ctrl+1-9",
            Action::RenameTab => "F2",
            Action::SplitRight => "Ctrl+Shift+D",
            Action::SplitDown => "Ctrl+Shift+E",
            Action::ClosePane => "Ctrl+Shift+X",
            Action::MaximizeToggle => "Ctrl+Shift+Z",
            Action::FocusUp => "Ctrl+Shift+Up",
            Action::FocusDown => "Ctrl+Shift+Down",
            Action::FocusLeft => "Ctrl+Shift+Left",
            Action::FocusRight => "Ctrl+Shift+Right",
            Action::CommandPalette => "Ctrl+Shift+P",
            Action::OpenSettings => "Ctrl+Shift+,",
            Action::ToggleAIChat => "Ctrl+Shift+A",
            Action::NewTerminal => "Ctrl+Shift+N",
            Action::NewBrowser => "Ctrl+Shift+B",
            Action::NewPreview => "Ctrl+Shift+O",
            Action::ShowHotkeyInfo => "Ctrl+Shift+H",
            Action::ToggleTheme => "Ctrl+Shift+L",
            Action::Search => "Ctrl+Shift+F",
            Action::Copy => "Ctrl+Shift+C",
            Action::Paste => "Ctrl+Shift+V",
            Action::ScrollUp => "Shift+Up",
            Action::ScrollDown => "Shift+Down",
            Action::ScrollPageUp => "Shift+PageUp",
            Action::ScrollPageDown => "Shift+PageDown",
        }
    }
}

/// Match a key + modifiers combination to a workspace [`Action`].
///
/// Returns `None` when the key combo doesn't match any registered shortcut.
pub fn match_shortcut(key: &Key, mods: &Modifiers) -> Option<Action> {
    // ── F-keys (no modifiers required) ──────────────────────────────
    if let Key::Named(Named::F2) = key {
        return Some(Action::RenameTab);
    }

    // ── Shift+PageUp/Down and Shift+Arrow for scrolling ─────────────
    if mods.shift() && !mods.control() {
        if let Key::Named(ref named) = key {
            match named {
                Named::PageUp => return Some(Action::ScrollPageUp),
                Named::PageDown => return Some(Action::ScrollPageDown),
                Named::ArrowUp => return Some(Action::ScrollUp),
                Named::ArrowDown => return Some(Action::ScrollDown),
                _ => {}
            }
        }
    }

    // ── Ctrl+Tab / Ctrl+Shift+Tab ───────────────────────────────────
    if mods.control() {
        if let Key::Named(Named::Tab) = key {
            return if mods.shift() {
                Some(Action::PrevTab)
            } else {
                Some(Action::NextTab)
            };
        }
    }

    // ── Ctrl+<digit> — jump to tab ──────────────────────────────────
    if mods.control() && !mods.shift() {
        if let Key::Character(ref c) = key {
            let s = c.as_str();
            if let Ok(n) = s.parse::<usize>() {
                if (1..=9).contains(&n) {
                    return Some(Action::JumpToTab(n));
                }
            }
        }
    }

    // ── Ctrl+Shift combos ───────────────────────────────────────────
    if mods.control() && mods.shift() {
        // Letter shortcuts
        if let Key::Character(ref c) = key {
            let ch = c.as_str().to_ascii_lowercase();
            match ch.as_str() {
                "t" => return Some(Action::NewTab),
                "w" => return Some(Action::CloseTab),
                "d" => return Some(Action::SplitRight),
                "e" => return Some(Action::SplitDown),
                "x" => return Some(Action::ClosePane),
                "z" => return Some(Action::MaximizeToggle),
                "p" => return Some(Action::CommandPalette),
                "a" => return Some(Action::ToggleAIChat),
                "n" => return Some(Action::NewTerminal),
                "b" => return Some(Action::NewBrowser),
                "o" => return Some(Action::NewPreview),
                "h" => return Some(Action::ShowHotkeyInfo),
                "l" => return Some(Action::ToggleTheme),
                "f" => return Some(Action::Search),
                "c" => return Some(Action::Copy),
                "v" => return Some(Action::Paste),
                "," => return Some(Action::OpenSettings),
                _ => {}
            }
        }

        // Arrow shortcuts for focus navigation
        if let Key::Named(ref named) = key {
            match named {
                Named::ArrowUp => return Some(Action::FocusUp),
                Named::ArrowDown => return Some(Action::FocusDown),
                Named::ArrowLeft => return Some(Action::FocusLeft),
                Named::ArrowRight => return Some(Action::FocusRight),
                _ => {}
            }
        }
    }

    None
}

/// Return a list of all "palette-worthy" actions with their labels and hints.
///
/// Excludes per-digit JumpToTab variants — only includes one representative.
pub fn all_palette_actions() -> Vec<Action> {
    vec![
        Action::NewTab,
        Action::CloseTab,
        Action::NextTab,
        Action::PrevTab,
        Action::RenameTab,
        Action::SplitRight,
        Action::SplitDown,
        Action::ClosePane,
        Action::MaximizeToggle,
        Action::FocusUp,
        Action::FocusDown,
        Action::FocusLeft,
        Action::FocusRight,
        Action::CommandPalette,
        Action::OpenSettings,
        Action::ToggleAIChat,
        Action::NewTerminal,
        Action::NewBrowser,
        Action::NewPreview,
        Action::ShowHotkeyInfo,
        Action::ToggleTheme,
        Action::Search,
        Action::Copy,
        Action::Paste,
        Action::ScrollPageUp,
        Action::ScrollPageDown,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_shift(ch: &str) -> Option<Action> {
        let key = Key::Character(ch.into());
        let mods = Modifiers::CTRL | Modifiers::SHIFT;
        match_shortcut(&key, &mods)
    }

    #[test]
    fn new_window_shortcuts_match() {
        assert_eq!(ctrl_shift("n"), Some(Action::NewTerminal));
        assert_eq!(ctrl_shift("b"), Some(Action::NewBrowser));
        assert_eq!(ctrl_shift("o"), Some(Action::NewPreview));
        assert_eq!(ctrl_shift("h"), Some(Action::ShowHotkeyInfo));
        assert_eq!(ctrl_shift("l"), Some(Action::ToggleTheme));
        assert_eq!(ctrl_shift("f"), Some(Action::Search));
    }

    #[test]
    fn new_actions_have_hints_and_labels() {
        for action in [
            Action::NewTerminal,
            Action::NewBrowser,
            Action::NewPreview,
            Action::ShowHotkeyInfo,
            Action::ToggleTheme,
            Action::Search,
        ] {
            assert!(!action.shortcut_hint().is_empty());
            assert!(!action.label().is_empty());
        }
        assert_eq!(Action::NewPreview.shortcut_hint(), "Ctrl+Shift+O");
        assert_eq!(Action::Search.shortcut_hint(), "Ctrl+Shift+F");
    }

    #[test]
    fn new_actions_are_in_palette() {
        let actions = all_palette_actions();
        for a in [
            Action::NewTerminal,
            Action::NewBrowser,
            Action::NewPreview,
            Action::ShowHotkeyInfo,
            Action::ToggleTheme,
            Action::Search,
        ] {
            assert!(actions.contains(&a), "missing {a:?} in palette actions");
        }
    }
}
