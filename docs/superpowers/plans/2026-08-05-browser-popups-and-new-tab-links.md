# Browser Popups and New-Tab Links Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `window.open` popups (Google Sign-In) and `target="_blank"` links work in alterm's browser panes.

**Architecture:** Connect WebKitGTK's `create` signal on each pane's underlying `webkit2gtk::WebView` (obtained via `wry::WebViewExtUnix::webview()`). Scripted opens get a process-**related** popup webview in a floating `gtk::Window` (so `window.opener`/`postMessage`/`window.close()` work — required for OAuth); clicked `_blank` links are queued to the app layer, which opens them as a new alterm tab.

**Tech Stack:** Rust, wry 0.55.0 (locked), webkit2gtk-rs =2.0.2 (feature `v2_38`), gtk-rs 0.18, iced pane_grid.

**Spec:** `docs/superpowers/specs/2026-08-05-browser-popups-and-new-tab-links-design.md`

## Global Constraints

- Branch: `feature/browser-popups-new-tab-links` (already created; never commit to `main` directly).
- All native popup code is Linux-only: gate with `#[cfg(target_os = "linux")]`, matching the existing style in `crates/browser/src/webview_manager.rs`.
- Non-Linux builds must keep compiling: every new public `webview_manager` function needs a stub in the fallback module in `crates/browser/src/lib.rs` (see the existing `drain_nav_events` stub at ~line 35).
- NEVER kill running alterm instances (`killall alterm` etc.) — the user works inside alterm.
- wry facts verified against vendored sources: wry only connects the WebKit `create` signal when `new_window_req_handler` is set (alterm never sets it), and wry's `decide-policy` handler returns `false` for `NewWindowAction` decisions, so the default policy applies and `create` fires. Our own `connect_create` therefore has no conflict.
- webkit2gtk-rs 2.0.2 has NO getter for `WindowProperties::geometry` — read it via `ObjectExt::property_value("geometry")` (the `geometry(self, ...)` method that greps find is the *builder* setter).
- No new wry IPC handlers on popup webviews (they are raw `webkit2gtk::WebView`s — they must not gain access to the origin-gated `alterm://` IPC commands).

---

### Task 1: `classify_create` + new-tab event queue (browser crate)

**Files:**
- Modify: `crates/browser/src/webview_manager.rs` (thread_local block ~line 57; public fns near `drain_find_events` ~line 124; tests module ~line 529)
- Modify: `crates/browser/src/lib.rs` (fallback stub module, near existing stubs ~line 35)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub enum CreateDisposition { Popup, NewTab }` (derives `Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn classify_create(is_link_clicked: bool, url: &str) -> CreateDisposition`
  - `pub fn drain_new_tab_events() -> Vec<(u64, String)>` — `(opener_pane_id, url)`, drained by main.rs in Task 3; Task 2's handler pushes into the backing `NEW_TAB_EVENTS` queue.

- [ ] **Step 1: Write the failing tests**

In the existing `#[cfg(test)] mod tests` at the bottom of `crates/browser/src/webview_manager.rs` (next to the `remap_*` tests), add:

```rust
    #[test]
    fn classify_link_clicks_open_new_tab() {
        assert_eq!(
            classify_create(true, "https://example.com/"),
            CreateDisposition::NewTab
        );
    }

    #[test]
    fn classify_scripted_opens_are_popups() {
        // Google's account chooser does window.open() — must be a popup.
        assert_eq!(
            classify_create(false, "https://accounts.google.com/o/oauth2/auth"),
            CreateDisposition::Popup
        );
        // Scripted about:blank popups have no URL yet.
        assert_eq!(classify_create(false, ""), CreateDisposition::Popup);
    }

    #[test]
    fn classify_link_without_url_falls_back_to_popup() {
        assert_eq!(classify_create(true, ""), CreateDisposition::Popup);
    }

    #[test]
    fn new_tab_events_drain_and_clear() {
        NEW_TAB_EVENTS.with(|q| q.borrow_mut().push((7, "https://a.com/".into())));
        assert_eq!(drain_new_tab_events(), vec![(7, "https://a.com/".to_string())]);
        assert!(drain_new_tab_events().is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p browser classify 2>&1 | tail -5`
Expected: compile error — `classify_create`/`CreateDisposition`/`NEW_TAB_EVENTS` not found.

- [ ] **Step 3: Implement**

Add to the `thread_local!` block in `webview_manager.rs` (unconditional, next to `FIND_EVENTS`):

