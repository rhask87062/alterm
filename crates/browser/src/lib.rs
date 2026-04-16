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
        }
    }

    /// Navigate to a new URL, pushing it onto the history stack.
    ///
    /// Any forward history beyond the current position is discarded
    /// (standard browser behaviour when you navigate from the middle
    /// of the back/forward list).
    pub fn navigate(&mut self, url: &str) {
        let url = normalise_url(url);

        // Don't push a duplicate of the current page.
        if url == self.url {
            self.input_url = url;
            return;
        }

        // Truncate any forward history.
        self.history.truncate(self.history_index + 1);

        // Push the new URL.
        self.history.push(url.clone());
        self.history_index = self.history.len() - 1;

        self.url = url.clone();
        self.input_url = url;
        self.loading = true;
        self.update_nav_flags();

        log::debug!(
            "Browser navigate: index={} history_len={}",
            self.history_index,
            self.history.len()
        );
    }

    /// Go back one page in the history. No-op if already at the beginning.
    pub fn go_back(&mut self) {
        if !self.can_go_back {
            return;
        }
        self.history_index -= 1;
        let url = self.history[self.history_index].clone();
        self.url = url.clone();
        self.input_url = url;
        self.loading = true;
        self.update_nav_flags();
    }

    /// Go forward one page in the history. No-op if already at the end.
    pub fn go_forward(&mut self) {
        if !self.can_go_forward {
            return;
        }
        self.history_index += 1;
        let url = self.history[self.history_index].clone();
        self.url = url.clone();
        self.input_url = url;
        self.loading = true;
        self.update_nav_flags();
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
    fn navigate_adds_to_history() {
        let mut s = BrowserState::new("https://a.com");
        s.navigate("https://b.com");
        assert_eq!(s.url, "https://b.com");
        assert_eq!(s.history.len(), 2);
        assert!(s.can_go_back);
        assert!(!s.can_go_forward);
    }

    #[test]
    fn back_and_forward() {
        let mut s = BrowserState::new("https://a.com");
        s.navigate("https://b.com");
        s.navigate("https://c.com");

        s.go_back();
        assert_eq!(s.url, "https://b.com");
        assert!(s.can_go_back);
        assert!(s.can_go_forward);

        s.go_forward();
        assert_eq!(s.url, "https://c.com");
        assert!(!s.can_go_forward);
    }

    #[test]
    fn navigate_from_middle_truncates_forward() {
        let mut s = BrowserState::new("https://a.com");
        s.navigate("https://b.com");
        s.navigate("https://c.com");
        s.go_back(); // at b.com
        s.navigate("https://d.com");

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
