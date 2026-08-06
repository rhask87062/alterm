/// Browser state management and embedded web view (wry/webkit2gtk).
///
/// This crate provides:
/// - `BrowserState`: tracks URL, navigation history, and loading status.
/// - `webview_manager`: manages real wry `WebView` instances on the main thread.

pub mod history;
pub mod internal_pages;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod webview_manager;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod webview_manager {
    /// Embedded browser is not supported on this platform.
    pub fn init_gtk() {}
    pub fn pump_gtk_events() {}
    pub fn set_data_dir(_path: &std::path::Path) {}
    pub fn create_webview(
        _pane_id: u64,
        _parent_window: u64,
        _url: &str,
        _bounds: (f64, f64, f64, f64),
    ) -> Result<(), String> {
        Err("Embedded browser is not supported on this platform.".to_string())
    }
    pub fn navigate(_pane_id: u64, _url: &str) {}
    pub fn set_bounds(_pane_id: u64, _x: f64, _y: f64, _w: f64, _h: f64) {}
    pub fn set_visible(_pane_id: u64, _visible: bool) {}
    pub fn destroy(_pane_id: u64) {}
    pub fn exists(_pane_id: u64) -> bool { false }
    pub fn reload(_pane_id: u64) {}
    pub fn go_back(_pane_id: u64) {}
    pub fn go_forward(_pane_id: u64) {}
    pub fn drain_nav_events() -> Vec<(u64, String)> { Vec::new() }
    pub fn drain_title_events() -> Vec<(u64, String)> { Vec::new() }
    pub fn drain_load_events() -> Vec<(u64, bool)> { Vec::new() }
    pub fn drain_ipc_events() -> Vec<(u64, String, String)> { Vec::new() }
    pub fn drain_find_events() -> Vec<(u64, u32)> { Vec::new() }
    pub fn drain_new_tab_events() -> Vec<(u64, String)> { Vec::new() }
    pub fn stop(_pane_id: u64) {}
    pub fn set_zoom(_pane_id: u64, _level: f64) {}
    pub fn find_start(_pane_id: u64, _text: &str) {}
    pub fn find_next(_pane_id: u64) {}
    pub fn find_prev(_pane_id: u64) {}
    pub fn find_finish(_pane_id: u64) {}
}

/// Manages the state for a single browser pane.
pub struct BrowserState {
    /// The currently loaded URL.
    pub url: String,
    /// The text shown (and editable) in the URL bar.
    pub input_url: String,
    /// Whether a page load is in progress.
    pub loading: bool,
    /// The page title (empty until a page sets it).
    pub title: String,
    /// Whether there is a previous page in the history to go back to.
    pub can_go_back: bool,
    /// Whether there is a next page in the history to go forward to.
    pub can_go_forward: bool,
    /// Ordered list of visited URLs.
    pub history: Vec<String>,
    /// Index into `history` pointing at the current page.
    pub history_index: usize,
    /// A back (-1) or forward (+1) move we initiated on the real webview and
    /// are waiting for the resulting navigation event to confirm. 0 = none.
    ///
    /// This lets `on_navigation` tell a Back/Forward press (which must move the
    /// index without discarding the other direction's history) apart from a
    /// fresh navigation (link click / URL bar) which truncates forward history.
    pub pending_move: i8,
    /// Page zoom factor (1.0 = 100%). Applied via the webview manager.
    pub zoom: f64,
}

impl BrowserState {
    /// Create a new browser state navigated to `url`.
    pub fn new(url: &str) -> Self {
        let url = normalise_url(url);
        BrowserState {
            url: url.clone(),
            input_url: url.clone(),
            loading: false,
            title: String::new(),
            can_go_back: false,
            can_go_forward: false,
            history: vec![url],
            history_index: 0,
            pending_move: 0,
            zoom: 1.0,
        }
    }

    /// Begin navigating to a URL typed in the URL bar.
    ///
    /// Returns the normalised URL to hand to the real webview. The history
    /// stack is *not* updated here — the resulting navigation event flows back
    /// through [`on_navigation`], which is the single source of truth for
    /// history (so in-page link clicks are recorded the same way).
    pub fn navigate(&mut self, url: &str) -> String {
        let url = normalise_url(url);
        self.input_url = url.clone();
        self.loading = true;
        // A URL-bar navigation is a fresh navigation, not a back/forward move.
        self.pending_move = 0;
        url
    }

