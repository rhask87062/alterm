/// Command palette — a fuzzy-searchable overlay listing all workspace actions.
///
/// Activated with Ctrl+Shift+P.  Provides a text input for filtering commands
/// and a list of matches the user can navigate with Up/Down and execute with
/// Enter.
use crate::keybindings::{all_palette_actions, Action};

/// A single entry in the command palette.
#[derive(Debug, Clone)]
pub struct Command {
    pub label: String,
    pub shortcut: String,
    pub action: Action,
}

/// The palette state.
pub struct CommandPalette {
    pub visible: bool,
    pub query: String,
    commands: Vec<Command>,
    pub filtered: Vec<usize>,
    pub selected: usize,
}

impl CommandPalette {
    /// Create a new palette pre-populated with every registered action.
    pub fn new() -> Self {
        let commands: Vec<Command> = all_palette_actions()
            .into_iter()
            .map(|a| Command {
                label: a.label().to_string(),
                shortcut: a.shortcut_hint().to_string(),
                action: a,
            })
            .collect();

        let filtered: Vec<usize> = (0..commands.len()).collect();

        CommandPalette {
            visible: false,
            query: String::new(),
            commands,
            filtered,
            selected: 0,
        }
    }

    /// Toggle the palette open/closed.  Clears the query when opening.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.query.clear();
            self.refilter();
        }
    }

    /// Open the palette (idempotent).
    pub fn open(&mut self) {
        if !self.visible {
            self.toggle();
        }
    }

    /// Close the palette.
    pub fn close(&mut self) {
        self.visible = false;
    }

    /// Update the search query and re-filter.
    pub fn update_query(&mut self, new_query: String) {
        self.query = new_query;
        self.refilter();
    }

    /// Move selection to the next item.
    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Move selection to the previous item.
    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            if self.selected == 0 {
                self.selected = self.filtered.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    /// Execute the currently selected command and close the palette.
    ///
    /// Returns `None` if nothing is selected.
    pub fn execute(&mut self) -> Option<Action> {
        if self.filtered.is_empty() {
            self.close();
            return None;
        }
        let idx = self.filtered[self.selected];
        let action = self.commands[idx].action.clone();
        self.close();
        Some(action)
    }

    /// Return a slice of commands matching the current filter.
    pub fn visible_commands(&self) -> Vec<&Command> {
        self.filtered.iter().map(|&i| &self.commands[i]).collect()
    }

    /// Re-compute the filtered indices from the current query.
    fn refilter(&mut self) {
        let query_lower = self.query.to_ascii_lowercase();

        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if query_lower.is_empty() {
                    return true;
                }
                let label_lower = cmd.label.to_ascii_lowercase();
                label_lower.contains(&query_lower)
            })
            .map(|(i, _)| i)
            .collect();

        // Keep selected in bounds.
        if self.selected >= self.filtered.len() {
            self.selected = if self.filtered.is_empty() {
                0
            } else {
                self.filtered.len() - 1
            };
        }
    }
}
