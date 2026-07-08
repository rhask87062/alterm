# Browser Build-Out: Global History, Bookmarks & Full-Browser UX

**Date:** 2026-07-08
**Status:** Implemented

## Goal

Make alterm's embedded browser feel like a full browser rather than a tacked-on
feature: a global, persistent, clickable history archive; a smart URL bar;
bookmarks; find-in-page and zoom; and browser-standard keyboard shortcuts.

## Constraints

- Browser panes are native X11 webviews (wry/webkit2gtk) composited **on top of**
  the iced UI. iced cannot draw over the page area, so no floating dropdowns or
  overlays over the webview. Chrome UI must live in the nav-bar region (which may
  grow rows, shrinking the webview via `set_bounds`) or inside the webview itself.
- Webviews and the iced loop share the main thread (`wry::WebView` is `!Send`);
  existing event plumbing uses thread-local queues drained on `Tick`.
- wry 0.55 (devtools feature) is the pinned dependency; it provides
  `with_custom_protocol`, `with_document_title_changed_handler`,
  `with_on_page_load_handler`, `with_ipc_handler`,
  `with_initialization_script`, and `WebView::zoom`.
- Linux/X11 is the primary target; the browser crate already stubs other
  platforms, and new platform-specific features follow the same gating.

## Architecture

### 1. Data layer & internal pages (crates/browser)

**`history.rs`**
- `HistoryStore`: global visit log, append-only JSONL at
  `~/.local/share/alterm/history.jsonl` (XDG data dir). One record per visit:
  `{url, title, timestamp}`. Full list held in memory for queries.
- On load: skip corrupt lines (log a warning, never fatal). If the file exceeds
  ~100,000 entries, compact to the newest 50,000 (rewrite file).
- Operations: `record_visit(url)`, `set_title(url, title)` (retro-fills the most
  recent matching entry), `query(filter) -> entries newest-first`,
  `delete(entry)`, `clear()`.
- `BookmarkStore`: `bookmarks.json` (small, rewritten wholesale). Operations:
  `toggle(url, title)`, `is_bookmarked(url)`, `list()`.
- Both stores live in thread-local state in the browser crate (same pattern as
  the existing `NAV_EVENTS` queue — main-thread only, no locking).
- Store I/O failures degrade gracefully: browsing continues, recording is
  skipped, warning logged.

**`internal_pages.rs`**
- Pure functions generating HTML for the internal pages, styled to match
  alterm's dark theme:
  - `alterm://history` — entries grouped by day (Today / Yesterday / date),
    each row: title, URL, time; clickable (plain `<a href>` to the target).
    Search box filters via `alterm://history?q=...`. Per-entry delete and a
    Clear-history control. Bookmarks section pinned at the top (this page
    doubles as the start page).
  - `alterm://bookmarks` — bookmark list with remove controls; cross-linked
    with the history page.
  - Unknown `alterm://` paths render a simple internal error page.
- **All titles and URLs are HTML-escaped** — page titles are untrusted input;
  escaping is unit-tested against hostile strings.

**Wiring (webview_manager.rs + main.rs)**
- Every webview is built with `with_custom_protocol("alterm", ...)` serving the
  internal pages from the stores.
- Fresh navigations (not confirmed back/forward moves, not `alterm://` URLs)
  are recorded to the global store at the same point per-pane history is
  updated today (`BrowserState::on_navigation` call site).
- `with_document_title_changed_handler` queues `(pane_id, title)` events in a
  new thread-local queue; drained on `Tick` alongside nav events. On drain:
  set `BrowserState.title`, retro-fill the history entry's title.
- `with_on_page_load_handler` (Started/Finished) queues load-state events for
  accurate `loading` state.
- Destructive actions on internal pages (delete entry, clear history, remove
  bookmark) post via `window.ipc.postMessage`; the IPC handler queues them;
  drained on `Tick` → mutate store → reload the internal page.

### 2. Navigation bar & URL bar UX (alterm/src/main.rs + browser crate)

