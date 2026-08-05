# Browser Popups and New-Tab Links — Design

**Date:** 2026-08-05
**Status:** Approved
**Problem:** "Sign in with Google" fails in browser panes. The in-page Google
account-chooser overlay appears, but clicking Continue calls `window.open()`,
which alterm's webviews do not handle — the call silently returns `null` and
nothing happens. The same missing handler means `target="_blank"` links do
nothing at all.

## Goals

- Google Sign-In (and OAuth popup flows generally) work end-to-end in browser
  panes: the popup opens, the user completes sign-in, the popup messages the
  opener page and closes itself.
- Clicked `target="_blank"` links open as a new alterm tab containing a
  browser pane at the link URL.

## Non-goals

- Persisting popups across session restore (popups are transient).
- Popup support on macOS/Windows (browser panes are Linux-only today).
- A user-agent override for Google's embedded-browser detection — contingency
  only, see Risks.

## Approach (chosen: native WebKitGTK `create` signal, related webviews)

Alternatives considered and rejected:

- **wry's `with_new_window_req_handler`** — only exposes the URL and an
  allow/deny bool; any webview we opened for it would be process-unrelated, so
  `window.opener` / `postMessage` would not work and OAuth would still fail.
- **Navigate the current pane to the popup URL** — breaks the
  message-back-to-opener flow the same way; rejected on UX grounds too.

The chosen approach connects WebKit's `create` signal directly on the
underlying `webkit2gtk::WebView` (we already drop to webkit2gtk for the
find-controller), and uses WebKit's related-view mechanism so popups share the
opener's web process and session.

## Design

### 1. Architecture

All native code lives in the browser crate, Linux-only like the rest of it:

- `connect_create_handler(webview, pane_id)` in
  `crates/browser/src/webview_manager.rs` (or a small `popup.rs` module),
  called at the end of `create_webview` alongside the existing
  find-controller hookup.
- The app layer (`alterm/src/main.rs`) only drains one new event queue
  (see §3).

### 2. OAuth popups (`window.open`)

When `create` fires for a scripted window-open (WebKit's navigation type
distinguishes scripted opens from clicked links):

- Build a `webkit2gtk::WebView` **related to** the opener (same web process,
  same session/cookies) inside a plain floating `gtk::Window`.
- `ready-to-show`: size the window from the site's requested window features
  (fallback ~500×640, centered), then show it.
- WebKit `close` signal (page called `window.close()`): destroy the window.
  The user can also close it like any normal window.
- Return the new webview from the `create` handler; WebKit loads popup
  content into it.
- Popups get the same `create` handling recursively (popups can open popups).
- Track open popups per opener pane id so destroying the pane/tab also closes
  its orphaned popups.
- Popup webviews get **no** wry IPC handler — no new surface for the
  origin-gated `alterm://` IPC commands.

Because the popup is process-related, `window.opener` and `postMessage` work,
which is exactly what Google's account chooser needs to hand the sign-in
result back to the page and self-close.

### 3. `_blank` links → new alterm tab

Same `create` signal, but for a clicked link:

- Return no webview; push `(opener_pane_id, url)` onto a new thread-local
  queue with a `drain_new_tab_events()` accessor — same pattern as the
  existing nav/title/load/find queues, including the non-Linux stub in
  `crates/browser/src/lib.rs`.
- `apply_browser_webview_events` in main.rs drains the queue and opens a new
  tab containing a browser pane at that URL, focused.
- The new tab is intentionally unrelated to the opener (matches modern
  browsers' default `noopener` behavior for links).

### 4. Error handling / edge cases

- Popups that open `about:blank` and are then scripted by the opener work
  naturally (related view).
- Popups are never persisted by session restore.
- Multiple simultaneous popups are fine (tracked in a list).
- Opener pane destroyed while its popup is open → popup is closed too.

### 5. Testing

- Unit-test the pure classification logic (navigation type + frame name →
  `Popup` vs `NewTab`) and the queue plumbing.
- `cargo clippy` + build.
- Manual verification in the running app:
  - Google sign-in on a real site (e.g. claude.ai) end-to-end.
  - A generic `window.open()` test page.
  - A `target="_blank"` link opens a new alterm tab.
  - `window.close()` self-dismissal of a popup.

## Risks / contingencies

- **Google embedded-browser rejection:** if Google serves "this browser or
  app may not be secure" despite the working popup, the mitigation is an
  explicit Safari-compatible user-agent via `WebKitSettings` on all webviews.
  Deliberately out of scope unless manual verification hits it.
- **Navigation-type classification:** if WebKit reports an unexpected
  navigation type for some site's open request, the fallback is to treat it
  as a popup (a floating window that works beats a dead click).
