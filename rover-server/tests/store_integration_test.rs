use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rover_server::rate_limiter::{
    RateLimitConfig, RateLimitPolicy, ScopedRateLimit, SharedRateLimiter,
};
use rover_server::store::{MemoryStore, NamespacedStore, SharedStore, StoreBackend, StoreValue};

/// Test multi-instance consistency with shared store
#[test]
fn should_share_rate_limit_state_across_instances() {
    let store = SharedStore::memory();
    let policy = RateLimitPolicy {
        requests_per_window: 5,
        window_secs: 60,
        key_header: None,
    };

    let config = RateLimitConfig {
        enabled: true,
        global: Some(policy),
        scoped: vec![],
    };

    // Create two rate limiter instances sharing the same store
    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Use limiter1 to consume some quota
    for _ in 0..3 {
        let result = limiter1.check_global("10.0.0.1", None).unwrap();
        assert!(result.allowed);
    }

    // limiter2 should see the same state
    let result = limiter2.check_global("10.0.0.1", None).unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 1);

    // Exhaust the limit
    let result = limiter2.check_global("10.0.0.1", None).unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    // limiter1 should now reject
    let result = limiter1.check_global("10.0.0.1", None).unwrap();
    assert!(!result.allowed);
}

/// Test namespace isolation between different rate limiters
#[test]
fn should_isolate_rate_limit_namespaces() {
    let store = SharedStore::memory();

    let global_config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 3,
            window_secs: 60,
            key_header: None,
        }),
        scoped: vec![],
    };

    let scoped_config = RateLimitConfig {
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

    let global_limiter = SharedRateLimiter::with_store(&global_config, store.clone());
    let scoped_limiter = SharedRateLimiter::with_store(&scoped_config, store);

    // Exhaust global limiter
    for _ in 0..3 {
        assert!(
            global_limiter
                .check_global("127.0.0.1", None)
                .unwrap()
                .allowed
        );
    }
    assert!(
        !global_limiter
            .check_global("127.0.0.1", None)
            .unwrap()
            .allowed
    );

    // Scoped limiter should still work
    for _ in 0..5 {
        assert!(
            scoped_limiter
                .check_scoped("/api/users", "127.0.0.1", None)
                .unwrap()
                .allowed
        );
    }
    assert!(
        !scoped_limiter
            .check_scoped("/api/users", "127.0.0.1", None)
            .unwrap()
            .allowed
    );
}

/// Test TTL expiry in store affects rate limiting
#[test]
fn should_reset_rate_limit_after_ttl_expiry() {
    let store = SharedStore::memory();
    let policy = RateLimitPolicy {
        requests_per_window: 2,
        window_secs: 1, // Short window for testing
        key_header: None,
    };

    let config = RateLimitConfig {
        enabled: true,
        global: Some(policy),
        scoped: vec![],
    };

    let limiter = SharedRateLimiter::with_store(&config, store);

    // Exhaust the limit
    assert!(limiter.check_global("127.0.0.1", None).unwrap().allowed);
    assert!(limiter.check_global("127.0.0.1", None).unwrap().allowed);
    assert!(!limiter.check_global("127.0.0.1", None).unwrap().allowed);

    // Wait for TTL to expire
    thread::sleep(Duration::from_millis(1200));

    // Should be able to make requests again
    let result = limiter.check_global("127.0.0.1", None).unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 1);
}