    /// Record a navigation that actually occurred in the webview (URL-bar
    /// submit, link click, redirect, or a confirmed back/forward move).
    ///
    /// Returns `true` when the navigation was fresh (recorded a new history
    /// entry), `false` for confirmed back/forward moves and duplicate reports.
    /// Callers use this to decide global-history recording.
    ///
    /// This is the single place history is mutated, so every navigation —
    /// however it was triggered — keeps the stack and nav flags accurate.
    pub fn on_navigation(&mut self, url: &str) -> bool {
        let url = normalise_url(url);

        let fresh = match self.pending_move {
            -1 => {
                // Confirmed Back: move the index, keep forward history intact.
                self.history_index = self.history_index.saturating_sub(1);
                self.pending_move = 0;
                false
            }
            1 => {
                // Confirmed Forward: move the index, keep back history intact.
                if self.history_index + 1 < self.history.len() {
                    self.history_index += 1;
                }
                self.pending_move = 0;
                false
            }
            _ => {
                // Fresh navigation. Ignore a duplicate of the current page
                // (e.g. the webview re-reporting the page we're already on).
                if url != self.url {
                    self.history.truncate(self.history_index + 1);
                    self.history.push(url.clone());
                    self.history_index = self.history.len() - 1;
                    true
                } else {
                    false
                }
            }
        };

        self.url = url.clone();
        self.input_url = url;
        self.loading = false;
        self.update_nav_flags();

        log::debug!(
            "Browser on_navigation: index={} history_len={}",
            self.history_index,
            self.history.len()
        );
        fresh
    }

    /// Move Back one entry. Returns `true` if there was somewhere to go, in
    /// which case the caller should drive the real webview's history.
    ///
    /// We update our own index *immediately* rather than waiting for a
    /// navigation event, because webkit's navigation handler does NOT fire for
    /// `history.back()`/`history.forward()` (bfcache restores don't trigger a
    /// navigation-policy decision). The webview honours the move deterministically,
    /// so mirroring it here keeps `can_go_back`/`can_go_forward` accurate. If a
    /// webview *does* report the move, `on_navigation` ignores it as a duplicate
    /// of the page we just moved to.
    pub fn begin_back(&mut self) -> bool {
        if !self.can_go_back {
            return false;
        }
        self.history_index -= 1;
        self.url = self.history[self.history_index].clone();
        self.input_url = self.url.clone();
        self.loading = true;
        self.pending_move = 0;
        self.update_nav_flags();
        true
    }

    /// Move Forward one entry. Returns `true` if there was somewhere to go.
    /// See [`begin_back`](Self::begin_back) for why the index moves immediately.
    pub fn begin_forward(&mut self) -> bool {
        if !self.can_go_forward {
            return false;
        }
        self.history_index += 1;
        self.url = self.history[self.history_index].clone();
        self.input_url = self.url.clone();
        self.loading = true;
        self.pending_move = 0;
        self.update_nav_flags();
        true
    }

    /// Reload the current page.
    pub fn reload(&mut self) {
        self.loading = true;
        log::debug!("Browser reload: {}", self.url);
    }

    /// Update the loading flag from a webview load-state event.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// The URL of the currently loaded page.
    pub fn current_url(&self) -> &str {
        &self.url
    }

    /// A human-readable title: the page title if set, otherwise the URL.
    pub fn display_title(&self) -> String {
        if self.title.is_empty() {
            self.url.clone()
        } else {
            self.title.clone()
        }
    }

    /// Update `can_go_back` / `can_go_forward` from the current history state.
    fn update_nav_flags(&mut self) {
        self.can_go_back = self.history_index > 0;
        self.can_go_forward = self.history_index + 1 < self.history.len();
    }
}

