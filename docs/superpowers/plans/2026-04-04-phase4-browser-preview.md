# Phase 4a: Browser Embed + File Preview

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a browser block (embedded webview via wry) and a file preview block (syntax-highlighted code, markdown, images) to the workspace — completing the core block types from the design spec.

**Architecture:** The `wry` crate (from the Tauri project) provides a cross-platform webview — WebKitGTK on Linux, WebKit on macOS, WebView2 on Windows. We embed a wry webview as a block in the pane_grid. The file preview block uses iced's built-in text/scrollable widgets for code and markdown, with the canvas for images.

**Tech Stack:** wry 0.50+ (webview), syntect (syntax highlighting), iced widgets (scrollable, text), existing workspace/block crates

---

## File Structure

```
altermative/
├── crates/
│   ├── browser/                    # NEW: webview embedding
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs              # BrowserState, webview lifecycle
│   ├── preview/                    # NEW: file preview
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # PreviewState, file type detection
│   │       ├── code.rs             # Syntax-highlighted code rendering
│   │       └── image.rs            # Image rendering
│   ├── workspace/src/
│   │   └── block.rs                # Add Browser + Preview variants
│   └── ...existing crates...
```

---

### Task 1: Browser Crate Scaffold + wry Integration

**Files:**
- Create: `crates/browser/Cargo.toml`
- Create: `crates/browser/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Create browser crate**

Cargo.toml with `wry` dependency. Add to workspace members.

- [ ] **Step 2: Define BrowserState**

```rust
pub struct BrowserState {
    pub url: String,           // Current URL
    pub input_url: String,     // URL bar text (may differ from current while typing)
    pub loading: bool,
    pub title: String,         // Page title from webview
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub history: Vec<String>,  // URL history for back/forward
    pub history_index: usize,
}
```

Methods: `new(url)`, `navigate(url)`, `go_back()`, `go_forward()`, `reload()`

- [ ] **Step 3: Investigate wry embedding in iced**

wry creates a native webview window. Embedding it INSIDE an iced pane requires one of:
- **Option A:** Create the webview as a child window positioned over the pane area (overlay approach)
- **Option B:** Use wry's `WebViewBuilder::with_web_context` and render to a texture (complex)
- **Option C:** Use a separate window that tracks the pane position

Option A (child window overlay) is the most practical approach — this is how most apps embed webviews. The webview window is positioned to match the pane's screen coordinates and resized when the pane resizes.

- [ ] **Step 4: Implement webview lifecycle**

- On Browser block creation: spawn a wry webview as a child/overlay window
- Track the pane's screen position and size
- Reposition the webview when the pane moves/resizes
- Hide the webview when the tab changes or pane is hidden
- Destroy the webview when the block is closed

- [ ] **Step 5: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: browser crate with wry webview integration"
```

---

### Task 2: Browser Block in Workspace