/// Test concurrent access to shared store
#[test]
fn should_handle_concurrent_store_access() {
    let store = Arc::new(MemoryStore::new());
    let mut handles = vec![];

    // Spawn multiple threads incrementing the same counter
    for thread_id in 0..10 {
        let store_clone = store.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let key = format!("thread_{}_counter", thread_id);
                store_clone
                    .increment(&key, 1)
                    .expect("increment should succeed");

                // Also write to a shared key
                store_clone
                    .increment("shared_counter", 1)
                    .expect("shared increment should succeed");

                // Occasionally read
                if i % 10 == 0 {
                    let _ = store_clone.get(&key);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("thread should complete");
    }

    // Verify each thread's counter
    for thread_id in 0..10 {
        let key = format!("thread_{}_counter", thread_id);
        let value = store.get(&key).unwrap().unwrap();
        assert_eq!(value.as_integer(), Some(100));
    }

    // Verify shared counter
    let shared = store.get("shared_counter").unwrap().unwrap();
    assert_eq!(shared.as_integer(), Some(1000)); // 10 threads * 100 increments
}

/// Test subnamespace creation and isolation
#[test]
fn should_support_subnamespaces() {
    let backend = Arc::new(MemoryStore::new());
    let ns = NamespacedStore::new(backend.clone(), "app", false);

    // Create sub-namespaces
    let session_ns = ns.with_subnamespace("session");
    let cache_ns = ns.with_subnamespace("cache");

    // Write to both
    session_ns
        .set("user_1", "session_data".into(), None)
        .unwrap();
    cache_ns.set("key_1", "cached_value".into(), None).unwrap();

    // Verify isolation
    assert!(session_ns.exists("user_1").unwrap());
    assert!(!cache_ns.exists("user_1").unwrap());

    assert!(cache_ns.exists("key_1").unwrap());
    assert!(!session_ns.exists("key_1").unwrap());

    // Verify full key structure
    assert!(backend.exists("app:session:user_1").unwrap());
    assert!(backend.exists("app:cache:key_1").unwrap());
}

/// Test TTL behavior with cleanup
#[test]
fn should_expire_keys_with_ttl() {
    let store = MemoryStore::new();

    // Set keys with different TTLs (use longer durations for test reliability)
    store
        .set("short", "value1".into(), Some(Duration::from_millis(100)))
        .unwrap();
    store
        .set("medium", "value2".into(), Some(Duration::from_millis(500)))
        .unwrap();
    store.set("long", "value3".into(), None).unwrap();

    assert!(store.exists("short").unwrap());
    assert!(store.exists("medium").unwrap());
    assert!(store.exists("long").unwrap());

    // Wait for short TTL to expire
    thread::sleep(Duration::from_millis(200));
    store.cleanup_expired();

    assert!(!store.exists("short").unwrap());
    assert!(store.exists("medium").unwrap());
    assert!(store.exists("long").unwrap());

    // Wait for medium TTL to expire
    thread::sleep(Duration::from_millis(500));
    store.cleanup_expired();

    assert!(!store.exists("medium").unwrap());
    assert!(store.exists("long").unwrap());
}

/// Test store value types
#[test]
fn should_handle_all_value_types() {
    let store = MemoryStore::new();

    // String
    store.set("str_key", "hello world".into(), None).unwrap();
    let val = store.get("str_key").unwrap().unwrap();
    assert_eq!(val.as_string(), Some("hello world"));
    assert!(val.as_bytes().is_none());
    assert!(val.as_integer().is_none());
    assert!(val.as_bool().is_none());

    // Bytes
    store
        .set("bytes_key", vec![0u8, 1, 2, 3].into(), None)
        .unwrap();
    let val = store.get("bytes_key").unwrap().unwrap();
    assert!(val.as_string().is_none());
    assert_eq!(val.as_bytes(), Some(&[0u8, 1, 2, 3][..]));

    // Integer
    store.set("int_key", 42i64.into(), None).unwrap();
    let val = store.get("int_key").unwrap().unwrap();
    assert_eq!(val.as_integer(), Some(42));

    // Boolean
    store.set("bool_key", true.into(), None).unwrap();
    let val = store.get("bool_key").unwrap().unwrap();
    assert_eq!(val.as_bool(), Some(true));
}

/// Test increment/decrement operations
#[test]
fn should_increment_and_decrement() {
    let store = MemoryStore::new();

    // Increment on non-existent key creates it
    assert_eq!(store.increment("counter", 5).unwrap(), 5);
    assert_eq!(store.increment("counter", 3).unwrap(), 8);

    // Decrement
    assert_eq!(store.decrement("counter", 2).unwrap(), 6);

    // Set then increment
    store.set("set_counter", 10i64.into(), None).unwrap();
    assert_eq!(store.increment("set_counter", 1).unwrap(), 11);
}

/// Test error handling for increment on non-integer values
#[test]
fn should_error_on_increment_non_integer() {
    let store = MemoryStore::new();

    store.set("str_key", "not a number".into(), None).unwrap();

    let result = store.increment("str_key", 1);
    assert!(result.is_err());
}

/// Test store flush
#[test]
fn should_flush_all_keys() {
    let store = MemoryStore::new();

    store.set("key1", "value1".into(), None).unwrap();
    store.set("key2", "value2".into(), None).unwrap();
    store.set("key3", "value3".into(), None).unwrap();

    assert_eq!(store.len(), 3);

    store.flush().unwrap();

    assert_eq!(store.len(), 0);
    assert!(!store.exists("key1").unwrap());
    assert!(!store.exists("key2").unwrap());
    assert!(!store.exists("key3").unwrap());
}

/// Test fallback behavior on store errors
#[test]
fn should_fallback_on_store_unavailable() {
    // Create a namespaced store with fallback enabled
    let backend = Arc::new(MemoryStore::new());
    let ns = NamespacedStore::new(backend, "test", true); // fallback_on_error = true

    // Normal operations should work
    ns.set("key", "value".into(), None).unwrap();
    assert!(ns.exists("key").unwrap());

    // Get should return the value
    let val = ns.get("key").unwrap();
    assert_eq!(val, Some(StoreValue::String("value".to_string())));
}

/// Test SharedStore convenience methods
#[test]
fn should_shared_store_create_namespaces() {
    let shared = SharedStore::memory();

    // Create namespace with fallback
    let ns1 = shared.namespace("ns1");
    ns1.set("key", "value".into(), None).unwrap();

    // Create strict namespace
    let ns2 = shared.namespace_strict("ns2");
    ns2.set("key", "value2".into(), None).unwrap();

    // Both should work independently
    assert_eq!(
        ns1.get("key").unwrap(),
        Some(StoreValue::String("value".to_string()))
    );
    assert_eq!(
        ns2.get("key").unwrap(),
        Some(StoreValue::String("value2".to_string()))
    );
}

/// Test rate limiter scoped to different paths
#[test]
fn should_rate_limit_different_paths_independently() {
    let store = SharedStore::memory();
    let config = RateLimitConfig {
        enabled: true,
        global: None,
        scoped: vec![
            ScopedRateLimit {
                path_pattern: "/api/users/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 2,
                    window_secs: 60,
                    key_header: None,
                },
            },
            ScopedRateLimit {
                path_pattern: "/api/posts/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 5,
                    window_secs: 60,
                    key_header: None,
                },
            },
        ],
    };

    let limiter = SharedRateLimiter::with_store(&config, store);

    // Exhaust users limit
    assert!(
        limiter
            .check_scoped("/api/users/123", "127.0.0.1", None)
            .unwrap()
            .allowed
    );
    assert!(
        limiter
            .check_scoped("/api/users/123", "127.0.0.1", None)
            .unwrap()
            .allowed
    );
    assert!(
        !limiter
            .check_scoped("/api/users/123", "127.0.0.1", None)
            .unwrap()
            .allowed
    );

    // Posts should still work
    for _ in 0..5 {
        assert!(
            limiter
                .check_scoped("/api/posts/456", "127.0.0.1", None)
                .unwrap()
                .allowed
        );
    }
}

/// Test header-based rate limiting with shared store
#[test]
fn should_rate_limit_by_header_with_shared_store() {
    let store = SharedStore::memory();
    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 3,
            window_secs: 60,
            key_header: Some("X-API-Key".to_string()),
        }),
        scoped: vec![],
    };

    let limiter = SharedRateLimiter::with_store(&config, store);

    // Different API keys should have independent limits
    for _ in 0..3 {
        assert!(
            limiter
                .check_global("127.0.0.1", Some("key-a"))
                .unwrap()
                .allowed
        );
    }
    assert!(
        !limiter
            .check_global("127.0.0.1", Some("key-a"))
            .unwrap()
            .allowed
    );

    // key-b should still have full quota
    for _ in 0..3 {
        assert!(
            limiter
                .check_global("127.0.0.1", Some("key-b"))
                .unwrap()
                .allowed
        );
    }
    assert!(
        !limiter
            .check_global("127.0.0.1", Some("key-b"))
            .unwrap()
            .allowed
    );
}
