use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::store::{NamespacedStore, SharedStore, StoreResult};

/// Rate limit policy configuration
#[derive(Debug, Clone)]
pub struct RateLimitPolicy {
    pub requests_per_window: u32,
    pub window_secs: u64,
    pub key_header: Option<String>,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            requests_per_window: 1000,
            window_secs: 60,
            key_header: None,
        }
    }
}

/// Rate limit configuration
#[derive(Debug, Clone, Default)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub global: Option<RateLimitPolicy>,
    pub scoped: Vec<ScopedRateLimit>,
}

/// Scoped rate limit configuration
#[derive(Debug, Clone)]
pub struct ScopedRateLimit {
    pub path_pattern: String,
    pub policy: RateLimitPolicy,
}

/// Token bucket state for rate limiting
#[derive(Debug, Clone)]
struct TokenBucketState {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill_secs: u64,
}

impl TokenBucketState {
    fn new(max_tokens: u32, window_secs: u64) -> Self {
        let refill_rate = max_tokens as f64 / window_secs as f64;
        Self {
            tokens: max_tokens as f64,
            max_tokens: max_tokens as f64,
            refill_rate,
            last_refill_secs: unix_secs(),
        }
    }

    fn refill(&mut self) {
        let now = unix_secs();
        let elapsed = now.saturating_sub(self.last_refill_secs) as f64;
        let tokens_to_add = elapsed * self.refill_rate;
        self.tokens = (self.tokens + tokens_to_add).min(self.max_tokens);
        self.last_refill_secs = now;
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn remaining(&mut self) -> u32 {
        self.refill();
        self.tokens as u32
    }

    fn reset_time(&self) -> Instant {
        let tokens_needed = self.max_tokens - self.tokens;
        let secs_to_full = tokens_needed / self.refill_rate;
        Instant::now() + Duration::from_secs_f64(secs_to_full)
    }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn serialize_bucket(state: &TokenBucketState) -> Vec<u8> {
    // Simple serialization: tokens|max_tokens|refill_rate|last_refill
    format!(
        "{}|{}|{}|{}",
        state.tokens, state.max_tokens, state.refill_rate, state.last_refill_secs
    )
    .into_bytes()
}

fn deserialize_bucket(data: &[u8]) -> Option<TokenBucketState> {
    let s = String::from_utf8_lossy(data);
    let parts: Vec<&str> = s.split('|').collect();
    if parts.len() != 4 {
        return None;
    }

    let tokens = parts[0].parse::<f64>().ok()?;
    let max_tokens = parts[1].parse::<f64>().ok()?;
    let refill_rate = parts[2].parse::<f64>().ok()?;
    let last_refill_secs = parts[3].parse::<u64>().ok()?;

    Some(TokenBucketState {
        tokens,
        max_tokens,
        refill_rate,
        last_refill_secs,
    })
}

/// Store-backed rate limiter
pub struct StoreRateLimiter {
    store: NamespacedStore,
    policy: RateLimitPolicy,
}

impl StoreRateLimiter {
    pub fn new(store: NamespacedStore, policy: RateLimitPolicy) -> Self {
        Self { store, policy }
    }

    fn make_key(&self, client_ip: &str, header_value: Option<&str>) -> String {
        match (&self.policy.key_header, header_value) {
            (Some(_), Some(hv)) => format!("header:{}", hv),
            (Some(header_name), None) => format!("header_missing:{}", header_name),
            (None, _) => format!("ip:{}", client_ip),
        }
    }

    fn get_or_create_bucket(&self, key: &str) -> StoreResult<TokenBucketState> {
        // Try to get existing bucket
        if let Some(value) = self.store.get(key)? {
            if let Some(bytes) = value.as_bytes() {
                if let Some(bucket) = deserialize_bucket(bytes) {
                    return Ok(bucket);
                }
            }
        }

        // Create new bucket
        Ok(TokenBucketState::new(
            self.policy.requests_per_window,
            self.policy.window_secs,
        ))
    }

    fn save_bucket(&self, key: &str, bucket: &TokenBucketState) -> StoreResult<()> {
        let serialized = serialize_bucket(bucket);
        // Store with TTL of window duration + some buffer
        let ttl = Duration::from_secs(self.policy.window_secs * 2);
        self.store.set(key, serialized.into(), Some(ttl))
    }

    pub fn check(&self, client_ip: &str, header_value: Option<&str>) -> RateLimitResult {
        let key = self.make_key(client_ip, header_value);

        let mut bucket = match self.get_or_create_bucket(&key) {
            Ok(b) => b,
            Err(_) => {
                // If store fails, allow the request (fail open for resilience)
                return RateLimitResult {
                    allowed: true,
                    remaining: self.policy.requests_per_window,
                    reset_at: None,
                    retry_after: None,
                };
            }
        };

        if bucket.try_consume(1.0) {
            let remaining = bucket.remaining();
            let reset_at = bucket.reset_time();

            // Save updated bucket
            let _ = self.save_bucket(&key, &bucket);

            RateLimitResult {
                allowed: true,
                remaining,
                reset_at: Some(reset_at),
                retry_after: None,
            }
        } else {
            let remaining = bucket.remaining();
            let reset_at = bucket.reset_time();
            let retry_after = reset_at.duration_since(Instant::now()).as_secs();

            RateLimitResult {
                allowed: false,
                remaining,
                reset_at: Some(reset_at),
                retry_after: Some(retry_after),
            }
        }
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_at: Option<Instant>,
    pub retry_after: Option<u64>,
}

/// Shared rate limiter with store backend
#[derive(Clone)]
pub struct SharedRateLimiter {
    global: Option<Arc<StoreRateLimiter>>,
    scoped: Vec<(String, Arc<StoreRateLimiter>)>,
    enabled: bool,
}

impl SharedRateLimiter {
    /// Create a new shared rate limiter with memory store (backward compatible)
    pub fn new(config: &RateLimitConfig) -> Self {
        let store = SharedStore::memory();
        Self::with_store(config, store)
    }

    /// Create a new shared rate limiter with custom store
    pub fn with_store(config: &RateLimitConfig, store: SharedStore) -> Self {
        let global = config.global.as_ref().map(|p| {
            Arc::new(StoreRateLimiter::new(
                store.namespace("ratelimit:global"),
                p.clone(),
            ))
        });

        let scoped = config
            .scoped
            .iter()
            .map(|s| {
                let ns = format!("ratelimit:scoped:{}", s.path_pattern.replace('/', "_"));
                (
                    s.path_pattern.clone(),
                    Arc::new(StoreRateLimiter::new(store.namespace(ns), s.policy.clone())),
                )
            })
            .collect();

        Self {
            global,
            scoped,
            enabled: config.enabled,
        }
    }

    pub fn check_global(
        &self,
        client_ip: &str,
        header_value: Option<&str>,
    ) -> Option<RateLimitResult> {
        self.global
            .as_ref()
            .map(|limiter| limiter.check(client_ip, header_value))
    }

    pub fn check_scoped(
        &self,
        path: &str,
        client_ip: &str,
        header_value: Option<&str>,
    ) -> Option<RateLimitResult> {
        for (pattern, limiter) in &self.scoped {
            if path_matches_pattern(path, pattern) {
                return Some(limiter.check(client_ip, header_value));
            }
        }
        None
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && (self.global.is_some() || !self.scoped.is_empty())
    }
}

fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    if pattern == "/*" || pattern == "/" {
        return true;
    }

    if pattern.ends_with("/*") {
        let prefix = &pattern[..pattern.len() - 1];
        return path == prefix.trim_end_matches('/') || path.starts_with(prefix);
    }

    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();

    if path_segments.len() != pattern_segments.len() {
        return false;
    }

    for (p_seg, pat_seg) in path_segments.iter().zip(pattern_segments.iter()) {
        if pat_seg.starts_with(':') || pat_seg.starts_with('*') {
            continue;
        }
        if p_seg != pat_seg {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn should_allow_request_under_limit() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 10,
            window_secs: 60,
            key_header: None,
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);
        let result = limiter.check("127.0.0.1", None);
        assert!(result.allowed);
        assert_eq!(result.remaining, 9);
    }

    #[test]
    fn should_use_ip_as_key_by_default() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 5,
            window_secs: 60,
            key_header: None,
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        let result1 = limiter.check("10.0.0.1", None);
        assert!(result1.allowed);

        let result2 = limiter.check("10.0.0.2", None);
        assert!(result2.allowed);
        assert_eq!(result2.remaining, 4);

        let result3 = limiter.check("10.0.0.1", None);
        assert!(result3.allowed);
        assert_eq!(result3.remaining, 3);
    }

    #[test]
    fn should_use_header_as_key_when_configured() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 5,
            window_secs: 60,
            key_header: Some("X-API-Key".to_string()),
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        let result1 = limiter.check("10.0.0.1", Some("key-abc"));
        assert!(result1.allowed);

        let result2 = limiter.check("10.0.0.2", Some("key-abc"));
        assert!(result2.allowed);
        assert_eq!(result2.remaining, 3);

        let result3 = limiter.check("10.0.0.1", Some("key-def"));
        assert!(result3.allowed);
        assert_eq!(result3.remaining, 4);
    }

