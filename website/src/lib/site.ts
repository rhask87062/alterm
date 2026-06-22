// Central content + config for the Alterm site.
// Edit data here; components stay presentation-only.

export const SITE = {
  name: "Alterm",
  tagline: "The alternative terminal — for beginners, intermediates, and experts.",
  description:
    "A GPU-accelerated, cross-platform terminal workspace built in Rust. The workspace UX of WaveTerm with the raw performance of WezTerm — without the Electron bloat.",
  repo: "https://github.com/rhask87062/alterm",
  version: "0.4.0",
  license: "MIT",
};

export const NAV = [
  { label: "Features", href: "/#features" },
  { label: "AI", href: "/#ai" },
  { label: "Roadmap", href: "/#roadmap" },
  { label: "Docs", href: "/docs" },
];

export const STATS = [
  { value: "< 50MB", label: "binary size", note: "vs ~300MB Electron" },
  { value: "< 1s", label: "cold start" },
  { value: "60fps", label: "GPU rendering" },
  { value: "6", label: "AI providers" },
  { value: "3", label: "platforms" },
];

// icon = filename in /icons/sidebar (masked + recolored), or null for emoji-free glyph
export const FEATURES = [
  {
    icon: "terminal",
    title: "GPU-accelerated rendering",
    body: "Terminal cells drawn on the GPU via wgpu (Vulkan / Metal / DX12). Truecolor, 10k-line scrollback, zero perceptible input latency.",
    status: "core",
  },
  {
    icon: "folder",
    title: "Tiling workspace",
    body: "Split panes right or down, drag to rearrange, resize handles, and block zoom — with tabs on top. A real tiling workspace, not just tabs.",
    status: "core",
  },
  {
    icon: "settings",
    title: "Clickable widget sidebar",
    body: "Spawn terminals, AI chat, browser, or file preview with a click. The terminal, made approachable for newcomers.",
    status: "core",
  },
  {
    icon: null,
    title: "Full keyboard control",
    body: "Every workspace action has a keyboard shortcut, surfaced in a searchable command palette — splits, tabs, navigation, and terminal control without leaving the keyboard.",
    status: "core",
  },
  {
    icon: "info",
    title: "Integrated AI chat",
    body: "Six providers, terminal-aware context, and AI actions behind confirmation. Help that can actually see your output.",
    status: "core",
  },
  {
    icon: "browser",
    title: "Embedded browser",
    body: "A real webview pane — docs and dashboards alongside your shell, using the system's native engine, with back/forward history and drag-aware placement.",
    status: "core",
  },
  {
    icon: "folder",
    title: "File preview",
    body: "Syntax-highlighted code, inline images, and even rendered PPTX slides in a dedicated block. Read without leaving the workspace.",
    status: "core",
  },
  {
    icon: "settings",
    title: "Settings GUI + TOML + Lua",
    body: "Three layers from beginner to expert: a settings panel, a documented TOML file, and optional Lua hooks for dynamic behavior.",
    status: "core",
  },
  {
    icon: null,
    title: "Cross-platform",
    body: "Linux (X11 & Wayland), macOS, and Windows from one Rust codebase. Native everywhere, Electron nowhere.",
    status: "core",
  },
];

export const AUDIENCES = [
  {
    key: "Beginners",
    body: "Clickable GUI controls, visual cues, and a welcoming workspace. The sidebar, drag-to-split, and settings panel make the terminal approachable.",
  },
  {
    key: "Intermediates",
    body: "A productive workspace — splits, tabs, keyboard shortcuts, and AI assistance that reads your terminal and helps you move faster.",
  },
  {
    key: "Experts",
    body: "GPU performance, Lua scripting, deep customization, and a roadmap toward a native agentic AI coding harness built into the terminal.",
  },
];

export const PROVIDERS = [
  { name: "OpenAI", kind: "cloud" },
  { name: "Anthropic", kind: "cloud" },
  { name: "Google Gemini", kind: "cloud" },
  { name: "xAI Grok", kind: "cloud" },
  { name: "Ollama", kind: "local" },
  { name: "LM Studio", kind: "local" },
];

export const AI_CONTEXT = [
  {
    title: "Active-pane context",
    body: "The chat reads the focused terminal's recent output, so answers are grounded in what's actually on your screen — not a guess.",
  },
  {
    title: "Streaming, multi-provider",
    body: "Replies stream in token by token from OpenAI, Anthropic, Gemini, or any OpenAI-compatible endpoint — Grok, Ollama, or LM Studio.",
  },
  {
    title: "Pinned panes & @-mentions",
    body: "Planned: keep the AI watching specific panes, or @-reference a pane by name in a single message — without changing focus.",
  },
];

export const ROADMAP = [
  {
    phase: "Phase 1",
    name: "Foundation",
    status: "done",
    body: "GPU-rendered terminal: PTY, ANSI truecolor, keyboard input, scrollback, resize, search, copy/paste.",
  },
  {
    phase: "Phase 2",
    name: "Workspace",
    status: "done",
    body: "Tabs, split panes, drag-to-rearrange tiling, a widget sidebar, keyboard shortcuts, command palette, and session restore.",
  },
  {
    phase: "Phase 3",
    name: "Intelligence",
    status: "done",
    body: "AI chat panel with active-pane context, multi-provider streaming, a settings GUI, and a theme engine.",
  },
  {
    phase: "Phase 4",
    name: "Extras",
    status: "current",
    body: "Embedded browser, file preview, and Lua hooks have landed. Desktop integration and packaging (deb, rpm, homebrew, winget) are in progress.",
  },
  {
    phase: "Phase 5",
    name: "AI Harness",
    status: "upcoming",
    body: "A native agentic AI coding assistant in the terminal: tool-use, self-improving techniques, a pluggable provider system.",
  },
];

export const ACK = [
  { name: "alacritty_terminal", what: "VT parsing & screen model" },
  { name: "iced", what: "GUI framework" },
  { name: "portable-pty", what: "cross-platform PTY (WezTerm)" },
  { name: "cosmic-text", what: "text shaping (HarfBuzz)" },
  { name: "glyphon", what: "GPU text rendering" },
  { name: "wry", what: "webview embedding (Tauri)" },
  { name: "mlua", what: "Lua 5.4 bindings" },
];

// Build a base-aware URL for internal links / public assets.
export function url(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  if (path.startsWith("http") || path.startsWith("#")) return path;
  return base + (path.startsWith("/") ? path : "/" + path);
}
