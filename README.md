# Alterm

A GPU-accelerated, cross-platform terminal workspace built in Rust. Combines the workspace UX of [WaveTerm](https://waveterm.dev) with the raw performance of [WezTerm](https://wezfurlong.org/wezterm/) — without the Electron bloat.

**For beginners, intermediates, and experts.**

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

## Packaging

Installer and bundle scripts live under [`packaging/`](/home/russell/dev-projects/apps/alterm/packaging).

Linux:
```bash
./packaging/build-release.sh linux
```

This produces:
- `dist/alterm_<version>_<arch>.deb`
- `dist/alterm-<version>-linux-<arch>.tar.gz`

macOS:
```bash
./packaging/build-release.sh macos
```

This produces:
- `dist/Alterm.app`
- `dist/alterm-<version>-macos-<arch>.pkg`

Windows:
```powershell
pwsh -File .\packaging\windows\build-installer.ps1
```

This produces:
- `dist\alterm-<version>-windows-x64-setup.exe`

Windows packaging expects Inno Setup (`ISCC.exe`) to be installed and available in `PATH`. macOS packaging expects Apple `pkgbuild`.

## Acknowledgments

Alterm builds on the work of several excellent open-source projects:

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
Embedded browser (wry), file preview with syntax highlighting, Lua hooks, desktop integration, packaging (deb, rpm, homebrew, winget), custom ASCII art branding, hotkey breakdown info pane (accessible from sidebar).

### Phase 5: AI Harness
Native agentic AI coding assistant built into the terminal. Self-improving harness techniques. Pluggable AI provider interface (plugin system).

## License

MIT