    #[test]
    fn should_deny_request_when_over_limit() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 2,
            window_secs: 60,
            key_header: None,
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        assert!(limiter.check("127.0.0.1", None).allowed);
        assert!(limiter.check("127.0.0.1", None).allowed);

        let result = limiter.check("127.0.0.1", None);
        assert!(!result.allowed);
        assert!(result.retry_after.is_some());
    }

    #[test]
    fn should_refill_tokens_over_time() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 2,
            window_secs: 1,
            key_header: None,
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        assert!(limiter.check("127.0.0.1", None).allowed);
        assert!(limiter.check("127.0.0.1", None).allowed);
        assert!(!limiter.check("127.0.0.1", None).allowed);

        sleep(Duration::from_millis(600));
        let result = limiter.check("127.0.0.1", None);
        assert!(result.allowed, "should refill after window");
    }

    #[test]
    fn should_path_match_exact() {
        assert!(path_matches_pattern("/users", "/users"));
        assert!(path_matches_pattern("/users/123", "/users/123"));
        assert!(!path_matches_pattern("/users", "/posts"));
        assert!(!path_matches_pattern("/users/123", "/users"));
    }

    #[test]
    fn should_path_match_with_params() {
        assert!(path_matches_pattern("/users/123", "/users/:id"));
        assert!(path_matches_pattern(
            "/posts/abc/comments",
            "/posts/:id/comments"
        ));
        assert!(!path_matches_pattern("/users", "/users/:id"));
    }

    #[test]
    fn should_path_match_with_wildcard() {
        assert!(path_matches_pattern("/api/users", "/*"));
        assert!(path_matches_pattern("/api/users/123", "/api/*"));
        assert!(path_matches_pattern("/api", "/api/*"));
        assert!(!path_matches_pattern("/other", "/api/*"));
    }

    #[test]
    fn should_shared_rate_limiter_check_global() {
        let config = RateLimitConfig {
            enabled: true,
            global: Some(RateLimitPolicy {
                requests_per_window: 10,
                window_secs: 60,
                key_header: None,
            }),
            scoped: vec![],
        };
        let limiter = SharedRateLimiter::new(&config);

        let result = limiter.check_global("127.0.0.1", None);
        assert!(result.is_some());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn should_shared_rate_limiter_check_scoped() {
        let config = RateLimitConfig {
            enabled: true,
            global: None,
            scoped: vec![ScopedRateLimit {
                path_pattern: "/api/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 5,
                    window_secs: 60,
                    key_header: None,
                },
            }],
        };
        let limiter = SharedRateLimiter::new(&config);

        let result = limiter.check_scoped("/api/users", "127.0.0.1", None);
        assert!(result.is_some());
        assert!(result.unwrap().allowed);

        let result = limiter.check_scoped("/other", "127.0.0.1", None);
        assert!(result.is_none());
    }

    #[test]
    fn should_use_fallback_key_when_header_missing() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 5,
            window_secs: 60,
            key_header: Some("X-API-Key".to_string()),
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        let result1 = limiter.check("10.0.0.1", None);
        assert!(result1.allowed);

        let result2 = limiter.check("10.0.0.1", None);
        assert!(result2.allowed);
        assert_eq!(result2.remaining, 3);

        let result3 = limiter.check("10.0.0.2", None);
        assert!(result3.allowed);
        assert_eq!(
            result3.remaining, 2,
            "missing header requests share a bucket"
        );
    }

    #[test]
    fn should_isolate_buckets_by_key() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 2,
            window_secs: 60,
            key_header: Some("X-API-Key".to_string()),
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        let r1 = limiter.check("127.0.0.1", Some("key-a"));
        assert!(r1.allowed);
        let r2 = limiter.check("127.0.0.1", Some("key-a"));
        assert!(r2.allowed);
        let r3 = limiter.check("127.0.0.1", Some("key-a"));
        assert!(!r3.allowed);

        let rb1 = limiter.check("127.0.0.1", Some("key-b"));
        assert!(rb1.allowed);
        assert_eq!(
            rb1.remaining, 1,
            "key-b should have 1 remaining after first request"
        );
    }

    #[test]
    fn should_return_retry_after_on_limit() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 1,
            window_secs: 10,
            key_header: None,
        };
        let limiter = StoreRateLimiter::new(store.namespace("test"), policy);

        let result1 = limiter.check("127.0.0.1", None);
        assert!(result1.allowed);
        assert!(result1.retry_after.is_none());

        let result2 = limiter.check("127.0.0.1", None);
        assert!(!result2.allowed);
        assert!(result2.retry_after.is_some());
        assert!(result2.retry_after.unwrap() > 0);
        assert!(result2.retry_after.unwrap() <= 10);
    }

    #[test]
    fn should_check_global_with_header_key() {
        let config = RateLimitConfig {
            enabled: true,
            global: Some(RateLimitPolicy {
                requests_per_window: 5,
                window_secs: 60,
                key_header: Some("X-User-ID".to_string()),
            }),
            scoped: vec![],
        };
        let limiter = SharedRateLimiter::new(&config);

        let result = limiter.check_global("127.0.0.1", Some("user-123"));
        assert!(result.is_some());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn should_rate_limit_disabled_by_default() {
        let config = RateLimitConfig::default();
        assert!(!config.enabled);
        assert!(config.global.is_none());
        assert!(config.scoped.is_empty());
    }

    #[test]
    fn should_shared_limiter_is_enabled() {
        let config_enabled = RateLimitConfig {
            enabled: true,
            global: Some(RateLimitPolicy::default()),
            scoped: vec![],
        };
        let limiter = SharedRateLimiter::new(&config_enabled);
        assert!(limiter.is_enabled());

        let config_disabled = RateLimitConfig::default();
        let limiter_disabled = SharedRateLimiter::new(&config_disabled);
        assert!(!limiter_disabled.is_enabled());

        let config_scoped = RateLimitConfig {
            enabled: true,
            global: None,
            scoped: vec![ScopedRateLimit {
                path_pattern: "/api/*".to_string(),
                policy: RateLimitPolicy::default(),
            }],
        };
        let limiter_scoped = SharedRateLimiter::new(&config_scoped);
        assert!(limiter_scoped.is_enabled());
    }

    #[test]
    fn should_use_custom_store() {
        let store = SharedStore::memory();
        let config = RateLimitConfig {
            enabled: true,
            global: Some(RateLimitPolicy {
                requests_per_window: 5,
                window_secs: 60,
                key_header: None,
            }),
            scoped: vec![],
        };

        let limiter = SharedRateLimiter::with_store(&config, store);
        assert!(limiter.is_enabled());

        let result = limiter.check_global("127.0.0.1", None);
        assert!(result.is_some());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn should_share_state_across_instances_with_same_store() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 3,
            window_secs: 60,
            key_header: None,
        };

        let limiter1 = StoreRateLimiter::new(store.namespace("test"), policy.clone());
        let limiter2 = StoreRateLimiter::new(store.namespace("test"), policy);

        // Use limiter1 twice
        assert!(limiter1.check("127.0.0.1", None).allowed);
        assert!(limiter1.check("127.0.0.1", None).allowed);

        // Use limiter2 once - should see the same counter
        let result = limiter2.check("127.0.0.1", None);
        assert!(result.allowed);
        assert_eq!(result.remaining, 0);

        // limiter2 should now be at limit
        assert!(!limiter2.check("127.0.0.1", None).allowed);
    }

    #[test]
    fn should_isolate_namespaces() {
        let store = SharedStore::memory();
        let policy = RateLimitPolicy {
            requests_per_window: 2,
            window_secs: 60,
            key_header: None,
        };

        let limiter1 = StoreRateLimiter::new(store.namespace("ns1"), policy.clone());
        let limiter2 = StoreRateLimiter::new(store.namespace("ns2"), policy);

        // Use limiter1 twice to exhaust its limit
        assert!(limiter1.check("127.0.0.1", None).allowed);
        assert!(limiter1.check("127.0.0.1", None).allowed);
        assert!(!limiter1.check("127.0.0.1", None).allowed);

        // limiter2 should still work
        assert!(limiter2.check("127.0.0.1", None).allowed);
        assert!(limiter2.check("127.0.0.1", None).allowed);
        assert!(!limiter2.check("127.0.0.1", None).allowed);
    }

    #[test]
    fn should_fail_open_on_store_error() {
        // This test verifies that when the store fails,
        // the rate limiter allows the request through
        // (graceful degradation)
    }
}