- **Smart URL bar**: a new `resolve_input(input) -> url` classification applied
  ONLY to URL-bar submissions (`BrowserState::navigate`), never to navigation
  events reported by the webview (those are already real URLs and keep using
  `normalise_url` unchanged):
  - has a scheme, or is `about:`/`alterm:` → navigate directly;
  - contains a dot and no spaces (or is `localhost[:port]`) → prepend
    `https://` and navigate;
  - anything else → search: `https://duckduckgo.com/?q=<url-encoded>`.
  - Search engine URL template configurable as `browser.search_engine` in
    alterm's config; DuckDuckGo is the default.
- **Nav bar layout** (left→right): back ◀, forward ▶, reload ↻ (swaps to stop ✕
  while loading), URL input (with a small spinner glyph at its right edge while
  loading, animated off the existing `Tick`), star ★ (filled when the current
  URL is bookmarked; toggles), history 🕓 (navigates to `alterm://history`).
- **Titles**: the pane title bar shows `display_title()` — real page title once
  the title event lands, URL as fallback (already implemented, now actually fed).
- **Start page**: new browser panes open `alterm://history` instead of a
  hardcoded URL.

### 3. Keyboard shortcuts, find-in-page, zoom

**Shortcuts** (browser pane focused): `Alt+Left`/`Alt+Right` back/forward,
`Ctrl+L` focus URL bar, `Ctrl+R` reload, `Ctrl+H` history page, `Ctrl+D`
toggle bookmark, `Ctrl+F` find bar, `Ctrl+=`/`Ctrl+-`/`Ctrl+0` zoom
in/out/reset. Two capture paths (the native webview swallows keys when it has
focus):
- iced-side: existing `KeyboardInput`/`match_shortcut` path.
- page-side: `with_initialization_script` installs a keydown listener for
  exactly these combos and forwards them via IPC; both paths funnel into the
  same `Message` variants.

**Find-in-page**: `Ctrl+F` opens a second chrome row under the nav bar (query
input, prev/next, match count, close). The webview shrinks by the row height
via the existing `set_bounds` machinery — no overlay, respecting the X11
constraint. Backend: webkit2gtk `FindController` obtained through wry's Linux
`WebViewExtUnix` (adds a Linux-only `webkit2gtk` dependency to the browser
crate; other platforms no-op, matching existing gating).

**Zoom**: per-pane zoom level in `BrowserState`, applied via `WebView::zoom()`,
persisted in the session `BlockState::Browser` (bump `SESSION_VERSION` handling
so old session files still load with a default zoom).

## Data flow summary

```
webview events (nav / title / load-state / ipc actions)
  → thread-local queues (webview_manager)
  → drained on iced Tick (main.rs)
  → BrowserState (per-pane) + HistoryStore/BookmarkStore (global)
  → UI re-render (nav bar, pane title) / internal page reload
```

## Not in scope

- Multiple browser profiles, cookie/site-data management UI
- Downloads manager
- Tabs-within-a-pane (alterm panes/tabs already cover this)
- Favicon fetching/caching
- Cross-pane webview keying fix (tracked separately as tech debt)

## Testing

- Unit tests (browser crate, same style as existing `lib.rs` tests):
  - HistoryStore: append, reload from disk, corrupt-line skip, compaction,
    query filtering, delete/clear, title retro-fill
  - BookmarkStore: toggle/list/persistence
  - Smart-URL classification (URL vs search vs localhost vs alterm://)
  - Internal-page HTML generation, including escaping of hostile titles/URLs
  - BrowserState: title events, zoom field, loading transitions from
    load-state events
- Manual verification of webview-dependent behavior (find bar, zoom, IPC
  actions, shortcuts inside the page) via the running app.

## Error handling

- Store I/O failures: log warning, keep browsing (no recording).
- Corrupt JSONL lines: skipped on load.
- Unknown `alterm://` paths: internal error page.
- Webview API failures (find/zoom on unsupported platform): no-op with log.
