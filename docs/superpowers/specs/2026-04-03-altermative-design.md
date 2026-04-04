# Altermative — Design Specification

**Date:** 2026-04-03
**Author:** Russell Alan Haskell + Claude
**Status:** Draft

## Overview

Altermative is a GPU-accelerated, cross-platform terminal workspace built in Rust. It combines the workspace UX of WaveTerm (tiling blocks, drag-to-split, widget sidebar, embedded browser, AI chat) with the raw performance of WezTerm (GPU-rendered terminal, sub-millisecond input latency) — without the Electron bloat that makes WaveTerm slow.

**App name:** Altermative
**Binary:** `alterm`
**Tagline:** The alternative terminal — for beginners, intermediates, and experts.

### Why the Name

"Altermative" carries four layers of meaning:
1. **Alternative** — a better alternative to slow Electron-based terminals
2. **Alter** — transform how you work with terminals
3. **Term** — it's a terminal, embedded right in the name
4. **Alan** — the creator's middle name (Russell Alan Haskell), a personal signature in the tradition of WezTerm (named after Wez Furlong)

## Target Users

- **Beginners:** People new to terminals who need clickable GUI controls, visual cues, and a welcoming environment. The widget sidebar, drag-to-split, and settings panel make the terminal approachable.
- **Intermediates:** Developers who know their way around but want a productive workspace — splits, tabs, AI assistance, keyboard shortcuts.
- **Experts:** Power users who want GPU performance, Lua scripting, deep customization, and an AI coding harness.

## Target Platforms

- Linux (X11 and Wayland) — primary development platform
- macOS (Metal)
- Windows (DX12/Vulkan via WebView2)

## Technology Stack

### Core
- **Language:** Rust
- **GUI Framework:** iced (Elm-inspired, built on wgpu)
- **GPU Rendering:** wgpu (Vulkan/Metal/DX12/OpenGL abstraction)
- **Terminal Emulation:** vte (ANSI/VT100 escape sequence parser, from Alacritty project)
- **PTY:** portable-pty (cross-platform PTY abstraction, from WezTerm project)
- **Text Shaping:** cosmic-text (HarfBuzz-based text shaping and layout)

### Workspace Features
- **Browser Embedding:** wry (webview from Tauri project — WebKitGTK on Linux, WebKit on macOS, WebView2 on Windows)
- **Drag and Drop:** Custom implementation within iced widget system

### AI Integration
- **HTTP Client:** reqwest (async HTTP for API calls)
- **Supported Providers:** OpenAI, Anthropic (Claude), Google Gemini, xAI (Grok)
- **Local Models:** Any OpenAI-compatible endpoint — LM Studio, Ollama, vLLM, etc.
- **Streaming:** Server-Sent Events (SSE) for streamed responses

### Configuration
- **Static Config:** TOML (via toml crate)
- **Scripting/Hooks:** Lua 5.4 (via mlua crate)
- **Settings GUI:** Built-in iced-based settings panel that reads/writes the TOML file

### Future (Phase 5)
- **AI Harness:** Native agentic AI coding assistant built into the terminal
- **Self-improving:** Techniques from self-improving harness research
- **Potential Integration:** Evaluate claw-code Rust crates for harness layer (pending license clarification)
- **Plugin System:** Pluggable AI provider interface

## Architecture

