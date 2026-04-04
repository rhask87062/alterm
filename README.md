# Altermative

A GPU-accelerated, cross-platform terminal workspace built in Rust. Combines the workspace UX of [WaveTerm](https://waveterm.dev) with the raw performance of [WezTerm](https://wezfurlong.org/wezterm/) — without the Electron bloat.

**For beginners, intermediates, and experts.**

## Why the Name

"Altermative" carries four layers of meaning:

1. **Alternative** — a better alternative to slow Electron-based terminals
2. **Alter** — transform how you work with terminals
3. **Term** — it's a terminal, embedded right in the name
4. **Alan** — the creator's middle name (Russell Alan Haskell), a personal signature in the tradition of WezTerm (named after Wez Furlong)

## Features (Planned)

- GPU-accelerated terminal rendering via wgpu
- Tiling workspace with drag-to-split blocks
- Clickable widget sidebar for beginners
- Full keyboard shortcuts for power users
- AI chat integrated into the workspace (OpenAI, Anthropic, Google Gemini, xAI, LM Studio, Ollama)
- Embedded browser panel (via system webview)
- File preview with syntax highlighting
- Settings GUI + TOML config + optional Lua scripting
- Cross-platform: Linux, macOS, Windows

## Building

Requires Rust 1.80+.

```bash
cargo build --release
cargo install --path alterm
```

The binary `alterm` will be available in your PATH.

## Acknowledgments

Altermative builds on the work of several excellent open-source projects:

- **[alacritty_terminal](https://github.com/alacritty/alacritty)** — Terminal emulation backend (VT parsing, screen model, selection, scrollback). Licensed under Apache 2.0. Created by Joe Wilm and the Alacritty contributors.
- **[iced](https://github.com/iced-rs/iced)** — Cross-platform GUI framework. Licensed under MIT.
- **[portable-pty](https://github.com/wezterm/wezterm/tree/main/pty)** — Cross-platform PTY abstraction from the WezTerm project. Licensed under MIT. Created by Wez Furlong.
- **[cosmic-text](https://github.com/pop-os/cosmic-text)** — Text shaping and layout (HarfBuzz). Licensed under MIT/Apache 2.0. Created by the System76/COSMIC team.
- **[glyphon](https://github.com/grovesNL/glyphon)** — GPU text rendering for wgpu. Licensed under MIT/Apache 2.0.
- **[wry](https://github.com/nickel-organic/nickel.rs/tree/main/nickel-wry)** — Webview embedding from the Tauri project. Licensed under MIT/Apache 2.0.
- **[mlua](https://github.com/mlua-rs/mlua)** — Lua 5.4 bindings for Rust. Licensed under MIT.

## Roadmap

### Phase 1: Foundation (current)
Single terminal pane with GPU rendering, PTY integration, ANSI colors, keyboard input, scrollback, resize, copy/paste.

### Phase 2: Workspace
Tab bar, split panes, drag-to-split, widget sidebar, keyboard shortcuts, command palette.

### Phase 3: Intelligence
AI chat panel, multi-provider support (OpenAI, Anthropic, Gemini, Grok, LM Studio, Ollama), terminal context awareness, settings GUI, themes.

### Phase 4: Extras
Embedded browser (wry), file preview, Lua hooks, desktop integration, packaging (deb, rpm, homebrew, winget).

### Phase 5: AI Harness
Native agentic AI coding assistant built into the terminal. Self-improving harness techniques. Pluggable AI provider interface (plugin system).

## License

MIT
