# Note Pane — Design

- **Date:** 2026-06-22
- **Status:** Approved (design)
- **Scope:** Add a new pane type — a plain-text note pad for quick jotting/recall — alongside the existing Terminal/AIChat/Browser/Preview panes.

## Problem / Goal

The user wants somewhere to "jot things down for quick recall" inside alterm, without leaving for an external editor. A new **Note pane** provides a multi-line, editable, word-wrapped text area whose content persists with the session like every other pane.

## Decisions (locked during brainstorming)

1. **Session-scoped persistence**, identical to other panes: content saved in `session.json`, restored on launch. Closing the note pane discards it (like closing a terminal). No separate notes store/manager.
2. **Plain text** editing via iced's `text_editor` widget. No markdown rendering, no notes browser, no export (all non-goals for v1).
3. The note pane follows the existing **Block / BlockState** model exactly — no new persistence machinery.

## Architecture

A new `Block::Note { state: NoteState }` variant. `NoteState` holds iced's stateful editable buffer (`text_editor::Content`), which must live in the block's state (not be rebuilt each frame). The serializable form is the plain `String` content, captured into a new `BlockState::Note { content }`. The `workspace` crate already depends on `iced`, so `NoteState` can hold the editor buffer.

`Block` is intentionally **not** `Clone` (it holds PTY handles / channels), so storing the non-`Clone` `text_editor::Content` is fine.

## Components & changes

### New: `crates/workspace/src/note.rs`

```rust
use iced::widget::text_editor;

/// State for a single note pane: a live, editable text buffer.
pub struct NoteState {
    pub content: text_editor::Content,
}

impl NoteState {
    /// Empty note.
    pub fn new() -> Self { Self { content: text_editor::Content::new() } }
    /// Note seeded with existing text (used on session restore).
    pub fn with_text(text: &str) -> Self { Self { content: text_editor::Content::with_text(text) } }
    /// Apply an editor action (typing, selection, etc.).
    pub fn perform(&mut self, action: text_editor::Action) { self.content.perform(action); }
    /// The current plain-text content (used for session capture).
    pub fn text(&self) -> String { self.content.text() }
}

impl Default for NoteState {
    fn default() -> Self { Self::new() }
}
```

Register the module in `crates/workspace/src/lib.rs` (`pub mod note;`) and re-export `NoteState` (`pub use note::NoteState;`) next to the existing `AIChatState` re-export.

### `crates/workspace/src/block.rs`

- Add the variant to `enum Block`:
  ```rust
  Note { state: NoteState },
  ```
- Constructors:
  ```rust
  pub fn new_note() -> Self { Block::Note { state: NoteState::new() } }
  pub fn new_note_with(text: &str) -> Self { Block::Note { state: NoteState::with_text(text) } }
  ```