```rust
    /// New-tab requests `(opener_pane_id, url)` queued when a clicked
    /// `target="_blank"` link asks for a new window.
    static NEW_TAB_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
```

Add near `drain_find_events` (match the style of the sibling drain fns — check how they take the Vec, likely `std::mem::take`):

```rust
/// Drain queued open-in-new-tab requests from `target="_blank"` links.
pub fn drain_new_tab_events() -> Vec<(u64, String)> {
    NEW_TAB_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Where a WebKit `create` (new window) request should be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateDisposition {
    /// Scripted `window.open` — needs a floating popup window backed by a
    /// process-related webview (OAuth relies on window.opener/postMessage).
    Popup,
    /// Clicked link targeting a new window — open the URL as a new tab.
    NewTab,
}

/// Classify a `create` request. `is_link_clicked` = navigation type was
/// `LinkClicked`; `url` may be empty for scripted about:blank popups.
/// Unexpected combinations fall back to `Popup`: a floating window that
/// works beats a dead click.
pub fn classify_create(is_link_clicked: bool, url: &str) -> CreateDisposition {
    if is_link_clicked && !url.is_empty() {
        CreateDisposition::NewTab
    } else {
        CreateDisposition::Popup
    }
}
```

In `crates/browser/src/lib.rs`, in the fallback stub module next to the existing `drain_ipc_events` stub:

```rust
    pub fn drain_new_tab_events() -> Vec<(u64, String)> { Vec::new() }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p browser 2>&1 | tail -5`
Expected: all tests pass, including the 4 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/webview_manager.rs crates/browser/src/lib.rs
git commit -m "feat(browser): classify new-window requests; queue _blank links for new tabs"
```

---

### Task 2: `create`-signal handler + floating popup windows (browser crate)

**Files:**
- Modify: `crates/browser/src/webview_manager.rs`
  - thread_local block (~line 57): add `POPUPS`
  - `create_webview` (~line 290, inside the existing `#[cfg(target_os = "linux")]` find-controller block): hook up the handler
  - `destroy` (~line 350): close orphaned popups
  - new private fns `connect_create_handler`, `build_popup`

**Interfaces:**
- Consumes: `classify_create`, `CreateDisposition`, `NEW_TAB_EVENTS` from Task 1.
- Produces: no new public API. Behavior only: popup windows appear/close; `NEW_TAB_EVENTS` gets pushed to.

- [ ] **Step 1: Add the `POPUPS` thread-local**

```rust
    /// Open popup windows `(opener_pane_id, window)` so a pane's popups can
    /// be closed when the pane is destroyed.
    #[cfg(target_os = "linux")]
    static POPUPS: RefCell<Vec<(u64, gtk::Window)>> = const { RefCell::new(Vec::new()) };
```

- [ ] **Step 2: Add the handler functions**

Add near the bottom of the file (before the tests module). Note the file's convention: trait imports live inside function bodies, types are fully qualified.

```rust
/// Route WebKit "create" (new window) requests for the webview belonging to
/// `pane_id`: scripted popups get a floating related-webview window; clicked
/// `_blank` links are queued for the app layer to open as a new tab.
#[cfg(target_os = "linux")]
fn connect_create_handler(webview: &webkit2gtk::WebView, pane_id: u64) {
    use webkit2gtk::WebViewExt;
    webview.connect_create(move |view, action| {
        let url = action
            .request()
            .and_then(|r| r.uri())
            .map(|u| u.to_string())
            .unwrap_or_default();
        let is_link =
            action.navigation_type() == webkit2gtk::NavigationType::LinkClicked;
        match classify_create(is_link, &url) {
            CreateDisposition::NewTab => {
                log::info!("[popup] _blank link -> new tab: pane={pane_id} url={url}");
                NEW_TAB_EVENTS.with(|q| q.borrow_mut().push((pane_id, url)));
                None
            }
            CreateDisposition::Popup => {
                use gtk::prelude::Cast;
                log::info!("[popup] window.open -> popup: pane={pane_id} url={url:?}");
                Some(build_popup(view, pane_id).upcast::<gtk::Widget>())
            }
        }
    });
}

/// Build a popup webview **related** to `opener` (same web process and
/// session — required so window.opener/postMessage reach the opener page)
/// inside its own floating window. Returns the webview; WebKit loads the
/// popup content into it. Deliberately gets NO wry IPC handler.
#[cfg(target_os = "linux")]
fn build_popup(opener: &webkit2gtk::WebView, opener_pane: u64) -> webkit2gtk::WebView {
    use gtk::prelude::{Cast, ContainerExt, GtkWindowExt, WidgetExt};
    use webkit2gtk::WebViewExt;

    let popup = webkit2gtk::WebView::builder().related_view(opener).build();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("alterm");
    window.add(&popup);

    // Show only once WebKit has applied the site's window features; size
    // from the requested geometry when given, else a sensible OAuth default.
    let win = window.clone();
    popup.connect_ready_to_show(move |wv| {
        let (mut w, mut h) = (500, 640);
        if let Some(props) = wv.window_properties() {
            if let Ok(geo) = props.property_value("geometry").get::<gtk::gdk::Rectangle>() {
                if geo.width() > 0 && geo.height() > 0 {
                    w = geo.width().clamp(200, 1600);
                    h = geo.height().clamp(200, 1200);
                }
            }
        }
        win.set_default_size(w, h);
        win.set_position(gtk::WindowPosition::Center);
        win.show_all();
    });

    // Keep the window title in sync with the page.
    let win = window.clone();
    popup.connect_title_notify(move |wv| {
        if let Some(t) = wv.title() {
            win.set_title(&t);
        }
    });

    // The page called window.close() (OAuth popups do this when done).
    let win = window.clone();
    popup.connect_close(move |_| win.close());

    // Popups can themselves open popups or _blank links; attribute them to
    // the original opener pane.
    connect_create_handler(&popup, opener_pane);

    // Track for cleanup; self-remove when the window is destroyed (either
    // via window.close() above or the user closing it).
    POPUPS.with(|p| p.borrow_mut().push((opener_pane, window.clone())));
    window.connect_destroy(|w| {
        POPUPS.with(|p| p.borrow_mut().retain(|(_, win)| win != w));
    });

    popup
}
```

