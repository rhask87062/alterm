//! Global browser history + bookmarks, persisted under the alterm data dir.
//!
//! `history.jsonl` is append-only: every mutation appends a `Record` line
//! (visit, title patch, delete, clear). The file is only rewritten during
//! load-time compaction. `bookmarks.json` is small and rewritten wholesale.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Compact when the on-disk record count exceeds this.
pub const COMPACT_THRESHOLD: usize = 100_000;
/// After compaction, keep this many newest entries.
pub const COMPACT_KEEP: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    /// Unix timestamp, seconds.
    pub timestamp: u64,
}

/// One line of `history.jsonl`. Mutations are appended, never edited in
/// place; `load` replays the log. Compaction squashes to plain visits.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    Visit { url: String, title: String, timestamp: u64 },
    Title { url: String, title: String },
    Delete { url: String, timestamp: u64 },
    Clear,
}

pub struct HistoryStore {
    path: PathBuf,
    /// Oldest-first.
    entries: Vec<HistoryEntry>,
    /// Records seen on disk at load time (drives compaction).
    disk_records: usize,
}

impl HistoryStore {
    pub fn load(path: PathBuf) -> Self {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        let mut disk_records = 0usize;
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                disk_records += 1;
                match serde_json::from_str::<Record>(line) {
                    Ok(Record::Visit { url, title, timestamp }) => {
                        entries.push(HistoryEntry { url, title, timestamp });
                    }
                    Ok(Record::Title { url, title }) => {
                        if let Some(e) = entries.iter_mut().rev().find(|e| e.url == url) {
                            e.title = title;
                        }
                    }
                    Ok(Record::Delete { url, timestamp }) => {
                        entries.retain(|e| !(e.url == url && e.timestamp == timestamp));
                    }
                    Ok(Record::Clear) => entries.clear(),
                    Err(e) => log::warn!("history: skipping corrupt line: {e}"),
                }
            }
        }
        let mut store = HistoryStore { path, entries, disk_records };
        if store.disk_records > COMPACT_THRESHOLD {
            store.compact();
        }
        store
    }

    /// Rewrite the file as plain visits, keeping the newest COMPACT_KEEP.
    fn compact(&mut self) {
        let excess = self.entries.len().saturating_sub(COMPACT_KEEP);
        self.entries.drain(..excess);
        let mut out = String::new();
        for e in &self.entries {
            let rec = Record::Visit {
                url: e.url.clone(), title: e.title.clone(), timestamp: e.timestamp,
            };
            if let Ok(line) = serde_json::to_string(&rec) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        if let Err(e) = std::fs::write(&self.path, out) {
            log::warn!("history: compaction write failed: {e}");
        }
        self.disk_records = self.entries.len();
    }

    fn append(&mut self, rec: &Record) {
        let line = match serde_json::to_string(rec) {
            Ok(l) => l,
            Err(e) => { log::warn!("history: serialize failed: {e}"); return; }
        };
        let res = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| writeln!(f, "{line}"));
        match res {
            Ok(()) => self.disk_records += 1,
            Err(e) => log::warn!("history: append failed (recording skipped): {e}"),
        }
    }

    pub fn record_visit(&mut self, url: &str, timestamp: u64) {
        self.append(&Record::Visit {
            url: url.to_string(), title: String::new(), timestamp,
        });
        self.entries.push(HistoryEntry {
            url: url.to_string(), title: String::new(), timestamp,
        });
    }

    /// Retro-fill the title on the most recent entry for `url`.
    ///
    /// Empty titles are ignored: webkit emits title-changed events with an
    /// empty string during page transitions, and those must not clobber a
    /// previously captured real title. Titles are only ever improved, never
    /// cleared, through this path.
    pub fn set_title(&mut self, url: &str, title: &str) {
        let Some(e) = self.entries.iter_mut().rev().find(|e| e.url == url) else {
            return;
        };
        if e.title == title || title.is_empty() {
            return;
        }
        e.title = title.to_string();
        self.append(&Record::Title { url: url.to_string(), title: title.to_string() });
    }

    /// Entries matching `filter` (case-insensitive substring of URL or
    /// title), newest first. Empty filter returns everything.
    pub fn query(&self, filter: &str) -> Vec<HistoryEntry> {
        let needle = filter.to_lowercase();
        self.entries
            .iter()
            .rev()
            .filter(|e| {
                needle.is_empty()
                    || e.url.to_lowercase().contains(&needle)
                    || e.title.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }

    pub fn delete(&mut self, url: &str, timestamp: u64) {
        self.entries.retain(|e| !(e.url == url && e.timestamp == timestamp));
        self.append(&Record::Delete { url: url.to_string(), timestamp });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.append(&Record::Clear);
    }

    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

pub struct BookmarkStore {
    path: PathBuf,
    bookmarks: Vec<Bookmark>,
}

impl BookmarkStore {
    pub fn load(path: PathBuf) -> Self {
        let bookmarks = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        BookmarkStore { path, bookmarks }
    }

    fn save(&self) {
        match serde_json::to_string_pretty(&self.bookmarks) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, json) {
                    log::warn!("bookmarks: write failed: {e}");
                }
            }
            Err(e) => log::warn!("bookmarks: serialize failed: {e}"),
        }
    }

    /// Toggle a bookmark; returns `true` if the URL is now bookmarked.
    pub fn toggle(&mut self, url: &str, title: &str) -> bool {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
            self.save();
            false
        } else {
            self.bookmarks.push(Bookmark {
                url: url.to_string(), title: title.to_string(),
            });
            self.save();
            true
        }
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    pub fn list(&self) -> &[Bookmark] { &self.bookmarks }
}

