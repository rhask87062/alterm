/// Browser state management and embedded web view (wry/webkit2gtk).
///
/// This crate provides:
/// - `BrowserState`: tracks URL, navigation history, and loading status.
/// - `webview_manager`: manages real wry `WebView` instances on the main thread.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod webview_manager;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod webview_manager {
    /// Embedded browser is not supported on this platform.
    pub fn init_gtk() {}
    pub fn pump_gtk_events() {}
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
    /// This is the single place history is mutated, so every navigation —
    /// however it was triggered — keeps the stack and nav flags accurate.
    pub fn on_navigation(&mut self, url: &str) {
        let url = normalise_url(url);

        match self.pending_move {
            -1 => {
                // Confirmed Back: move the index, keep forward history intact.
                self.history_index = self.history_index.saturating_sub(1);
                self.pending_move = 0;
            }
            1 => {
                // Confirmed Forward: move the index, keep back history intact.
                if self.history_index + 1 < self.history.len() {
                    self.history_index += 1;
                }
                self.pending_move = 0;
            }
            _ => {
                // Fresh navigation. Ignore a duplicate of the current page
                // (e.g. the webview re-reporting the page we're already on).
                if url != self.url {
                    self.history.truncate(self.history_index + 1);
                    self.history.push(url.clone());
                    self.history_index = self.history.len() - 1;
                }
            }
        }

        self.url = url.clone();
        self.input_url = url;
        self.loading = false;
        self.update_nav_flags();

        log::debug!(
            "Browser on_navigation: index={} history_len={}",
            self.history_index,
            self.history.len()
        );
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

/// Ensure a URL has a scheme. Bare domains get `https://` prepended.
fn normalise_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "about:blank".to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("about:") {
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
}