- `from_state` (`block.rs:145`): add `BlockState::Note { content } => Block::new_note_with(content),`
- Capture method (`block.rs:488` area, the `Block -> BlockState` match): add `Block::Note { state } => BlockState::Note { content: state.text() },`
- Add `Block::Note { .. }` to every other `match self` arm that currently groups the non-terminal blocks. Concretely:
  - `write_input`: Note is a no-op (notes don't take PTY bytes) — add `Block::Note { .. }` to the same arm as the other non-terminal no-op blocks.
  - `working_dir`: returns `None` for Note (already the `_ => None` fallback — verify no exhaustive match breaks).
  - Any render/dimension/dirty/`is_*` helpers and the non-terminal groupings (`Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => …`) — add `Block::Note { .. }` to those groupings so the code compiles. Add an `is_note()` helper mirroring `is_ai_chat()` if a call site needs it.

### `crates/workspace/src/session.rs`

- Add to `enum BlockState`:
  ```rust
  Note { content: String },
  ```
  (Plain `String`, serde-serializable like the other variants.)

### `crates/workspace/src/sidebar.rs`

- Add `NewNote` to `enum SidebarAction`.
- Add a sidebar button for it next to the other "new pane" buttons (a notepad glyph, e.g. `"\u{1F4DD}"` 📝, or a short text label `"Note"` — match the style of neighboring buttons).

### `alterm/src/main.rs`

- `Message` enum: add `NoteEdited(pane_grid::Pane, iced::widget::text_editor::Action)`.
- `SidebarAction::NewNote` arm (in the `Message::SidebarAction(action)` match, `main.rs:1393` area):
  ```rust
  SidebarAction::NewNote => {
      let pane = self.add_window(Block::new_note());
      // focus handling consistent with other new panes if applicable
      return /* focus task or Task::none() */;
  }
  ```
- `Message::NoteEdited(pane, action)` handler:
  ```rust
  Message::NoteEdited(pane, action) => {
      let tab = self.active_tab_mut();
      if let Some(Block::Note { state }) = tab.panes.get_mut(pane) {
          state.perform(action);
      }
  }
  ```
- Block→view dispatch (`main.rs:2247` area): add
  ```rust
  Block::Note { state } => note_view(pane, state),
  ```
- New `note_view(pane, state: &NoteState) -> Element<Message>`:
  ```rust
  fn note_view<'a>(pane: pane_grid::Pane, state: &'a workspace::NoteState) -> Element<'a, Message> {
      iced::widget::text_editor(&state.content)
          .on_action(move |a| Message::NoteEdited(pane, a))
          .height(Length::Fill)
          // style consistent with other panes (padding/font)
          .into()
  }
  ```
- Pane title: the note pane's title label is `"Note"` (wherever per-block titles are produced). Renaming is the existing double-click-title mechanism — no new code.

## Data flow

Create: sidebar `NewNote` → `add_window(Block::new_note())`. Edit: keystroke → `text_editor` emits `Action` → `Message::NoteEdited` → `state.perform(action)`. Persist: session capture reads `state.text()` → `BlockState::Note { content }` → `session.json`. Restore: `from_state` → `Block::new_note_with(content)`.

**Trailing-newline caveat (must handle):** iced's `text_editor::Content::text()` returns the buffer **with a trailing `\n` appended**. If captured verbatim, a note's content would gain a newline on every capture→restore cycle. Therefore `NoteState::text()` must strip a single trailing `\n` (e.g. `self.content.text().strip_suffix('\n').map(str::to_string).unwrap_or_else(|| self.content.text())`). This keeps capture idempotent and round-trips clean. The implementer should confirm the exact iced 0.14 behavior and adjust if it does not append `\n`.

## Error handling

None beyond the norm: notes are pure in-memory text. `write_input` is a no-op for Note. Empty notes persist as empty strings. There are no fallible operations.

## Testing

**`workspace` unit tests (in `note.rs`):**
- `with_text("hello world").text() == "hello world"` (seed → read round-trip; the trailing-newline strip makes this exact, not `"hello world\n"`).
- `new().text()` is empty (`""`).
- **Idempotent round-trip:** `with_text("multi\nline").text() == "multi\nline"`, and re-seeding that result and reading again yields the same string (proves no newline accretion across capture→restore cycles).

**Manual verification (controller, isolated config):**
- Click the Note sidebar button → an empty note pane opens with a text cursor.
- Type multi-line text; it wraps and stays editable.
- Split/rearrange panes — the note keeps its content.
- Restart the app (session restore on) → the note pane returns with its text.

## Non-goals (v1)

Markdown preview/rendering, a notes list/browser, file export/import, search within notes, per-note titles beyond the existing pane rename, and any AI integration (the note pane is a future target for the separate AI-app-control feature's `create_note` tool, but that is out of scope here).

## File change summary

| File | Change |
|------|--------|
| `crates/workspace/src/note.rs` | **new** — `NoteState` (editor buffer + text/perform) |
| `crates/workspace/src/lib.rs` | `pub mod note;` + `pub use note::NoteState;` |
| `crates/workspace/src/block.rs` | `Block::Note` variant, constructors, from_state + capture arms, match-arm groupings, optional `is_note()` |
| `crates/workspace/src/session.rs` | `BlockState::Note { content: String }` |
| `crates/workspace/src/sidebar.rs` | `SidebarAction::NewNote` + sidebar button |
| `alterm/src/main.rs` | `NoteEdited` message + handler, `NewNote` arm, `note_view`, view dispatch, title label |