/// Monotonic allocator for browser-tab webview ids. Ids are unique for the
/// process lifetime and are the keys for every `webview_manager` call — they
/// never change when panes or workspace tabs move.
static NEXT_WEBVIEW_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn alloc_webview_id() -> u64 {
    NEXT_WEBVIEW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// One tab inside a browser pane: its page state plus the id of the native
/// webview that renders it.
pub struct BrowserTab {
    /// Permanent key for this tab's native webview (allocated once).
    pub webview_id: u64,
    /// Per-page state: URL, history, zoom, loading, title.
    pub state: BrowserState,
}

impl BrowserTab {
    fn new(url: &str) -> Self {
        BrowserTab { webview_id: alloc_webview_id(), state: BrowserState::new(url) }
    }
}

/// Result of closing a tab via [`BrowserPaneState::close`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabClose {
    /// Webview id of the closed tab — the caller destroys the native webview.
    pub closed_webview_id: u64,
    /// True when this was the pane's last tab. The tab is left in place; the
    /// caller closes the whole pane, which destroys all remaining webviews.
    pub pane_empty: bool,
}

/// All tabs of one browser pane plus which one is active. Pure logic — no
/// GTK/webview calls — so every operation is unit-testable.
pub struct BrowserPaneState {
    pub tabs: Vec<BrowserTab>,
    pub active: usize,
}

impl BrowserPaneState {
    /// A pane with a single tab at `url`.
    pub fn new(url: &str) -> Self {
        BrowserPaneState { tabs: vec![BrowserTab::new(url)], active: 0 }
    }

    /// Rebuild from restored per-tab states (session restore). Allocates
    /// fresh webview ids, clamps `active`, and falls back to a single blank
    /// tab if `states` is empty — a pane never has zero tabs.
    pub fn from_states(states: Vec<BrowserState>, active: usize) -> Self {
        let tabs: Vec<BrowserTab> = states
            .into_iter()
            .map(|state| BrowserTab { webview_id: alloc_webview_id(), state })
            .collect();
        if tabs.is_empty() {
            return BrowserPaneState::new("about:blank");
        }
        let active = active.min(tabs.len() - 1);
        BrowserPaneState { tabs, active }
    }

    pub fn active_tab(&self) -> &BrowserTab {
        &self.tabs[self.active]
    }

    pub fn active_state(&self) -> &BrowserState {
        &self.tabs[self.active].state
    }

    pub fn active_state_mut(&mut self) -> &mut BrowserState {
        &mut self.tabs[self.active].state
    }

    pub fn active_webview_id(&self) -> u64 {
        self.tabs[self.active].webview_id
    }

    /// Append a new tab at the end of the strip (Ctrl+T / `+` button) and
    /// activate it. Returns the new tab's webview id.
    pub fn open_at_end(&mut self, url: &str) -> u64 {
        let tab = BrowserTab::new(url);
        let id = tab.webview_id;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        id
    }

    /// Insert a new tab right after the active one (`_blank` links) and
    /// activate it. Returns the new tab's webview id.
    pub fn open_after_active(&mut self, url: &str) -> u64 {
        let tab = BrowserTab::new(url);
        let id = tab.webview_id;
        let at = self.active + 1;
        self.tabs.insert(at, tab);
        self.active = at;
        id
    }

    /// Close the tab at `idx`. Returns `None` if out of range. When closing
    /// the last remaining tab, reports `pane_empty` WITHOUT removing it (see
    /// [`TabClose`]); otherwise removes the tab and fixes `active` so the
    /// selection stays sensible (right neighbor when closing the active tab).
    pub fn close(&mut self, idx: usize) -> Option<TabClose> {
        if idx >= self.tabs.len() {
            return None;
        }
        if self.tabs.len() == 1 {
            return Some(TabClose {
                closed_webview_id: self.tabs[0].webview_id,
                pane_empty: true,
            });
        }
        let closed = self.tabs.remove(idx);
        if idx < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        Some(TabClose { closed_webview_id: closed.webview_id, pane_empty: false })
    }

    /// Activate the tab at `idx`. Returns true when the active tab changed.
    pub fn select(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() || idx == self.active {
            return false;
        }
        self.active = idx;
        true
    }

    /// Activate the next tab, wrapping past the end.
    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// Activate the previous tab, wrapping past the start.
    pub fn prev(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// Which tab (index) owns `webview_id`, if any.
    pub fn index_of_webview(&self, webview_id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview_id == webview_id)
    }
}

