# Browser Pane Tabs — Design

**Date:** 2026-08-06
**Status:** Approved
**Branch:** `feature/browser-pane-tabs` (based on
`feature/browser-popups-new-tab-links`, which merges with it)
**Problem:** A browser pane can show exactly one website. Users who want
several sites open must split panes or open workspace tabs, both of which are
heavier than what a browser naturally offers: tabs inside the browser itself.

## Goals

- A single browser pane can hold many sites at once, each in its own tab.
- Tab switching is instant and lossless: scroll position, form input, and
  playing media are preserved (each tab keeps a live webview).
- `target="_blank"` links open as a new tab in the same pane (replacing the
  new-workspace-tab behavior from the popup branch).
- Sessions persist and restore all tabs per pane, backward-compatibly.

## Non-goals (deferred)

- Drag-to-reorder, pinned tabs, audio indicators, tab overflow scrolling.
- Tab sleeping / LRU eviction (Approach C) — can be layered on later if
  memory becomes a concern.
- Favicons in the tab strip.

## Approach (chosen: A — one live webview per tab)

Alternatives considered and rejected:

- **B: one webview per pane, switch-by-navigation** — every switch reloads
  the page, losing scroll/forms/media. Tabs in name only.
- **C: hybrid LRU (N live, rest sleeping)** — B's memory bound with most of
  A's feel, but needs eviction logic and sleeping-tab UI. Overkill for v1;
  compatible with A's UI if ever needed.

A is how real browsers behave, and it *simplifies* webview bookkeeping:
webview keys stop being derived from (workspace-tab, pane) position and
become permanent per-tab ids, deleting the remap-on-layout-change machinery
(known tech debt).

## Design

### 1. Data model

- `crates/browser`: `BrowserState` is unchanged — it is already exactly
  per-page state (url, input_url, history, zoom, loading, title, nav flags).
- New `BrowserPaneState { tabs: Vec<BrowserTab>, active: usize }` with
  `BrowserTab { webview_id: u64, state: BrowserState }`.
  `Block::Browser { state: BrowserPaneState }`.
- `webview_id` is allocated from a global monotonic `AtomicU64` in the
  browser crate and is the key used for every `webview_manager` call. It
  never changes for the life of the tab.
- Pure tab operations live on `BrowserPaneState` (unit-testable, no GTK):
  open-at-end (Ctrl+T / `+` button), open-after-active (`_blank` links) —
  both focus the new tab; close (returns the closed tab's webview_id;
  adjusts `active`; reports when the last tab closed so the caller closes
  the pane), switch, next/prev with wraparound.

### 2. Webview keying cleanup

- `compose_key` / `webview_key` / `webview_manager::remap` and the re-keying
  calls in `add_window` (and any other layout-change re-key sites) are
  deleted. Browser webviews are always addressed by the stored per-tab id.
- Events drained from the webview manager (nav, title, load, IPC, find,
  new-tab) arrive keyed by webview id. The app layer resolves an id to
  (workspace tab, pane, tab index) by scanning tabs' browser panes — N is
  small, and the scan replaces key decomposition.
- `browser_find` in the app layer stays keyed by (workspace tab id, pane);
  it applies to the pane's active tab.

### 3. UI (tab strip)

- New chrome row `BROWSER_TAB_BAR_HEIGHT` (~30 px) at the top of every
  browser pane, above the nav bar; always visible.
- Contents: one button per tab (equal widths, ellipsized `display_title()`,
  active tab visually distinct, × close button, middle-click closes) plus a
  `+` new-tab button at the end.
- `browser_chrome_height` and the webview-bounds math in `resize_all_panes`
  gain the tab-bar height. Nav bar, find bar, and zoom all show/drive the
  active tab's state.
- New messages: `BrowserTabSelected(pane, idx)`,
  `BrowserTabNew(pane)`, `BrowserTabClose(pane, idx)`,
  `BrowserTabNext(pane)` / `BrowserTabPrev(pane)`.

### 4. Behavior