Notes for the implementer:
- `property_value` needs `ObjectExt`, already imported at the top of the file (`gtk::prelude::ObjectExt`).
- If `connect_title_notify` doesn't exist under that name in webkit2gtk-rs 2.0.2, check `~/.cargo/registry/src/*/webkit2gtk-2.0.2/src/auto/web_view.rs` for the generated notify connector (it is `connect_title_notify`) — do not guess alternatives, read the file.
- `gtk::gdk` is gtk-rs 0.18's re-export of gdk; no new Cargo dependency.

- [ ] **Step 3: Hook the handler into `create_webview`**

In the existing `#[cfg(target_os = "linux")]` block at the end of `create_webview` (the one that connects the find controller, ~line 290), after the `connect_counted_matches` hookup add:

```rust
        // Handle window.open popups and target="_blank" links (Google
        // sign-in and friends). See connect_create_handler.
        connect_create_handler(&webview.webview(), pane_id);
```

(`use wry::WebViewExtUnix;` is already in scope in that block.)

- [ ] **Step 4: Close orphaned popups in `destroy`**

In `pub fn destroy(pane_id: u64)` (~line 350), inside its Linux path, add:

```rust
    // Close any popup windows this pane opened; each window's destroy
    // handler removes its own POPUPS entry.
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;
        let orphans: Vec<gtk::Window> = POPUPS.with(|p| {
            p.borrow()
                .iter()
                .filter(|(owner, _)| *owner == pane_id)
                .map(|(_, w)| w.clone())
                .collect()
        });
        for w in orphans {
            w.close();
        }
    }
```

(Collect first, then close: `close()` eventually re-enters `POPUPS` via the destroy handler, so no borrow may be held while closing.)

- [ ] **Step 5: Build and lint**

Run: `cargo build -p browser 2>&1 | tail -5` — Expected: success.
Run: `cargo clippy -p browser 2>&1 | tail -5` — Expected: no new warnings.
Run: `cargo test -p browser 2>&1 | tail -5` — Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/browser/src/webview_manager.rs
git commit -m "feat(browser): related-webview popup windows for window.open (Google sign-in)"
```

---

### Task 3: Drain new-tab events and open browser tabs (app layer)

**Files:**
- Modify: `alterm/src/main.rs`
  - `apply_browser_webview_events` (~line 869): drain the new queue
  - new helper `open_url_in_new_tab` near `create_browser_webview` (~line 765)

**Interfaces:**
- Consumes: `webview_manager::drain_new_tab_events() -> Vec<(u64, String)>` (Task 1); existing `Block::new_browser(url)`, `Tab::from_parts(title, panes, focus)`, `self.create_browser_webview(pane, url)`, `webview_key(tab_id, pane)`, `self.resize_all_panes()`, `self.update_webview_visibility()`.
- Produces: browser tabs opening in response to `_blank` clicks. No new public API.

- [ ] **Step 1: Add the helper**

Near `create_browser_webview` in main.rs:

```rust
    /// Open `url` as a browser pane in a fresh tab and focus that tab. Used
    /// by target="_blank" links clicked inside webviews.
    fn open_url_in_new_tab(&mut self, url: &str) {
        let block = Block::new_browser(url);
        let (panes, pane) = pane_grid::State::new(block);
        let tab = Tab::from_parts("Browser".to_string(), panes, Some(pane));
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.create_browser_webview(pane, url);
        // Apply persisted zoom if non-default (mirrors Message::OpenBrowser).
        let tab_id = self.active_tab().id;
        if let Some(Block::Browser { state }) = self.active_tab().panes.get(pane) {
            if (state.zoom - 1.0).abs() > f64::EPSILON {
                webview_manager::set_zoom(webview_key(tab_id, pane), state.zoom);
            }
        }
        webview_manager::pump_gtk_events();
        self.resize_all_panes();
        self.update_webview_visibility();
    }
