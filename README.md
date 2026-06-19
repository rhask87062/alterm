# Alterm

A GPU-accelerated, cross-platform terminal **workspace** built in Rust. It combines the workspace UX of [WaveTerm](https://waveterm.dev) with the raw performance of [WezTerm](https://wezfurlong.org/wezterm/) — without the Electron bloat.

**For beginners, intermediates, and experts.**

---

## Features

Alterm is more than a terminal — it's a tiling workspace where every pane can be a
terminal, an AI chat, a browser, a file preview, or a settings panel.

### Available now

- **Tiling workspace** — split panes right/down, focus navigation between panes, and maximize-toggle, all powered by `iced`'s pane grid.
- **Tabs** — open, close, cycle, jump (`Ctrl+1–9`), and rename. Double-click a tab or pane title to rename it inline (or press `F2`).
- **GPU-accelerated terminal** — VT/ANSI emulation via `alacritty_terminal`, cross-platform PTY via `portable-pty`, scrollback, resize, and copy/paste.
- **Integrated AI chat** — chat panes that can see your recent terminal output for context. Multiple providers: OpenAI, Anthropic, Google Gemini, xAI (Grok), LM Studio, and Ollama (local, default).
- **Embedded browser** — open web pages in a pane via the system webview (`wry`).
- **File preview** — preview panes for code (syntax highlighting), images, and PowerPoint files.
- **Command palette** — `Ctrl+Shift+P` to fuzzy-find and run any action.
- **Settings GUI + TOML config** — edit appearance, AI providers, and terminal settings in-app, or hand-edit `~/.config/alterm/config.toml`.
- **Themes** — dark/light themes with theme-aware chrome and a theme-adaptive keyboard-shortcut help panel. Toggle with `Ctrl+Shift+L`.
- **Session persistence** — your tabs, panes, and window are restored on next launch.
- **Clickable widget sidebar** — point-and-click access to actions for users who prefer the mouse.
- **Branding** — custom ASCII-art startup logo with a gradient paint.

### Experimental

- **Lua hooks** — Alterm loads `~/.config/alterm/hooks.lua` at startup (Lua 5.4 via `mlua`). The scripting host is wired in, but hook trigger points are still being added.

### Planned

See the [Roadmap](#roadmap) — next up is a native agentic AI coding harness and a pluggable provider plugin system.

---

## Quickstart

Requires **Rust 1.80+**.

```bash
git clone https://github.com/rhask87062/alterm.git
cd alterm
cargo run --release
```

To install the `alterm` binary into your PATH:

```bash
cargo install --path alterm
```

First steps once it's open:

- `Ctrl+Shift+N` — new terminal pane
- `Ctrl+Shift+D` / `Ctrl+Shift+E` — split right / split down
- `Ctrl+Shift+P` — command palette (discover everything)
- `Ctrl+Shift+H` — keyboard-shortcut help panel
- `Ctrl+Shift+,` — open settings

---

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| New tab | `Ctrl+Shift+T` |
| Close tab | `Ctrl+Shift+W` |
| Next / previous tab | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Jump to tab 1–9 | `Ctrl+1`…`Ctrl+9` |
| Rename tab | `F2` |
| Split right / down | `Ctrl+Shift+D` / `Ctrl+Shift+E` |
| Close pane | `Ctrl+Shift+X` |
| Toggle maximize pane | `Ctrl+Shift+Z` |
| Focus pane (up/down/left/right) | `Ctrl+Shift+Arrow` |
| Command palette | `Ctrl+Shift+P` |
| New terminal | `Ctrl+Shift+N` |
| Toggle AI chat | `Ctrl+Shift+A` |
| New browser | `Ctrl+Shift+B` |
| New file preview | `Ctrl+Shift+O` |
| Keyboard shortcuts panel | `Ctrl+Shift+H` |
| Open settings | `Ctrl+Shift+,` |
| Toggle theme | `Ctrl+Shift+L` |
| Search | `Ctrl+Shift+F` |
| Copy / paste | `Ctrl+Shift+C` / `Ctrl+Shift+V` |
| Scroll line (up/down) | `Shift+Up` / `Shift+Down` |
| Scroll page (up/down) | `Shift+PageUp` / `Shift+PageDown` |

The shortcut registry lives in [`crates/workspace/src/keybindings.rs`](crates/workspace/src/keybindings.rs) and is the single source of truth — the in-app help panel and command palette are generated from it.

---

## Configuration

Alterm reads `~/.config/alterm/config.toml`. The full set of defaults is documented in [`config/default.toml`](config/default.toml). Highlights:

```toml
[general]
# default_shell = "/bin/zsh"   # unset → use $SHELL

[ai]
default_provider = "ollama"
max_tokens       = 4096
temperature      = 0.7
system_prompt    = "You are a helpful terminal assistant. You can see the user's terminal output for context."

[appearance]
font_size   = 14.0
font_family = "monospace"   # or e.g. "JetBrains Mono"
theme       = "dark"        # "dark" | "light"

[terminal]
scrollback_lines = 10000
cursor_blink     = true
copy_on_select   = false

[session]
restore = true               # restore tabs/panes/window on launch
```

### AI providers

Each provider is configured under `[ai.providers.<name>]`. Local providers (Ollama, LM Studio) need no API key; cloud providers do.

```toml
# Local Ollama (default, no key needed)
[ai.providers.ollama]
model    = "llama3.2"
base_url = "http://localhost:11434/v1"

# OpenAI
# [ai.providers.openai]
# api_key  = "sk-..."
# model    = "gpt-4o"

# Anthropic
# [ai.providers.anthropic]
# api_key  = "sk-ant-..."
# model    = "claude-opus-4-5"
```

Set `[ai].default_provider` to whichever provider key you want chat panes to use by default. See [`config/default.toml`](config/default.toml) for Gemini, xAI, and LM Studio examples.

---

## Building

```bash
cargo build --release
cargo install --path alterm
```

The binary `alterm` will be available in your PATH.

## Packaging

Installer and bundle scripts live under [`packaging/`](packaging).

**Linux:**
```bash
./packaging/build-release.sh linux
```
Produces `dist/alterm_<version>_<arch>.deb` and `dist/alterm-<version>-linux-<arch>.tar.gz`.

**macOS:**
```bash
./packaging/build-release.sh macos
```
Produces `dist/Alterm.app` and `dist/alterm-<version>-macos-<arch>.pkg`. Requires Apple `pkgbuild`.

**Windows:**
```powershell
pwsh -File .\packaging\windows\build-installer.ps1
```
Produces `dist\alterm-<version>-windows-x64-setup.exe`. Requires Inno Setup (`ISCC.exe`) on `PATH`.

---

## Architecture

Alterm is a Cargo workspace. The application shell is `iced`-based; the heavy lifting lives in focused crates.

| Crate | Responsibility |
| --- | --- |
| [`alterm/`](alterm/src/main.rs) | Application entry point — the `iced` app, message loop, pane/tab/webview wiring, and all panel views. |
| [`crates/terminal/`](crates/terminal) | PTY management and the `alacritty_terminal` screen model. |
| [`crates/workspace/`](crates/workspace) | Workspace model: blocks (pane contents), tabs, grid, keybindings, command palette, sidebar, session persistence. |
| [`crates/ai/`](crates/ai) | Multi-provider AI chat (OpenAI-compatible, Anthropic, Gemini). |
| [`crates/browser/`](crates/browser) | `wry` webview manager for embedded browser panes. |
| [`crates/preview/`](crates/preview) | File preview (code, image, pptx). |
| [`crates/config/`](crates/config) | TOML config, themes, and Lua hooks. |
| [`crates/gpu-renderer/`](crates/gpu-renderer) | GPU rendering helpers (grid, colors, widget). |
| [`website/`](website) | Astro marketing/docs site. |

A pane's content is a `Block` — one of terminal, AI chat, settings, browser, preview, or the hotkey-info reference. Tabs each own a pane grid.

---

## Roadmap

- **Phase 1 — Foundation** ✅ — GPU terminal rendering, PTY integration, ANSI colors, keyboard input, scrollback, resize, copy/paste.
- **Phase 2 — Workspace** ✅ — tab bar, split panes, widget sidebar, keyboard shortcuts, command palette.
- **Phase 3 — Intelligence** ✅ — AI chat panel, multi-provider support, terminal context awareness, settings GUI, themes.
- **Phase 4 — Extras** ✅ (mostly) — embedded browser, file preview with syntax highlighting, desktop integration, packaging, custom ASCII branding, hotkey info pane. Lua hooks are in progress; `rpm`/Homebrew/winget packaging still pending.
- **Phase 5 — AI Harness** 🚧 — a native agentic AI coding assistant built into the terminal, self-improving harness techniques, and a pluggable AI-provider plugin system.

---

## Acknowledgments

Alterm builds on the work of several excellent open-source projects:

- **[alacritty_terminal](https://github.com/alacritty/alacritty)** — Terminal emulation backend (VT parsing, screen model, selection, scrollback). Apache 2.0. Created by Joe Wilm and the Alacritty contributors.
- **[iced](https://github.com/iced-rs/iced)** — Cross-platform GUI framework. MIT.
- **[portable-pty](https://github.com/wezterm/wezterm/tree/main/pty)** — Cross-platform PTY abstraction from the WezTerm project. MIT. Created by Wez Furlong.
- **[cosmic-text](https://github.com/pop-os/cosmic-text)** — Text shaping and layout (HarfBuzz). MIT/Apache 2.0. Created by the System76/COSMIC team.
- **[glyphon](https://github.com/grovesNL/glyphon)** — GPU text rendering for wgpu. MIT/Apache 2.0.
- **[wry](https://github.com/tauri-apps/wry)** — Webview embedding from the Tauri project. MIT/Apache 2.0.
- **[mlua](https://github.com/mlua-rs/mlua)** — Lua 5.4 bindings for Rust. MIT.

---

## License

MIT.
