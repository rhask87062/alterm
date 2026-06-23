//! Persisted per-provider model-list cache.
//!
//! Stores the last-known list of models for each AI provider on disk so the
//! model dropdown is populated instantly on launch / session restore and stays
//! usable when the provider is briefly unreachable. The caller supplies the
//! directory (so this module stays decoupled from the config crate) and the
//! current time (so staleness logic is deterministic in tests).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How long a cached model list is considered fresh (24 hours).
pub const MODEL_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

const CACHE_FILE: &str = "model-cache.json";

/// One provider's cached model list and when it was fetched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachedProvider {
    pub models: Vec<String>,
    /// Unix timestamp (seconds) the list was fetched.
    pub fetched_at: u64,
}

/// The whole cache: provider name -> cached list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCache {
    pub providers: HashMap<String, CachedProvider>,
}

/// Result of a cache lookup: the models to show now, and whether the caller
/// should kick off a background refresh.
pub struct CacheLookup {
    pub models: Vec<String>,
    pub needs_refresh: bool,
}

impl ModelCache {
    /// Look up a provider. Returns its cached models (empty if none) and whether
    /// a refresh is due (missing entry, or older than `ttl` seconds).
    pub fn lookup(&self, provider: &str, now: u64, ttl: u64) -> CacheLookup {
        match self.providers.get(provider) {
            Some(entry) => {
                let stale = now.saturating_sub(entry.fetched_at) > ttl;
                CacheLookup { models: entry.models.clone(), needs_refresh: stale }
            }
            None => CacheLookup { models: Vec::new(), needs_refresh: true },
        }
    }

    /// Insert or replace a provider's model list, stamped at `now`.
    pub fn put(&mut self, provider: &str, models: Vec<String>, now: u64) {
        self.providers.insert(
            provider.to_string(),
            CachedProvider { models, fetched_at: now },
        );
    }
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

/// Load the cache from `<dir>/model-cache.json`. A missing or corrupt file
/// yields an empty cache — never an error or panic.
pub fn load(dir: &Path) -> ModelCache {
    let path = cache_path(dir);
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return ModelCache::default(),
    };
    match serde_json::from_str(&contents) {
        Ok(cache) => cache,
        Err(e) => {
            log::warn!("Ignoring corrupt model cache at {path:?}: {e}");
            ModelCache::default()
        }
    }
}

/// Write the cache to `<dir>/model-cache.json`. Best-effort: logs on failure,
/// never panics.
pub fn save(dir: &Path, cache: &ModelCache) {
    let path = cache_path(dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to write model cache to {path:?}: {e}");
            }
        }
        Err(e) => log::warn!("Failed to serialize model cache: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alterm-modelcache-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lookup_missing_needs_refresh() {
        let cache = ModelCache::default();
        let r = cache.lookup("openai", 1000, MODEL_CACHE_TTL_SECS);
        assert!(r.models.is_empty());
        assert!(r.needs_refresh);
    }

    #[test]
    fn lookup_fresh_does_not_need_refresh() {
        let mut cache = ModelCache::default();
        cache.put("openai", vec!["gpt-4o".into()], 1000);
        let r = cache.lookup("openai", 1000 + 10, MODEL_CACHE_TTL_SECS);
        assert_eq!(r.models, vec!["gpt-4o".to_string()]);
        assert!(!r.needs_refresh);
    }

    #[test]
    fn lookup_stale_needs_refresh_but_returns_models() {
        let mut cache = ModelCache::default();
        cache.put("openai", vec!["gpt-4o".into()], 1000);
        let r = cache.lookup("openai", 1000 + MODEL_CACHE_TTL_SECS + 1, MODEL_CACHE_TTL_SECS);
        assert_eq!(r.models, vec!["gpt-4o".to_string()]);
        assert!(r.needs_refresh);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = temp_dir("missing");
        assert!(load(&dir).providers.is_empty());
    }

    #[test]
    fn load_corrupt_file_is_empty() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join("model-cache.json"), "{ not json").unwrap();
        assert!(load(&dir).providers.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let mut cache = ModelCache::default();
        cache.put("anthropic", vec!["claude-x".into(), "claude-y".into()], 42);
        save(&dir, &cache);
        let loaded = load(&dir);
        let entry = loaded.providers.get("anthropic").unwrap();
        assert_eq!(entry.models, vec!["claude-x".to_string(), "claude-y".to_string()]);
        assert_eq!(entry.fetched_at, 42);
    }
}