### Layer Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    GUI Layer (iced + wgpu)                   │
│  Tab Bar │ Tiling Layout │ Widget Sidebar │ Settings Panel   │
├─────────────────────────────────────────────────────────────┤
│                    Block Types (iced widgets)                │
│  Terminal │ AI Chat │ Browser (wry) │ File Preview           │
├─────────────────────────────────────────────────────────────┤
│                    Core Services                             │
│  Mux │ PTY Manager │ VT Parser │ Config │ AI Service        │
│  Font System │ Keybindings │ Theme Engine                    │
├─────────────────────────────────────────────────────────────┤
│                    Platform Layer                            │
│  Linux (X11/Wayland) │ macOS (Cocoa) │ Windows (Win32)      │
└─────────────────────────────────────────────────────────────┘
```

### GUI Layer

The top-level iced application manages:

- **Tab Bar:** Horizontal tab strip with a "+" button for new tabs, close buttons, drag-to-reorder. Tabs can be renamed (F2).
- **Tiling Layout:** A tree-based layout where each node is either a leaf (single block) or a split (horizontal/vertical) containing child nodes. Inspired by WaveTerm's alternating row/column tree.
- **Widget Sidebar:** Vertical bar on the right with clickable icons for creating new blocks: Terminal, AI Chat, Browser, File Preview, System Info. Customizable via config.
- **Settings Panel:** In-app GUI for common settings (font, theme, keybindings, AI provider). Writes to `config.toml` under the hood.

### Block Types

Each block is an iced custom widget that renders inside the tiling layout:

- **Terminal Block:** GPU-rendered terminal using a custom wgpu shader pipeline. VT100/ANSI escape sequence support via vte. PTY management via portable-pty. Features: scrollback, selection, copy/paste, clickable URLs, search.
- **AI Chat Block:** Chat interface for interacting with AI. Supports markdown rendering, code blocks with syntax highlighting, streaming responses. Context-aware (see AI Context Model below).
- **Browser Block:** Embedded webview via wry. URL bar, back/forward/reload, bookmarks. Can render any website using the system's native web engine.
- **File Preview Block:** Displays files with appropriate rendering — syntax-highlighted code, rendered markdown, inline images. Directory browser with file listing.

### Core Services

All core services are GUI-independent and could theoretically run headless:

- **Mux (Multiplexer):** Singleton that manages the complete state tree: Windows → Tabs → Blocks. Tracks the active block, handles block creation/destruction, notifies the GUI of state changes.
- **PTY Manager:** Spawns shell processes, manages pseudo-terminal I/O, handles resize events. Cross-platform via portable-pty.
- **VT Parser:** Parses ANSI/VT100 escape sequences from PTY output into a screen model (grid of cells with attributes). Uses the vte crate.
- **Config Service:** Loads `config.toml` on startup, watches for changes (hot-reload). Evaluates `hooks.lua` for dynamic behavior. Exposes config values to all other services.
- **AI Service:** Manages API connections to AI providers. Handles authentication (API keys from config or system keychain), request formatting per provider, response streaming, conversation history. Designed with a provider trait so new providers are easy to add.
- **Font System:** Font discovery (platform-native), loading, shaping (HarfBuzz via cosmic-text), and glyph rasterization. Maintains a GPU texture atlas for rendered glyphs.
- **Keybinding Registry:** Maps key combinations to actions. Supports modifiers (Ctrl, Shift, Alt, Super), chord sequences (press leader key, then action key), and context-dependent bindings (different bindings in terminal vs AI chat).
- **Theme Engine:** Manages color schemes, font settings, spacing, and visual properties. Supports built-in themes and custom user themes defined in config.

### Drag-to-Split System

Inspired by WaveTerm's 7-zone drop target model:

When dragging a block header over another block, the target is divided into drop zones:
- **Left/Right edges:** Split vertically, place dragged block to the left or right
- **Top/Bottom edges:** Split horizontally, place above or below
- **Center:** Swap the two blocks

Visual feedback: A colored highlight preview shows where the block will land. The tiling tree is updated to insert a new split node.

Resize handles appear between adjacent blocks. Dragging them adjusts the size ratio.

### AI Context Model

The AI chat block can access terminal output through three mechanisms:

1. **Active Pane (default):** AI automatically sees the output of whichever terminal pane is currently focused. When you switch focus, the AI's context switches too.
2. **Pinned Panes:** Explicitly pin specific panes to the AI context. Pinned panes stay visible to the AI regardless of which pane is focused. Useful for monitoring — "watch my build output AND my server logs while I work in this other terminal."
3. **@ Mentions:** In the AI chat, type `@pane:2` or `@tab:ssh-prod` to reference a specific pane's output in a single message without permanently pinning it.

**AI Actions (with confirmation):**
- Open a new terminal pane and run a command
- Split the current view
- Open a browser pane to show documentation
- All behind a confirmation prompt — the AI proposes, you approve

## Configuration

### File Locations

```
~/.config/altermative/
├── config.toml          # Main configuration
├── hooks.lua            # Optional Lua hooks for dynamic behavior
├── themes/              # Custom theme files
│   └── my-theme.toml
└── bookmarks.toml       # Browser bookmarks
```

### Three Layers

1. **Settings Panel (GUI):** Click to change font, theme, keybindings, AI provider. Beginner-friendly. Writes to config.toml.
2. **config.toml:** Power users edit directly. All static settings — fonts, colors, keybindings, AI keys, terminal behavior.
3. **hooks.lua (optional):** Lua scripting for dynamic behavior. Conditionals, event reactions, computed values. Expert-only, completely optional.

### Key Configuration Areas

- **Appearance:** Font family/size, color scheme, window opacity, tab bar position
- **Terminal:** Scrollback size, cursor style/blink, copy-on-select, bell behavior
- **Keybindings:** Full remapping, leader key support, chord sequences
- **AI:** Provider selection, API keys, default model, context settings
- **Browser:** Default URL, search engine, bookmarks
- **Window:** Default size, startup behavior, tile gap size

## Keyboard Shortcuts (Defaults)

### Window Management
| Key | Action |
|-----|--------|
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl+Shift+N` | New window |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Ctrl+1-9` | Jump to tab N |

### Splits and Blocks
| Key | Action |
|-----|--------|
| `Ctrl+Shift+D` | Split horizontal (new block right) |
| `Ctrl+Shift+E` | Split vertical (new block below) |
| `Ctrl+Shift+Arrow` | Navigate between blocks |
| `Ctrl+Shift+Alt+Arrow` | Resize blocks |
| `Ctrl+Shift+Z` | Toggle block zoom (maximize/restore) |
| `Ctrl+Shift+X` | Close current block |

### Terminal
| Key | Action |
|-----|--------|
| `Ctrl+Shift+C` | Copy |
| `Ctrl+Shift+V` | Paste |
| `Ctrl+Shift+F` | Find in terminal |
| `Ctrl+Shift+K` | Clear scrollback |
| `Shift+PageUp/Down` | Scroll |

### AI
| Key | Action |
|-----|--------|
| `Ctrl+Shift+A` | Toggle AI chat panel |
| `Ctrl+Shift+0` | Focus AI input |

### General
| Key | Action |
|-----|--------|
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+Shift+,` | Open settings |
| `F2` | Rename current tab |

