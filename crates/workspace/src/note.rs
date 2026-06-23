//! Note pane state — a plain-text scratch buffer for quick jotting.
//!
//! Wraps iced's stateful `text_editor::Content` (the live editable buffer,
//! which must live in the block's state, not be rebuilt each frame). The
//! serializable form is the plain `String` returned by `text()`.

use iced::widget::text_editor;

/// State for a single note pane.
pub struct NoteState {
    /// The live, editable text buffer.
    pub content: text_editor::Content,
}

impl NoteState {
    /// An empty note.
    pub fn new() -> Self {
        Self { content: text_editor::Content::new() }
    }

    /// A note seeded with existing text (used on session restore).
    pub fn with_text(text: &str) -> Self {
        Self { content: text_editor::Content::with_text(text) }
    }

    /// Apply an editor action (typing, selection, cursor movement, etc.).
    pub fn perform(&mut self, action: text_editor::Action) {
        self.content.perform(action);
    }

    /// The current plain-text content, used for session capture.
    ///
    /// iced's `Content::text()` appends a trailing newline; strip exactly one
    /// so capture→restore is idempotent (no newline accretion across sessions).
    pub fn text(&self) -> String {
        let t = self.content.text();
        t.strip_suffix('\n').map(str::to_string).unwrap_or(t)
    }
}

impl Default for NoteState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_note_has_empty_text() {
        assert_eq!(NoteState::new().text(), "");
    }

    #[test]
    fn with_text_round_trips_exactly() {
        assert_eq!(NoteState::with_text("hello world").text(), "hello world");
    }

    #[test]
    fn multiline_round_trips_without_newline_accretion() {
        let once = NoteState::with_text("multi\nline").text();
        assert_eq!(once, "multi\nline");
        // Re-seeding the captured text and reading again must be stable.
        let twice = NoteState::with_text(&once).text();
        assert_eq!(twice, "multi\nline");
    }
}
