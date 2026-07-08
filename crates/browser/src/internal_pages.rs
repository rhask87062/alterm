//! HTML generation for alterm's internal browser pages
//! (`alterm://history`, `alterm://bookmarks`).
//!
//! Pure functions from store data to HTML strings, plus a `respond` router
//! used by the webview custom-protocol handler. Every title and URL is
//! HTML-escaped: page titles are untrusted input.

use chrono::{Local, TimeZone};

use crate::history::{Bookmark, HistoryEntry};

/// Escape text for safe embedding in HTML content or attribute values.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Extract and percent-decode the `q` query parameter from a URI.
pub fn parse_query(uri: &str) -> String {
    let Some(qs) = uri.splitn(2, '?').nth(1) else { return String::new() };
    for pair in qs.split('&') {
        if let Some(v) = pair.strip_prefix("q=") {
            return percent_decode(v);
        }
    }
    String::new()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if let (Some(h), Some(l)) = (
                    bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                    bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
                ) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Shared dark-theme CSS + the IPC helper script for internal pages.
const PAGE_STYLE: &str = r#"<style>
:root { color-scheme: dark; }
body { background: #141419; color: #ccccd6; font: 14px/1.5 system-ui, sans-serif;
       margin: 0; padding: 24px 32px; }
a { color: #8ab4f8; text-decoration: none; overflow-wrap: anywhere; }
a:hover { text-decoration: underline; }
h1 { font-size: 20px; margin: 0 0 16px; color: #e8e8ee; }
h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .08em;
     color: #8888a0; margin: 24px 0 8px; }
.row { display: flex; align-items: baseline; gap: 12px; padding: 5px 8px;
       border-radius: 6px; }
.row:hover { background: #1e1e28; }
.row:hover .del { visibility: visible; }
.time { color: #666678; font-size: 12px; min-width: 48px; }
.url { color: #666678; font-size: 12px; overflow-wrap: anywhere; }
.del { visibility: hidden; margin-left: auto; cursor: pointer; color: #666678;
       background: none; border: none; font-size: 13px; }
.del:hover { color: #ff7b72; }
.toolbar { display: flex; gap: 12px; margin-bottom: 20px; align-items: center; }
input[type=text] { background: #1e1e28; color: #ccccd6; border: 1px solid #2e2e3a;
       border-radius: 6px; padding: 6px 10px; width: 320px; font-size: 13px; }
.btn { background: #1e1e28; color: #ccccd6; border: 1px solid #2e2e3a;
       border-radius: 6px; padding: 6px 12px; cursor: pointer; font-size: 13px; }
.btn:hover { background: #26262f; }
.empty { color: #666678; padding: 16px 8px; }
nav { margin-bottom: 20px; font-size: 13px; }
nav a { margin-right: 16px; color: #8888a0; }
</style>
<script>
function ipc(obj) { window.ipc.postMessage(JSON.stringify(obj)); }
function delEntry(url, ts) { ipc({cmd: 'history-delete', url: url, ts: ts}); }
function clearHistory() {
  if (confirm('Clear all browsing history?')) ipc({cmd: 'history-clear'});
}
function removeBookmark(url) { ipc({cmd: 'bookmark-remove', url: url}); }
</script>"#;

fn page_shell(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <title>{}</title>{PAGE_STYLE}</head><body>{body}</body></html>",
        html_escape(title)
    )
}

fn nav_links() -> &'static str {
    r#"<nav><a href="alterm://history">History</a><a href="alterm://bookmarks">Bookmarks</a></nav>"#
}

/// `HH:MM` in local time.
fn format_time(ts: u64) -> String {
    Local
        .timestamp_opt(ts as i64, 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Day heading for grouping: Today / Yesterday / `Weekday, D Month YYYY`.
fn format_day(ts: u64) -> String {
    let Some(then) = Local.timestamp_opt(ts as i64, 0).single() else {
        return "Unknown date".to_string();
    };
    let today = Local::now().date_naive();
    let date = then.date_naive();
    if date == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(date) {
        "Yesterday".to_string()
    } else {
        then.format("%A, %-d %B %Y").to_string()
    }
}

fn history_row(e: &HistoryEntry) -> String {
    let label = if e.title.is_empty() { &e.url } else { &e.title };
    // JSON-encode the url for the onclick JS string (quotes/backslashes).
    // SECURITY: the onclick attribute below is deliberately single-quoted.
    // The JSON encoding produces a double-quoted JS string literal, and
    // html_escape turns its quotes into entities that the browser decodes
    // only when parsing the attribute value. Do not change the attribute
    // quoting without re-checking this chain (same applies in
    // bookmarks_page's removeBookmark button).
    let js_url = serde_json::to_string(&e.url).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"<div class="row"><span class="time">{}</span><a href="{}">{}</a><span class="url">{}</span><button class="del" title="Remove from history" onclick='delEntry({}, {})'>&#10005;</button></div>"#,
        format_time(e.timestamp),
        html_escape(&e.url),
        html_escape(label),
        html_escape(&e.url),
        html_escape(&js_url),
        e.timestamp,
    )
}

/// The `alterm://history` page (also the start page): search box,
/// bookmarks strip, entries grouped by day, newest first.
pub fn history_page(entries: &[HistoryEntry], bookmarks: &[Bookmark], query: &str) -> String {
    let mut body = String::new();
    body.push_str(nav_links());
    body.push_str("<h1>History</h1>");
    body.push_str(&format!(
        r#"<div class="toolbar"><form action="alterm://history" method="get">
           <input type="text" name="q" placeholder="Search history..." value="{}" autofocus></form>
           <button class="btn" onclick="clearHistory()">Clear history</button></div>"#,
        html_escape(query)
    ));

    if !bookmarks.is_empty() && query.is_empty() {
        body.push_str("<h2>Bookmarks</h2>");
        for b in bookmarks {
            let label = if b.title.is_empty() { &b.url } else { &b.title };
            body.push_str(&format!(
                r#"<div class="row"><a href="{}">&#9733; {}</a></div>"#,
                html_escape(&b.url),
                html_escape(label),
            ));
        }
    }

    if entries.is_empty() {
        body.push_str(r#"<div class="empty">No history yet. Pages you visit will appear here.</div>"#);
    } else {
        let mut current_day = String::new();
        for e in entries {
            let day = format_day(e.timestamp);
            if day != current_day {
                body.push_str(&format!("<h2>{}</h2>", html_escape(&day)));
                current_day = day;
            }
            body.push_str(&history_row(e));
        }
    }
    page_shell("History", &body)
}

/// The `alterm://bookmarks` page.
pub fn bookmarks_page(bookmarks: &[Bookmark]) -> String {
    let mut body = String::new();
    body.push_str(nav_links());
    body.push_str("<h1>Bookmarks</h1>");
    if bookmarks.is_empty() {
        body.push_str(r#"<div class="empty">No bookmarks yet. Press the &#9734; button in the URL bar to bookmark a page.</div>"#);
    } else {
        for b in bookmarks {
            let label = if b.title.is_empty() { &b.url } else { &b.title };
            let js_url = serde_json::to_string(&b.url).unwrap_or_else(|_| "\"\"".into());
            body.push_str(&format!(
                r#"<div class="row"><a href="{}">&#9733; {}</a><span class="url">{}</span><button class="del" title="Remove bookmark" onclick='removeBookmark({})'>&#10005;</button></div>"#,
                html_escape(&b.url),
                html_escape(label),
                html_escape(&b.url),
                html_escape(&js_url),
            ));
        }
    }
    page_shell("Bookmarks", &body)
}

/// Error page for unknown `alterm://` paths.
pub fn error_page(path: &str) -> String {
    let body = format!(
        r#"{}<h1>Unknown internal page</h1><div class="empty">{} is not a known alterm page. Try <a href="alterm://history">alterm://history</a>.</div>"#,
        nav_links(),
        html_escape(path),
    );
    page_shell("Not found", &body)
}

/// Router used by the webview custom-protocol handler. Reads the
/// thread-local stores (empty view when uninitialised).
pub fn respond(uri: &str) -> (&'static str, String) {
    let path = uri.splitn(2, '?').next().unwrap_or(uri);
    let path = path.trim_end_matches('/');
    match path {
        "alterm://history" => {
            let query = parse_query(uri);
            let (entries, bookmarks) = crate::history::with_stores(|s| {
                (s.history.query(&query), s.bookmarks.list().to_vec())
            })
            .unwrap_or_default();
            ("text/html", history_page(&entries, &bookmarks, &query))
        }
        "alterm://bookmarks" => {
            let bookmarks = crate::history::with_stores(|s| s.bookmarks.list().to_vec())
                .unwrap_or_default();
            ("text/html", bookmarks_page(&bookmarks))
        }
        other => ("text/html", error_page(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, title: &str, ts: u64) -> HistoryEntry {
        HistoryEntry { url: url.into(), title: title.into(), timestamp: ts }
    }

    #[test]
    fn html_escape_neutralises_hostile_input() {
        assert_eq!(
            html_escape(r#"<script>alert("x")</script>&'"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&amp;&#39;"
        );
    }

    #[test]
    fn history_page_escapes_titles_and_urls() {
        let hostile = entry(
            "https://evil.com/?a=<b>&c=\"d\"",
            "<img src=x onerror=alert(1)>",
            1,
        );
        let html = history_page(&[hostile], &[], "");
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(!html.contains(r#"?a=<b>"#));
    }

    #[test]
    fn onclick_url_argument_cannot_break_out_of_attribute() {
        // A URL crafted to escape the single-quoted onclick attribute or the
        // JS string must arrive fully entity-encoded.
        let hostile = entry("https://x.com/'); alert(1);//\"<>", "t", 1);
        let html = history_page(&[hostile], &[], "");
        // Raw single quote, double quote, or angle brackets from the URL
        // must never appear inside the emitted markup unescaped.
        assert!(!html.contains("'); alert(1)"));
        assert!(html.contains("&#39;); alert(1);//&quot;&lt;&gt;"));
    }

    #[test]
    fn history_page_lists_entries_and_reflects_query() {
        let entries = vec![
            entry("https://a.com", "Site A", 1_700_000_000),
            entry("https://b.com", "", 1_700_000_100),
        ];
        let html = history_page(&entries, &[], "rust");
        assert!(html.contains("Site A"));
        assert!(html.contains("https://b.com")); // untitled entries show URL
        assert!(html.contains(r#"value="rust""#)); // search box keeps query
        assert!(html.contains(r#"href="https://a.com""#));
    }

    #[test]
    fn history_page_shows_bookmarks_section_and_empty_state() {
        let bm = vec![Bookmark { url: "https://b.com".into(), title: "B".into() }];
        let html = history_page(&[], &bm, "");
        assert!(html.contains("Bookmarks"));
        assert!(html.contains("No history yet"));
    }

    #[test]
    fn bookmarks_page_lists_and_escapes() {
        let bm = vec![Bookmark {
            url: "https://b.com".into(),
            title: "<b>B</b>".into(),
        }];
        let html = bookmarks_page(&bm);
        assert!(html.contains("&lt;b&gt;B&lt;/b&gt;"));
        assert!(html.contains(r#"href="https://b.com""#));
    }

    #[test]
    fn parse_query_decodes_q_param() {
        assert_eq!(parse_query("alterm://history?q=rust%20lifetimes"), "rust lifetimes");
        assert_eq!(parse_query("alterm://history?q=a%26b"), "a&b");
        assert_eq!(parse_query("alterm://history"), "");
    }

    #[test]
    fn respond_routes_known_and_unknown_paths() {
        // Stores are uninitialised on the test thread: pages render empty.
        let (mime, body) = respond("alterm://history");
        assert_eq!(mime, "text/html");
        assert!(body.contains("History"));
        let (_, body) = respond("alterm://bookmarks");
        assert!(body.contains("Bookmarks"));
        let (_, body) = respond("alterm://nonsense");
        assert!(body.contains("Unknown internal page"));
    }
}
