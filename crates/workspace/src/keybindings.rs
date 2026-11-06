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
    NewNote,
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
            Action::NewNote => "New Note",
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
            Action::CloseTab => "Ctrl+Shift+Q",
            Action::NextTab => "Ctrl+Tab",
            Action::PrevTab => "Ctrl+Shift+Tab",
            Action::JumpToTab(_) => "Ctrl+1-9",
            Action::RenameTab => "F2",
            Action::SplitRight => "Ctrl+Shift+D",
            Action::SplitDown => "Ctrl+Shift+E",
            Action::ClosePane => "Ctrl+Shift+W / X",
            Action::MaximizeToggle => "Ctrl+Shift+Alt+F / Z",
            Action::FocusUp => "Ctrl+Shift+Up",
            Action::FocusDown => "Ctrl+Shift+Down",
            Action::FocusLeft => "Ctrl+Shift+Left",
            Action::FocusRight => "Ctrl+Shift+Right",
            Action::CommandPalette => "Ctrl+Shift+Space / P",
            Action::OpenSettings => "Ctrl+Shift+,",
            Action::ToggleAIChat => "Ctrl+Shift+A",
            Action::NewTerminal => "Ctrl+Shift+Return",
            Action::NewBrowser => "Ctrl+Shift+B",
            Action::NewPreview => "Alt+Shift+F / Ctrl+Shift+O",
            Action::NewNote => "Ctrl+Shift+N",
            Action::ShowHotkeyInfo => "Ctrl+Shift+K / H",
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

    // ── Alt+Shift+F / Ctrl+Shift+Alt+F ─────────────────────────────
    // Checked before the Ctrl+Shift block, which ignores Alt-held combos.
    if mods.alt() && mods.shift() {
        if let Key::Character(ref c) = key {
            if c.as_str().eq_ignore_ascii_case("f") {
                return Some(if mods.control() {
                    Action::MaximizeToggle
                } else {
                    Action::NewPreview
                });
            }
        }
    }

    // ── Ctrl+Shift combos ───────────────────────────────────────────
    if mods.control() && mods.shift() && !mods.alt() {
        // Letter shortcuts
        if let Key::Character(ref c) = key {
            let ch = c.as_str().to_ascii_lowercase();
            match ch.as_str() {
                "t" => return Some(Action::NewTab),
                "q" => return Some(Action::CloseTab),
                "w" | "x" => return Some(Action::ClosePane),
                "d" => return Some(Action::SplitRight),
                "e" => return Some(Action::SplitDown),
                "z" => return Some(Action::MaximizeToggle),
                "p" => return Some(Action::CommandPalette),
                "a" => return Some(Action::ToggleAIChat),
                "b" => return Some(Action::NewBrowser),
                "o" => return Some(Action::NewPreview),
                "n" => return Some(Action::NewNote),
                "k" | "h" => return Some(Action::ShowHotkeyInfo),
                "l" => return Some(Action::ToggleTheme),
                "f" => return Some(Action::Search),
                "c" => return Some(Action::Copy),
                "v" => return Some(Action::Paste),
                "," => return Some(Action::OpenSettings),
                _ => {}
            }
        }

        // Arrow shortcuts for focus navigation, plus Return / Space
        if let Key::Named(ref named) = key {
            match named {
                Named::ArrowUp => return Some(Action::FocusUp),
                Named::ArrowDown => return Some(Action::FocusDown),
                Named::ArrowLeft => return Some(Action::FocusLeft),
                Named::ArrowRight => return Some(Action::FocusRight),
                Named::Enter => return Some(Action::NewTerminal),
                Named::Space => return Some(Action::CommandPalette),
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
        Action::NewNote,
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
        assert_eq!(ctrl_shift("n"), Some(Action::NewNote));
        assert_eq!(ctrl_shift("b"), Some(Action::NewBrowser));
        assert_eq!(ctrl_shift("o"), Some(Action::NewPreview));
        assert_eq!(ctrl_shift("k"), Some(Action::ShowHotkeyInfo));
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
        assert_eq!(Action::NewTerminal.shortcut_hint(), "Ctrl+Shift+Return");
        assert_eq!(Action::NewBrowser.shortcut_hint(), "Ctrl+Shift+B");
        assert_eq!(Action::NewPreview.shortcut_hint(), "Alt+Shift+F / Ctrl+Shift+O");
        assert_eq!(Action::ShowHotkeyInfo.shortcut_hint(), "Ctrl+Shift+K / H");
        assert_eq!(Action::ToggleTheme.shortcut_hint(), "Ctrl+Shift+L");
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

    #[test]
    fn omarchy_aligned_named_shortcuts_match() {
        let mods = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            match_shortcut(&Key::Named(Named::Enter), &mods),
            Some(Action::NewTerminal)
        );
        assert_eq!(
            match_shortcut(&Key::Named(Named::Space), &mods),
            Some(Action::CommandPalette)
        );
        // Ctrl+Shift+P stays as an alias for the palette.
        assert_eq!(ctrl_shift("p"), Some(Action::CommandPalette));
        // Plain Ctrl+Return / Ctrl+Space stay with the terminal.
        assert_eq!(match_shortcut(&Key::Named(Named::Enter), &Modifiers::CTRL), None);
        assert_eq!(match_shortcut(&Key::Named(Named::Space), &Modifiers::CTRL), None);
    }

    #[test]
    fn close_and_alt_shortcuts_match() {
        assert_eq!(ctrl_shift("w"), Some(Action::ClosePane));
        assert_eq!(ctrl_shift("x"), Some(Action::ClosePane));
        assert_eq!(ctrl_shift("q"), Some(Action::CloseTab));
        assert_eq!(ctrl_shift("z"), Some(Action::MaximizeToggle));
        assert_eq!(ctrl_shift("o"), Some(Action::NewPreview));
        // Search keeps plain Ctrl+Shift+F.
        assert_eq!(ctrl_shift("f"), Some(Action::Search));

        let f = Key::Character("f".into());
        assert_eq!(
            match_shortcut(&f, &(Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT)),
            Some(Action::MaximizeToggle)
        );
        assert_eq!(
            match_shortcut(&f, &(Modifiers::ALT | Modifiers::SHIFT)),
            Some(Action::NewPreview)
        );
        // Plain Alt+F stays with the shell (forward-word).
        assert_eq!(match_shortcut(&f, &Modifiers::ALT), None);
        // Alt-held variants of other Ctrl+Shift letters don't fire.
        assert_eq!(
            match_shortcut(
                &Key::Character("t".into()),
                &(Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT)
            ),
            None
        );
    }
}
