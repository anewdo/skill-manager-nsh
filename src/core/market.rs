use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::error::{AppError, Result};
use super::model::{CacheClearReport, ProxySettings};
use super::sources::skills_sh::{LeaderboardKind, MarketplaceSkill, SkillsShClient};

/// Marketplace data is always fetched online; this cache only absorbs repeated
/// requests for the same list so switching views does not hit skills.sh again.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Recently fetched marketplace lists, kept for one process only.
pub struct MarketCache {
    ttl: Duration,
    entries: Mutex<HashMap<String, CachedSkills>>,
    fetch: Mutex<()>,
}

struct CachedSkills {
    stored_at: Instant,
    skills: Vec<MarketplaceSkill>,
}

impl Default for MarketCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketCache {
    pub fn new() -> Self {
        Self::with_ttl(CACHE_TTL)
    }

    fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
            fetch: Mutex::new(()),
        }
    }

    /// Returns the cached list younger than the TTL, or fetches it once and
    /// caches the result. A failed fetch is reported to the caller as is.
    fn load(
        &self,
        key: &str,
        force_refresh: bool,
        fetch: impl FnOnce() -> Result<Vec<MarketplaceSkill>>,
    ) -> Result<Vec<MarketplaceSkill>> {
        if !force_refresh && let Some(skills) = self.fresh(key) {
            return Ok(skills);
        }

        // One fetch at a time: a second request for the same list waits for the
        // first and then reuses its result instead of fetching again.
        let _fetch = lock(&self.fetch);
        if !force_refresh && let Some(skills) = self.fresh(key) {
            return Ok(skills);
        }

        let skills = fetch()?;
        let mut entries = lock(&self.entries);
        entries.retain(|_, cached| cached.stored_at.elapsed() < self.ttl);
        entries.insert(
            key.to_owned(),
            CachedSkills {
                stored_at: Instant::now(),
                skills: skills.clone(),
            },
        );
        Ok(skills)
    }

    fn fresh(&self, key: &str) -> Option<Vec<MarketplaceSkill>> {
        let entries = lock(&self.entries);
        let cached = entries.get(key)?;
        (cached.stored_at.elapsed() < self.ttl).then(|| cached.skills.clone())
    }

    fn clear(&self) -> u64 {
        let mut entries = lock(&self.entries);
        let cleared = entries.len() as u64;
        entries.clear();
        cleared
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub fn clear_cache(cache: &MarketCache) -> CacheClearReport {
    CacheClearReport {
        cleared_entries: cache.clear(),
    }
}

pub fn leaderboard(
    cache: &MarketCache,
    proxy: &ProxySettings,
    kind: LeaderboardKind,
    force_refresh: bool,
) -> Result<Vec<MarketplaceSkill>> {
    cache.load(
        &format!("leaderboard:{}", kind.as_str()),
        force_refresh,
        || SkillsShClient::new(proxy)?.fetch_leaderboard(kind),
    )
}

pub fn search(
    cache: &MarketCache,
    proxy: &ProxySettings,
    query: &str,
    limit: usize,
    force_refresh: bool,
) -> Result<Vec<MarketplaceSkill>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::invalid_input("search query is required"));
    }
    if !(1..=300).contains(&limit) {
        return Err(AppError::invalid_input(
            "search limit must be between 1 and 300",
        ));
    }

    let key = format!("search:{}:{limit}", query.to_lowercase());
    cache.load(&key, force_refresh, || {
        SkillsShClient::new(proxy)?.search(query, limit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;
    use crate::core::sources::skills_sh::marketplace_source;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn skill(name: &str) -> MarketplaceSkill {
        MarketplaceSkill {
            id: format!("github:owner/repo/{name}"),
            skill_id: name.to_string(),
            name: name.to_string(),
            source: "owner/repo".to_string(),
            installs: 1,
            lineage: marketplace_source("owner/repo", name).unwrap(),
        }
    }

    fn proxy() -> ProxySettings {
        ProxySettings::default()
    }

    #[test]
    fn repeated_requests_within_the_ttl_fetch_once() {
        let cache = MarketCache::new();
        let calls = AtomicUsize::new(0);
        let first = cache
            .load("leaderboard:all_time", false, || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![skill("cached")])
            })
            .unwrap();
        let second = cache
            .load("leaderboard:all_time", false, || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![skill("other")])
            })
            .unwrap();

        assert_eq!(first, vec![skill("cached")]);
        assert_eq!(second, vec![skill("cached")]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn expired_entries_are_fetched_again() {
        let cache = MarketCache::with_ttl(Duration::ZERO);
        let mut fetched = vec![skill("first")];

        let first = cache.load("key", false, || Ok(fetched.clone())).unwrap();
        assert_eq!(first, vec![skill("first")]);
        assert!(cache.fresh("key").is_none());

        fetched = vec![skill("second")];
        let second = cache.load("key", false, || Ok(fetched.clone())).unwrap();
        assert_eq!(second, vec![skill("second")]);
    }

    #[test]
    fn force_refresh_bypasses_the_cached_list() {
        let cache = MarketCache::new();
        cache
            .load("key", false, || Ok(vec![skill("cached")]))
            .unwrap();

        let refreshed = cache
            .load("key", true, || Ok(vec![skill("refreshed")]))
            .unwrap();

        assert_eq!(refreshed, vec![skill("refreshed")]);
        assert_eq!(cache.fresh("key").unwrap(), vec![skill("refreshed")]);
    }

    #[test]
    fn network_failures_are_reported_even_when_a_list_was_cached() {
        let cache = MarketCache::new();
        cache
            .load("key", false, || Ok(vec![skill("cached")]))
            .unwrap();

        let error = cache
            .load("key", true, || {
                Err(AppError::new(ErrorCode::Network, "offline").retryable(true))
            })
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::Network);
        assert!(error.retryable);
    }

    #[test]
    fn clearing_reports_and_removes_every_cached_list() {
        let cache = MarketCache::new();
        cache.load("one", false, || Ok(vec![skill("one")])).unwrap();
        cache.load("two", false, || Ok(vec![skill("two")])).unwrap();

        assert_eq!(clear_cache(&cache).cleared_entries, 2);
        assert_eq!(clear_cache(&cache).cleared_entries, 0);
        assert!(cache.fresh("one").is_none());
    }

    #[test]
    fn search_validates_input_and_normalizes_its_cache_key() {
        let cache = MarketCache::new();
        assert!(search(&cache, &proxy(), " ", 60, false).is_err());
        assert!(search(&cache, &proxy(), "react", 0, false).is_err());
        assert!(search(&cache, &proxy(), "react", 301, false).is_err());

        cache
            .load("search:react:60", false, || Ok(vec![skill("cached")]))
            .unwrap();
        let skills = search(&cache, &proxy(), " React ", 60, false).unwrap();
        assert_eq!(skills, vec![skill("cached")]);
    }
}