- **Switch:** hide the old tab's webview, show + re-bounds the new one.
  Nothing reloads. An open find bar is finished (closed) on switch.
- **New tab (Ctrl+T / `+`):** appended at the end of the strip, opens the
  same start page as a new browser pane, URL bar focused.
- **Close tab:** `webview_manager::destroy(webview_id)` (which already
  closes popups the tab opened), remove from `tabs`, fix `active`. Closing
  the last tab closes the pane (funnels into the existing close-pane path,
  which now destroys all remaining tab webviews).
- **`_blank` links / popup-branch NewTab classification:** the drained
  `(opener_webview_id, url)` event opens a new tab in the owning pane,
  inserted after the active tab, focused. `open_url_in_new_tab` (workspace
  tab) is replaced by this.
- **Visibility:** `update_webview_visibility` shows only the active tab's
  webview of each visible browser pane; all other tabs' webviews are hidden
  (background tabs keep running, just unmapped). Maximize/workspace-tab
  switching hide *all* tabs of hidden panes.
- **Zoom:** per tab (already lives in `BrowserState`); applied on restore
  and on switch-created webviews as today.

### 5. Shortcuts

When a browser pane has focus (iced path) or the page has focus (existing
in-page IPC key-forwarder path, extended with these combos):

- **Ctrl+T** — new tab; **Ctrl+W** — close active tab;
- **Ctrl+PageDown / Ctrl+PageUp** — next / previous tab (wraparound).

No conflicts: workspace tabs use Ctrl+Shift+T/W and Ctrl+(Shift+)Tab;
terminal panes keep sending Ctrl+T/W to the shell. The implementation plan
verifies Ctrl+PageUp/Down are unbound elsewhere before wiring them.

### 6. Persistence

- `BlockState::Browser` gains `#[serde(default)] tabs: Vec<BrowserTabState>`
  and `#[serde(default)] active_tab: usize`, where `BrowserTabState { url,
  history, history_index, zoom }`.
- The legacy flat fields (`url`, `history`, `history_index`, `zoom`) remain
  and are written from the active tab, so older binaries can still read new
  session files (they restore the active tab only).
- Restore: if `tabs` is non-empty, rebuild every tab (fresh webview ids)
  and the active index (clamped); otherwise build a single tab from the
  legacy fields. Old session files therefore restore as one-tab panes.
- Popups remain unpersisted.

### 7. Error handling / edge cases

- Closing a tab left of the active one shifts `active` down by one; closing
  the active tab activates its right neighbor (or new last tab).
- Destroying a pane/tab/workspace-tab destroys every tab webview it owns
  (and, via the existing popup tracking, their popups).
- Webview-id scan misses (event for an already-closed tab) are dropped
  silently — same as today's stale-key behavior.
- `alterm://` internal pages work per tab (the custom protocol is
  registered on the shared WebContext).

### 8. Testing

- Unit tests (pure, no GTK): `BrowserPaneState` open/close/switch/next/prev,
  active-index adjustment for closes left/right of active, close-last
  reports pane-close, `_blank` insertion position; session round-trip
  including decoding a legacy (flat) `BlockState::Browser` JSON.
- `cargo clippy` + full build; existing browser tests keep passing.
- Manual verification: multi-tab browsing; switch preserves scroll and
  playing video; Ctrl+T/W/PageUp/PageDown from both pane focus and page
  focus; `_blank` opens in-pane; Google sign-in popup still works from a
  background *and* foreground tab; close-last-tab closes the pane; session
  restore of a multi-tab pane; old session file restores as one tab.

## Risks / contingencies

- **Memory growth with many tabs:** accepted (normal browser cost). If it
  bites, Approach C (tab sleeping) layers on without UI changes.
- **X11 map/unmap churn on rapid switching:** the same show/hide path the
  pane system already uses; if flicker appears, reuse whatever mitigation
  the maximize path uses today.
- **Keybinding collisions:** if Ctrl+PageUp/Down turn out to be taken (e.g.
  terminal scroll), fall back to Ctrl+Shift+PageUp/Down and note it in the
  hotkey pane.