/// Resolve text typed in the URL bar into a navigable URL.
///
/// Applied ONLY to URL-bar submissions — navigation events reported by the
/// webview are already real URLs and go through [`normalise_url`] instead.
///
/// - Explicit scheme (`http`, `https`, `about`, `alterm`) → unchanged.
/// - Single token with a dot in its host part, or `localhost[:port]` →
///   `https://` prefixed.
/// - Anything else → search via `search_engine` (a URL template whose `{}`
///   is replaced with the percent-encoded query; appended if no `{}`).
pub fn resolve_input(input: &str, search_engine: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return "about:blank".to_string();
    }
    let lower = t.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("about:")
        || lower.starts_with("alterm://")
    {
        return t.to_string();
    }
    let no_spaces = !t.contains(char::is_whitespace);
    let host = t.split('/').next().unwrap_or(t);
    let hostname = host.split(':').next().unwrap_or(host);
    if no_spaces && (hostname == "localhost" || hostname.contains('.')) {
        return format!("https://{t}");
    }
    let q = percent_encode(t);
    if search_engine.contains("{}") {
        search_engine.replacen("{}", &q, 1)
    } else {
        format!("{search_engine}{q}")
    }
}

/// Percent-encode a query string for use in a URL (RFC 3986 unreserved
/// characters pass through; everything else is `%XX`-escaped).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Ensure a URL has a scheme. Bare domains get `https://` prepended.
fn normalise_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "about:blank".to_string();
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("about:")
        || trimmed.starts_with("alterm://")
    {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_url_in_history() {
        let s = BrowserState::new("https://example.com");
        assert_eq!(s.url, "https://example.com");
        assert_eq!(s.history.len(), 1);
        assert!(!s.can_go_back);
        assert!(!s.can_go_forward);
    }

    #[test]
    fn navigate_returns_normalised_url_without_touching_history() {
        let mut s = BrowserState::new("https://a.com");
        let target = s.navigate("b.com");
        assert_eq!(target, "https://b.com");
        // History only changes once the navigation is confirmed.
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.input_url, "https://b.com");
    }

    #[test]
    fn on_navigation_records_fresh_navigations() {
        // Simulates URL-bar submits and/or in-page link clicks reported by
        // the webview's navigation handler.
        let mut s = BrowserState::new("https://a.com");
        s.on_navigation("https://b.com");
        assert_eq!(s.url, "https://b.com");
        assert_eq!(s.history.len(), 2);
        assert!(s.can_go_back);
        assert!(!s.can_go_forward);
    }

    #[test]
    fn on_navigation_ignores_duplicate_of_current_page() {
        let mut s = BrowserState::new("https://a.com");
        s.on_navigation("https://a.com");
        assert_eq!(s.history.len(), 1);
        assert!(!s.can_go_back);
    }

    #[test]
    fn back_and_forward_move_immediately() {
        // webkit doesn't fire the navigation handler for history.back()/forward(),
        // so begin_back/begin_forward must update our index themselves — no
        // on_navigation event arrives to confirm them.
        let mut s = BrowserState::new("https://a.com");
        s.on_navigation("https://b.com");
        s.on_navigation("https://c.com"); // [a,b,c] index=2

        assert!(s.begin_back());
        assert_eq!(s.url, "https://b.com");
        assert!(s.can_go_back);
        assert!(s.can_go_forward); // forward history preserved

        assert!(s.begin_forward());
        assert_eq!(s.url, "https://c.com");
        assert!(!s.can_go_forward);

        // Back to the very start: forward stays available the whole way.
        assert!(s.begin_back());
        assert!(s.begin_back());
        assert_eq!(s.url, "https://a.com");
        assert!(!s.can_go_back);
        assert!(s.can_go_forward);
        assert!(!s.begin_back()); // nowhere left to go
    }

    #[test]
    fn duplicate_nav_event_after_back_is_ignored() {
        // Some webkit builds *might* still report the back/forward navigation.
        // If so, it lands on the page we already moved to and must be a no-op.
        let mut s = BrowserState::new("https://a.com");
        s.on_navigation("https://b.com");
        s.on_navigation("https://c.com");
        assert!(s.begin_back()); // index=1, url=b
        s.on_navigation("https://b.com"); // late/duplicate report
        assert_eq!(s.url, "https://b.com");
        assert_eq!(s.history.len(), 3);
        assert!(s.can_go_forward);
    }

    #[test]
    fn begin_back_returns_false_at_start() {
        let mut s = BrowserState::new("https://a.com");
        assert!(!s.begin_back());
        assert_eq!(s.pending_move, 0);
    }

    #[test]
    fn fresh_navigation_from_middle_truncates_forward() {
        let mut s = BrowserState::new("https://a.com");
        s.on_navigation("https://b.com");
        s.on_navigation("https://c.com");
        assert!(s.begin_back()); // back at b.com
        s.on_navigation("https://d.com"); // fresh nav (e.g. link click)

        assert_eq!(s.history.len(), 3); // a, b, d
        assert_eq!(s.url, "https://d.com");
        assert!(!s.can_go_forward);
    }

    #[test]
    fn on_navigation_reports_freshness() {
        let mut s = BrowserState::new("https://a.com");
        assert!(s.on_navigation("https://b.com"));   // fresh
        assert!(!s.on_navigation("https://b.com"));  // duplicate of current
        assert!(s.begin_back());
        assert!(!s.on_navigation("https://a.com"));  // late back-report, not fresh
    }

    #[test]
    fn zoom_defaults_to_one() {
        let s = BrowserState::new("https://a.com");
        assert_eq!(s.zoom, 1.0);
    }

    #[test]
    fn set_loading_toggles() {
        let mut s = BrowserState::new("https://a.com");
        s.set_loading(true);
        assert!(s.loading);
        s.set_loading(false);
        assert!(!s.loading);
    }

    #[test]
    fn normalise_adds_scheme() {
        assert_eq!(normalise_url("google.com"), "https://google.com");
        assert_eq!(normalise_url("http://foo.bar"), "http://foo.bar");
        assert_eq!(normalise_url(""), "about:blank");
    }

    #[test]
    fn display_title_falls_back_to_url() {
        let s = BrowserState::new("https://example.com");
        assert_eq!(s.display_title(), "https://example.com");

        let mut s2 = BrowserState::new("https://example.com");
        s2.title = "Example Domain".to_string();
        assert_eq!(s2.display_title(), "Example Domain");
    }

    const DDG: &str = "https://duckduckgo.com/?q={}";

    #[test]
    fn resolve_input_passes_urls_through() {
        assert_eq!(resolve_input("https://a.com/x", DDG), "https://a.com/x");
        assert_eq!(resolve_input("http://a.com", DDG), "http://a.com");
        assert_eq!(resolve_input("about:blank", DDG), "about:blank");
        assert_eq!(resolve_input("alterm://history", DDG), "alterm://history");
    }

    #[test]
    fn resolve_input_prefixes_bare_hosts() {
        assert_eq!(resolve_input("google.com", DDG), "https://google.com");
        assert_eq!(resolve_input("docs.rs/serde/latest", DDG), "https://docs.rs/serde/latest");
        assert_eq!(resolve_input("localhost:3000/app", DDG), "https://localhost:3000/app");
    }

    #[test]
    fn resolve_input_searches_everything_else() {
        assert_eq!(
            resolve_input("rust lifetimes", DDG),
            "https://duckduckgo.com/?q=rust%20lifetimes"
        );
        assert_eq!(resolve_input("rust", DDG), "https://duckduckgo.com/?q=rust");
        // Spaces force a search even when a dot is present.
        assert_eq!(
            resolve_input("what is docs.rs", DDG),
            "https://duckduckgo.com/?q=what%20is%20docs.rs"
        );
        // Template without {} gets the query appended.
        assert_eq!(
            resolve_input("cats", "https://x.com/search?q="),
            "https://x.com/search?q=cats"
        );
        assert_eq!(resolve_input("", DDG), "about:blank");
    }

    #[test]
    fn percent_encode_escapes_reserved_bytes() {
        assert_eq!(percent_encode("a b&c=d?e#f"), "a%20b%26c%3Dd%3Fe%23f");
        assert_eq!(percent_encode("safe-._~AZaz09"), "safe-._~AZaz09");
    }

    #[test]
    fn normalise_url_passes_alterm_scheme() {
        assert_eq!(normalise_url("alterm://history"), "alterm://history");
    }

    // ── BrowserPaneState (in-pane tabs) ─────────────────────────────

    #[test]
    fn pane_state_starts_with_one_tab() {
        let p = BrowserPaneState::new("https://a.com");
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.active, 0);
        assert_eq!(p.active_state().url, "https://a.com");
        assert_eq!(p.active_webview_id(), p.tabs[0].webview_id);
    }

    #[test]
    fn webview_ids_are_unique() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id1 = p.tabs[0].webview_id;
        let id2 = p.open_at_end("https://b.com");
        let id3 = p.open_after_active("https://c.com");
        assert!(id1 != id2 && id2 != id3 && id1 != id3);
    }

    #[test]
    fn open_at_end_appends_and_activates() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id = p.open_at_end("https://b.com");
        assert_eq!(p.tabs.len(), 2);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_webview_id(), id);
        assert_eq!(p.active_state().url, "https://b.com");
    }

    #[test]
    fn open_after_active_inserts_next_to_active() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // [a, b], active=1
        p.select(0);                    // active=0
        let id = p.open_after_active("https://c.com"); // [a, c, b]
        assert_eq!(p.active, 1);
        assert_eq!(p.active_webview_id(), id);
        assert_eq!(p.tabs[2].state.url, "https://b.com");
    }

    #[test]
    fn close_left_of_active_shifts_active_down() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // [a, b, c], active=2
        let closed = p.close(0).unwrap();
        assert!(!closed.pane_empty);
        assert_eq!(p.tabs.len(), 2);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_state().url, "https://c.com");
    }

    #[test]
    fn close_active_activates_right_neighbor() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // [a, b, c]
        p.select(1);
        let closed = p.close(1).unwrap(); // [a, c]
        assert!(!closed.pane_empty);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_state().url, "https://c.com");
    }

    #[test]
    fn close_last_position_activates_new_last() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // [a, b], active=1
        p.close(1).unwrap();
        assert_eq!(p.active, 0);
        assert_eq!(p.active_state().url, "https://a.com");
    }

    #[test]
    fn close_final_tab_reports_pane_empty_without_removing() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id = p.tabs[0].webview_id;
        let closed = p.close(0).unwrap();
        assert!(closed.pane_empty);
        assert_eq!(closed.closed_webview_id, id);
        // Caller closes the whole pane; the tab list is left intact so the
        // close-pane path can destroy every remaining webview uniformly.
        assert_eq!(p.tabs.len(), 1);
    }

    #[test]
    fn close_out_of_range_is_none() {
        let mut p = BrowserPaneState::new("https://a.com");
        assert!(p.close(5).is_none());
    }

    #[test]
    fn select_reports_change_and_ignores_bad_index() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // active=1
        assert!(p.select(0));
        assert!(!p.select(0)); // already active
        assert!(!p.select(9)); // out of range
        assert_eq!(p.active, 0);
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // active=2
        p.next();
        assert_eq!(p.active, 0);
        p.prev();
        assert_eq!(p.active, 2);
        p.prev();
        assert_eq!(p.active, 1);
    }

    #[test]
    fn index_of_webview_resolves_ids() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id_b = p.open_at_end("https://b.com");
        assert_eq!(p.index_of_webview(id_b), Some(1));
        assert_eq!(p.index_of_webview(id_b + 999_999), None);
    }

    #[test]
    fn from_states_clamps_active_and_survives_empty() {
        let states = vec![BrowserState::new("https://a.com"), BrowserState::new("https://b.com")];
        let p = BrowserPaneState::from_states(states, 7);
        assert_eq!(p.active, 1); // clamped to last

        let empty = BrowserPaneState::from_states(Vec::new(), 0);
        assert_eq!(empty.tabs.len(), 1); // fallback tab, never zero tabs
        assert_eq!(empty.active, 0);
    }
}