pub struct Stores {
    pub history: HistoryStore,
    pub bookmarks: BookmarkStore,
}

thread_local! {
    static STORES: RefCell<Option<Stores>> = const { RefCell::new(None) };
}

/// Load the stores from `data_dir` into main-thread state. Call once at
/// startup, before any webview is created.
pub fn init(data_dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        log::warn!("history: cannot create data dir {data_dir:?}: {e}");
    }
    let stores = Stores {
        history: HistoryStore::load(data_dir.join("history.jsonl")),
        bookmarks: BookmarkStore::load(data_dir.join("bookmarks.json")),
    };
    STORES.with(|s| *s.borrow_mut() = Some(stores));
}

/// Run `f` against the stores. `None` when `init` hasn't been called
/// (recording is skipped; browsing keeps working).
pub fn with_stores<R>(f: impl FnOnce(&mut Stores) -> R) -> Option<R> {
    STORES.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Current unix time in seconds.
pub fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn history_in(dir: &TempDir) -> HistoryStore {
        HistoryStore::load(dir.path().join("history.jsonl"))
    }

    #[test]
    fn record_and_reload_visits() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        drop(s);
        let s = history_in(&dir);
        let all = s.query("");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].url, "https://b.com"); // newest first
        assert_eq!(all[1].url, "https://a.com");
    }

    #[test]
    fn title_patch_applies_to_most_recent_matching_entry_and_persists() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        s.record_visit("https://a.com", 300);
        s.set_title("https://a.com", "A!");
        let all = s.query("");
        assert_eq!(all[0].title, "A!");       // ts=300 entry patched
        assert_eq!(all[2].title, "");          // ts=100 entry untouched
        drop(s);
        let s = history_in(&dir);
        assert_eq!(s.query("")[0].title, "A!"); // patch survives reload
    }

    #[test]
    fn query_filters_case_insensitively_on_url_and_title() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://docs.rs/serde", 100);
        s.record_visit("https://example.com", 200);
        s.set_title("https://example.com", "Serde Guide");
        assert_eq!(s.query("SERDE").len(), 2); // matches url of one, title of other
        assert_eq!(s.query("docs.rs").len(), 1);
        assert_eq!(s.query("zzz").len(), 0);
    }

    #[test]
    fn delete_and_clear_persist() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        s.delete("https://a.com", 100);
        assert_eq!(s.query("").len(), 1);
        drop(s);
        let mut s = history_in(&dir);
        assert_eq!(s.query("").len(), 1);
        s.clear();
        drop(s);
        let s = history_in(&dir);
        assert!(s.is_empty());
    }

    #[test]
    fn corrupt_lines_are_skipped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path,
            "{\"type\":\"visit\",\"url\":\"https://a.com\",\"title\":\"\",\"timestamp\":1}\nnot json\n").unwrap();
        let s = HistoryStore::load(path);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn load_compacts_oversized_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");
        {
            let mut s = HistoryStore::load(path.clone());
            for i in 0..(COMPACT_THRESHOLD as u64 + 10) {
                s.record_visit(&format!("https://x.com/{i}"), i);
            }
        }
        let s = HistoryStore::load(path.clone());
        assert_eq!(s.len(), COMPACT_KEEP);
        assert_eq!(s.query("")[0].url,
            format!("https://x.com/{}", COMPACT_THRESHOLD as u64 + 9)); // newest kept
        // File was rewritten: reloading again keeps the same count.
        let s2 = HistoryStore::load(path);
        assert_eq!(s2.len(), COMPACT_KEEP);
    }

    #[test]
    fn bookmarks_toggle_and_persist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bookmarks.json");
        let mut b = BookmarkStore::load(path.clone());
        assert!(b.toggle("https://a.com", "A"));      // now bookmarked
        assert!(b.is_bookmarked("https://a.com"));
        assert!(!b.toggle("https://a.com", "A"));     // now removed
        assert!(b.toggle("https://b.com", "B"));
        drop(b);
        let b = BookmarkStore::load(path);
        assert!(!b.is_bookmarked("https://a.com"));
        assert!(b.is_bookmarked("https://b.com"));
        assert_eq!(b.list().len(), 1);
    }

    #[test]
    fn with_stores_returns_none_before_init() {
        // Runs on the test thread where init() was never called.
        assert!(with_stores(|_| ()).is_none());
    }
}