**Files:**
- Modify: `crates/workspace/src/block.rs`
- Modify: `crates/workspace/src/lib.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Add Browser variant to Block**

```rust
pub enum Block {
    Terminal { ... },
    AIChat { ... },
    Settings { ... },
    Browser { state: BrowserState },
}
```

- [ ] **Step 2: Browser view in main.rs**

The browser block view shows:
- URL bar with back/forward/reload buttons
- The actual webview is an overlay window (not an iced widget)
- A placeholder/border area where the webview will be positioned

```
┌──────────────────────────────────────┐
│ ◀ ▶ ↻  [https://example.com    ] 🔖 │  ← nav bar
├──────────────────────────────────────┤
│                                      │
│         (webview overlay)            │
│                                      │
└──────────────────────────────────────┘
```

- [ ] **Step 3: Wire sidebar browser button**

Activate the "W" sidebar button → creates a new Browser block

- [ ] **Step 4: Add keyboard shortcuts**

- `Ctrl+L` when browser focused → focus URL bar
- `Ctrl+R` → reload
- `Alt+Left/Right` → back/forward

- [ ] **Step 5: Verify and commit**

```bash
git add -A && git commit -m "feat: browser block with URL bar, navigation, webview overlay"
```

---

### Task 3: File Preview Crate

**Files:**
- Create: `crates/preview/Cargo.toml`
- Create: `crates/preview/src/lib.rs`
- Create: `crates/preview/src/code.rs`
- Create: `crates/preview/src/image.rs`

- [ ] **Step 1: Create preview crate**

Dependencies: `syntect` (syntax highlighting), `iced` (widgets), image detection.

- [ ] **Step 2: Define PreviewState**

```rust
pub struct PreviewState {
    pub path: PathBuf,
    pub file_type: FileType,
    pub content: PreviewContent,
}

pub enum FileType {
    Code { language: String },
    Markdown,
    Image,
    Text,
    Binary,
    Directory,
}

pub enum PreviewContent {
    Text(String),
    HighlightedCode(Vec<HighlightedLine>),
    Image { width: u32, height: u32, data: Vec<u8> },
    Directory(Vec<DirEntry>),
    Unsupported,
}
```

- [ ] **Step 3: Implement syntax highlighting**

```rust
// crates/preview/src/code.rs
use syntect::highlighting::{ThemeSet, Theme};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

pub struct HighlightedLine {
    pub spans: Vec<HighlightedSpan>,
}

pub struct HighlightedSpan {
    pub text: String,
    pub fg: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

pub fn highlight_code(source: &str, language: &str) -> Vec<HighlightedLine> {
    // Use syntect to tokenize and highlight
}
```

- [ ] **Step 4: Implement file type detection**

Detect file type from extension and content:
- `.rs`, `.py`, `.js`, `.ts`, `.go`, `.c`, `.cpp`, `.java`, etc. → Code
- `.md` → Markdown
- `.png`, `.jpg`, `.gif`, `.svg`, `.webp` → Image
- `.txt`, `.log`, `.csv` → Text
- Directories → Directory listing
- Everything else → Binary/Unsupported

- [ ] **Step 5: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: preview crate with syntax highlighting and file type detection"
```

---

### Task 4: File Preview Block in Workspace

**Files:**
- Modify: `crates/workspace/src/block.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Add Preview variant to Block**

```rust
pub enum Block {
    Terminal { ... },
    AIChat { ... },
    Settings { ... },
    Browser { ... },
    Preview { state: PreviewState },
}
```

- [ ] **Step 2: Preview view in main.rs**

The preview block view shows:
- File path header with breadcrumb
- Content area depends on file type:
  - **Code:** scrollable with syntax-highlighted lines (colored spans)
  - **Markdown:** rendered text (basic — headers, bold, code blocks)
  - **Image:** centered image display
  - **Directory:** file listing with icons, click to open

```
┌──────────────────────────────────────┐
│ 📁 ~/projects/altermative/src/main.rs │  ← path bar
├──────────────────────────────────────┤
│ 1  use iced::{application, ...};     │
│ 2                                    │  ← syntax highlighted
│ 3  fn main() -> iced::Result {       │
│ 4      env_logger::init();           │
│ ...                                  │
└──────────────────────────────────────┘
```

- [ ] **Step 3: File opening dialog**

Add a way to open files:
- Sidebar file button → opens a file picker or path input
- Terminal integration: `preview <filepath>` could eventually open a preview pane

- [ ] **Step 4: Verify and commit**

```bash
git add -A && git commit -m "feat: file preview block with syntax highlighting, directory listing"
```

---

### Task 5: Polish and Integration

- [ ] **Step 1: Sidebar buttons wired**
  - "W" button → Browser block (opens to a default URL or blank page)
  - File button → Preview block (opens to home directory)

- [ ] **Step 2: Cross-block interactions**
  - Terminal `cd` doesn't auto-update preview (future feature)
  - Browser URLs can be bookmarked (future feature)

- [ ] **Step 3: Final verification**

Run and verify:
1. Browser block opens with URL bar and working webview
2. Can navigate to websites, go back/forward, reload
3. File preview opens and shows syntax-highlighted code
4. Directory listing works, can click files to preview them
5. Image files display correctly
6. All block types coexist in the same tab

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: Phase 4a complete — browser embed and file preview"
```

---

## Phase 4a Completion Checklist

- [ ] Browser block with embedded wry webview
- [ ] URL bar with back/forward/reload
- [ ] Webview positioned as overlay matching pane bounds
- [ ] File preview block with syntax highlighting (syntect)
- [ ] Code file preview with line numbers and colors
- [ ] Directory listing with file type icons
- [ ] Image display
- [ ] Sidebar W button creates browser, file button creates preview
- [ ] All 5 block types work in the same tab

## Notes

- Lua hooks, `.desktop` integration, and packaging are deferred to Phase 4b
- The wry webview overlay approach means the browser may not perfectly clip to pane boundaries during rapid resizing — this is a known limitation of the overlay approach
- Syntax highlighting themes should match the terminal theme (dark/light)