## Build Phases

### Phase 1: Foundation — Get a Terminal on Screen
- iced window with a single terminal pane
- GPU text rendering via wgpu custom widget
- PTY integration (spawn default shell)
- ANSI escape sequence parsing (vte)
- Basic input handling (keyboard, mouse selection)
- ANSI color support (16 + 256 + truecolor)
- Scrollback buffer
- Font loading and rendering (cosmic-text)

### Phase 2: Workspace — Tabs, Splits, Drag-and-Drop
- Tab bar with + button and close buttons
- Split panes (horizontal and vertical)
- Tiling tree layout engine
- Drag-to-split blocks (7-zone drop targets)
- Resize handles between blocks
- Widget sidebar with clickable icons
- Keyboard shortcuts for all workspace operations
- Block zoom (maximize/restore)
- Command palette

### Phase 3: Intelligence — AI Chat + Config
- AI chat block with markdown rendering
- AI provider abstraction (provider trait)
- OpenAI, Anthropic, Google Gemini, xAI (Grok) support
- OpenAI-compatible endpoint support (LM Studio, Ollama, etc.)
- AI context model (active pane, pinned panes, @ mentions)
- AI actions with confirmation (open pane, run command)
- Settings panel GUI
- TOML config file with hot-reload
- Theme engine (built-in + custom themes)
- Keybinding configuration

### Phase 4: Extras — Browser, Preview, Lua, Polish
- Browser block via wry (webview embedding)
- File preview block (code, markdown, images)
- Lua hooks (mlua) for dynamic configuration
- `.desktop` file integration for Linux
- macOS .app bundle
- Windows installer
- Packaging (deb, rpm, homebrew, winget)

### Phase 5: AI Harness — Agentic Mode (Roadmap)
- Native AI coding harness built into the terminal
- Tool-use: file editing, command execution, diff display
- Self-improving harness techniques
- Evaluate claw-code Rust crates for integration (pending license)
- Pluggable AI provider interface (plugin system)

## Installation & Distribution

### Development
```
~/dev-projects/apps/altermative/    # Source code
```

### Install (user-local)
```bash
cargo install --path .
# Binary: ~/.cargo/bin/alterm
```

### Desktop Shortcut (Linux)
```
~/.local/share/applications/altermative.desktop
```

### Future Distribution
- `.deb` package (Ubuntu/Debian)
- `.rpm` package (Fedora/RHEL)
- Homebrew formula (macOS)
- Winget/MSIX (Windows)
- Flatpak (cross-distro Linux)

## Project Structure (Planned)

```
altermative/
├── Cargo.toml               # Workspace root
├── README.md
├── alterm/                   # Main binary crate
│   └── src/main.rs
├── crates/
│   ├── terminal/             # VT parser, screen model, PTY
│   ├── gpu-renderer/         # wgpu text rendering, glyph atlas
│   ├── workspace/            # Mux, tiling tree, tab/block management
│   ├── ai/                   # AI service, provider trait, context
│   ├── config/               # TOML + Lua config loading
│   ├── theme/                # Color schemes, font config
│   └── browser/              # wry webview integration
├── assets/
│   ├── icons/                # App icons (various sizes)
│   └── themes/               # Built-in theme files
├── config/
│   └── default.toml          # Default configuration
├── docs/
│   └── superpowers/specs/    # Design documents
└── packaging/
    ├── linux/                # .desktop file, deb/rpm scripts
    ├── macos/                # .app bundle config
    └── windows/              # Installer config
```

## Success Criteria

- Terminal renders at 60fps with zero perceptible input latency
- Drag-to-split works smoothly with visual feedback
- A complete beginner can open the app and create terminal splits using only the mouse
- An expert can do everything via keyboard shortcuts
- AI chat can read terminal output and provide contextual help
- App binary is under 50MB (vs WaveTerm's ~300MB)
- Cold start under 1 second
