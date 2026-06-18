// Central content + config for the Alterm site.
// Edit data here; components stay presentation-only.

export const SITE = {
  name: "Alterm",
  tagline: "The alternative terminal — for beginners, intermediates, and experts.",
  description:
    "A GPU-accelerated, cross-platform terminal workspace built in Rust. The workspace UX of WaveTerm with the raw performance of WezTerm — without the Electron bloat.",
  repo: "https://github.com/rhask87062/alterm",
  version: "0.1.0",
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
    body: "Drag-to-split blocks with a 7-zone drop model, tabs, resize handles, and block zoom. A real workspace, not just tabs.",
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
    body: "Every workspace action is bound. Modifiers, leader chords, and a command palette put power users entirely on the keyboard.",
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
    body: "A real webview pane via wry — docs and dashboards alongside your shell, using the system's native engine.",
    status: "planned",
  },
  {
    icon: "folder",
    title: "File preview",
    body: "Syntax-highlighted code, rendered markdown, and inline images in a dedicated block. Read without leaving the workspace.",
    status: "planned",
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
    title: "Active pane",
    body: "The AI automatically sees whichever terminal pane is focused. Switch focus, and its context follows you.",
  },
  {
    title: "Pinned panes",
    body: "Pin specific panes so the AI keeps watching them — your build output and server logs stay in view while you work elsewhere.",
  },
  {
    title: "@ mentions",
    body: "Type @pane:2 or @tab:ssh-prod to reference a pane's output in a single message without pinning it permanently.",
  },
];

export const ROADMAP = [
  {
    phase: "Phase 1",
    name: "Foundation",
    current: true,
    body: "Single GPU-rendered terminal pane: PTY, ANSI colors, keyboard input, scrollback, resize, copy/paste.",
  },
  {
    phase: "Phase 2",
    name: "Workspace",
    body: "Tab bar, split panes, drag-to-split, tiling tree, widget sidebar, keyboard shortcuts, command palette.",
  },
  {
    phase: "Phase 3",
    name: "Intelligence",
    body: "AI chat panel, multi-provider support, terminal context awareness, settings GUI, theme engine.",
  },
  {
    phase: "Phase 4",
    name: "Extras",
    body: "Embedded browser, file preview, Lua hooks, desktop integration, and packaging (deb, rpm, homebrew, winget).",
  },
  {
    phase: "Phase 5",
    name: "AI Harness",
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