```

- [ ] **Step 2: Drain the queue**

In `apply_browser_webview_events`, after the IPC-events loop and before the find-events drain, add:

```rust
        // Open new tabs requested by target="_blank" link clicks.
        for (_opener, url) in webview_manager::drain_new_tab_events() {
            self.open_url_in_new_tab(&url);
        }
```

- [ ] **Step 3: Build and lint the workspace**

Run: `cargo build 2>&1 | tail -5` — Expected: success.
Run: `cargo clippy --workspace 2>&1 | tail -5` — Expected: no new warnings.
Run: `cargo test --workspace 2>&1 | tail -8` — Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): open _blank links from webviews as new browser tabs"
```

---

### Task 4: Verification, install, docs

**Files:**
- Create: `/tmp/alterm-popup-test.html` (throwaway, not committed)
- Modify: `docs/superpowers/specs/2026-08-05-browser-popups-and-new-tab-links-design.md` (status line)

**Interfaces:** none — verification only.

- [ ] **Step 1: Write the manual test page**

```html
<!doctype html>
<title>alterm popup test</title>
<button onclick="w=window.open('https://example.com','','width=480,height=600')">window.open popup</button>
<button onclick="w=window.open('')">window.open blank</button>
<button onclick="w&&w.close()">close popup</button>
<a href="https://example.com" target="_blank">_blank link</a>
<script>window.addEventListener('message',e=>document.title='got msg')</script>
```

- [ ] **Step 2: Install the new build**

Run: `cargo install --path alterm 2>&1 | tail -2`
Expected: `Replaced package alterm v0.4.0 ...`
Do NOT restart or kill the user's running alterm.

- [ ] **Step 3: Hand off to the user for interactive verification**

Ask the user to relaunch alterm and check, per the spec's test list:
1. `file:///tmp/alterm-popup-test.html` — popup button opens a floating window (~480×600); blank button opens a default-size popup; close button dismisses it; the `_blank` link opens a new alterm tab.
2. Google sign-in on a real site (e.g. claude.ai) completes end-to-end and the popup closes itself.
3. Closing a browser pane while its popup is open also closes the popup.

If Google shows "this browser or app may not be secure", invoke the spec's UA contingency (Safari-compatible user-agent via `WebKitSettings`) as a follow-up task — do not improvise mid-verification.

- [ ] **Step 4: Mark spec implemented and check the website claims**

- Change the spec's `**Status:** Approved` line to `**Status:** Implemented (2026-08-05)`.
- Check the website doesn't now understate/overstate browser features: `search_text` for `popup|sign.?in` under `website/` — expected: no claims to update; if the browser feature list in `website/src/data/site.ts` enumerates capabilities, adding "popup & sign-in support" is optional and needs no design cycle.

```bash
git add docs/superpowers/specs/2026-08-05-browser-popups-and-new-tab-links-design.md
git commit -m "docs(alterm): mark popup/new-tab spec implemented"
```

---

## Self-review notes

- Spec §2 popups → Task 2; §3 `_blank` → Tasks 1+3; §4 edge cases → Task 2 steps 2/4 (about:blank via related view + fallback classify; cleanup on destroy); §5 testing → Task 1 (unit) + Task 4 (manual). UA contingency intentionally deferred (spec Risks).
- Popups get no wry IPC handler (raw webkit2gtk views) — spec §2 security note holds by construction.
- Type check: `classify_create(bool, &str) -> CreateDisposition` and `drain_new_tab_events() -> Vec<(u64, String)>` used identically in Tasks 1–3.
