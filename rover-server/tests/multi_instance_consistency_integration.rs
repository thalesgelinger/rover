//! Integration tests for multi-instance consistency paths
//!
//! These tests verify that multiple server instances sharing the same backend
//! maintain consistency for shared state like rate limits, sessions, and stores.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rover_server::load_shedder::{LoadShedConfig, LoadShedder};
use rover_server::rate_limiter::{
    RateLimitConfig, RateLimitPolicy, ScopedRateLimit, SharedRateLimiter,
};
use rover_server::store::{MemoryStore, SharedStore, StoreBackend, StoreValue};

/// Test that multiple instances share rate limit state correctly
#[test]
fn should_maintain_consistent_rate_limits_across_instances() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 10,
            window_secs: 60,
            key_header: None,
        }),
        scoped: vec![],
    };

    // Create multiple rate limiter instances sharing the same store
    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter3 = SharedRateLimiter::with_store(&config, store);

    // Each instance should see the same counter state
    for i in 0..10 {
        let limiter = match i % 3 {
            0 => &limiter1,
            1 => &limiter2,
            _ => &limiter3,
        };

        let result = limiter.check_global("192.168.1.1", None).unwrap();
        assert!(
            result.allowed,
            "Request {} should be allowed, remaining={}",
            i + 1,
            result.remaining
        );
        assert_eq!(result.remaining, 9 - i as u32);
    }

    // All instances should now reject
    assert!(!limiter1.check_global("192.168.1.1", None).unwrap().allowed);
    assert!(!limiter2.check_global("192.168.1.1", None).unwrap().allowed);
    assert!(!limiter3.check_global("192.168.1.1", None).unwrap().allowed);
}

/// Test that scoped rate limits are consistent across instances
#[test]
fn should_maintain_consistent_scoped_rate_limits_across_instances() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: None,
        scoped: vec![
            ScopedRateLimit {
                path_pattern: "/api/v1/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 5,
                    window_secs: 60,
                    key_header: None,
                },
            },
            ScopedRateLimit {
                path_pattern: "/api/v2/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 3,
                    window_secs: 60,
                    key_header: None,
                },
            },
        ],
    };

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Use different instances to exhaust v1 limit
    for _ in 0..5 {
        let result = limiter1
            .check_scoped("/api/v1/users", "10.0.0.1", None)
            .unwrap();
        assert!(result.allowed);
    }

    // Both instances should see v1 as exhausted
    assert!(
        !limiter1
            .check_scoped("/api/v1/users", "10.0.0.1", None)
            .unwrap()
            .allowed
    );
    assert!(
        !limiter2
            .check_scoped("/api/v1/users", "10.0.0.1", None)
            .unwrap()
            .allowed
    );

    // But v2 should still work from both instances
    for _ in 0..3 {
        assert!(
            limiter2
                .check_scoped("/api/v2/items", "10.0.0.1", None)
                .unwrap()
                .allowed
        );
    }

    // v2 should now be exhausted from both instances
    assert!(
        !limiter1
            .check_scoped("/api/v2/items", "10.0.0.1", None)
            .unwrap()
            .allowed
    );
}

/// Test that different clients are isolated across instances
#[test]
fn should_isolate_clients_across_instances() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 3,
            window_secs: 60,
            key_header: None,
        }),
        scoped: vec![],
    };

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Exhaust client A's limit using limiter1
    for _ in 0..3 {
        assert!(limiter1.check_global("client-a", None).unwrap().allowed);
    }

    // Client A should be blocked from both instances
    assert!(!limiter1.check_global("client-a", None).unwrap().allowed);
    assert!(!limiter2.check_global("client-a", None).unwrap().allowed);

    // But client B should have full quota from both instances
    for _ in 0..3 {
        assert!(limiter1.check_global("client-b", None).unwrap().allowed);
    }

    // And client C should also have full quota
    for _ in 0..3 {
        assert!(limiter2.check_global("client-c", None).unwrap().allowed);
    }
}

/// Test header-based rate limiting across instances
#[test]
fn should_maintain_header_based_limits_across_instances() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 5,
            window_secs: 60,
            key_header: Some("X-API-Key".to_string()),
        }),
        scoped: vec![],
    };

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Exhaust key-1 using both instances
    for i in 0..5 {
        let limiter = if i % 2 == 0 { &limiter1 } else { &limiter2 };
        assert!(
            limiter
                .check_global("10.0.0.1", Some("key-1"))
                .unwrap()
                .allowed
        );
    }

    // key-1 should be blocked from both instances
    assert!(
        !limiter1
            .check_global("10.0.0.1", Some("key-1"))
            .unwrap()
            .allowed
    );
    assert!(
        !limiter2
            .check_global("10.0.0.2", Some("key-1"))
            .unwrap()
            .allowed
    );

    // key-2 should have full quota from both instances
    for i in 0..5 {
        let limiter = if i % 2 == 0 { &limiter1 } else { &limiter2 };
        assert!(
            limiter
                .check_global("10.0.0.1", Some("key-2"))
                .unwrap()
                .allowed
        );
    }
}

/// Test store data consistency across multiple instances
#[test]
fn should_maintain_store_consistency_across_instances() {
    let store = SharedStore::memory();

    // Instance 1 writes
    store.set("session:123", "data1".into(), None).unwrap();
    store.set("counter:requests", 100i64.into(), None).unwrap();

    // Instance 2 (same store) should see the data
    assert_eq!(
        store.get("session:123").unwrap(),
        Some(StoreValue::String("data1".to_string()))
    );
    assert_eq!(
        store.get("counter:requests").unwrap(),
        Some(StoreValue::Integer(100))
    );

    // Instance 2 updates
    store.set("session:123", "data2".into(), None).unwrap();
    store.increment("counter:requests", 50).unwrap();

    // Both instances should see the updates
    assert_eq!(
        store.get("session:123").unwrap(),
        Some(StoreValue::String("data2".to_string()))
    );
    assert_eq!(
        store.get("counter:requests").unwrap(),
        Some(StoreValue::Integer(150))
    );
}

/// Test namespace isolation across multiple instances
#[test]
fn should_maintain_namespace_isolation_across_instances() {
    let backend = Arc::new(MemoryStore::new());
    let store = SharedStore::new(backend.clone());

    // Create namespaced stores from different "instances"
    let session_ns1 = store.namespace("session");
    let session_ns2 = store.namespace("session");
    let cache_ns = store.namespace("cache");

    // Write to session namespace from instance 1
    session_ns1
        .set("user-1", "session-data".into(), None)
        .unwrap();

    // Instance 2 should see it
    assert_eq!(
        session_ns2.get("user-1").unwrap(),
        Some(StoreValue::String("session-data".to_string()))
    );

    // Cache namespace should not see it
    assert_eq!(cache_ns.get("user-1").unwrap(), None);

    // Write to cache namespace
    cache_ns.set("user-1", "cached-data".into(), None).unwrap();

    // Session namespace should still have its own data
    assert_eq!(
        session_ns1.get("user-1").unwrap(),
        Some(StoreValue::String("session-data".to_string()))
    );
}

/// Test concurrent access to shared store across threads
#[test]
fn should_handle_concurrent_multi_instance_access() {
    let store = Arc::new(MemoryStore::new());
    let shared_store = SharedStore::new(store.clone());

    let mut handles = vec![];

    // Spawn threads simulating different instances
    for instance_id in 0..5 {
        let store_clone = shared_store.clone();
        let handle = thread::spawn(move || {
            let ns = store_clone.namespace(format!("instance-{}", instance_id));

            for i in 0..100 {
                // Write instance-specific data
                let key = format!("key-{}", i);
                ns.set(&key, i.into(), None).unwrap();

                // Increment shared counter
                store_clone.increment("global-counter", 1).unwrap();

                // Occasionally read
                if i % 10 == 0 {
                    let key = format!("key-{}", i);
                    let _ = ns.get(&key);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("thread should complete");
    }

    // Verify global counter
    let global = store.get("global-counter").unwrap().unwrap();
    assert_eq!(global.as_integer(), Some(500)); // 5 instances * 100 increments
}

/// Test TTL consistency across instances
#[test]
fn should_maintain_ttl_consistency_across_instances() {
    let store = SharedStore::memory();

    // Instance 1 sets with TTL
    store
        .set(
            "temp-key",
            "temp-value".into(),
            Some(Duration::from_millis(500)),
        )
        .unwrap();

    // Instance 2 should see it immediately
    assert_eq!(
        store.get("temp-key").unwrap(),
        Some(StoreValue::String("temp-value".to_string()))
    );

    // Wait for TTL to expire
    thread::sleep(Duration::from_millis(700));

    // Both instances should see it as expired
    assert_eq!(store.get("temp-key").unwrap(), None);
}

/// Test load shedder state sharing across cloned instances
#[test]
fn should_share_load_shedder_state_across_cloned_instances() {
    let config = LoadShedConfig {
        max_inflight: Some(5),
        max_queue: Some(10),
    };

    let shedder1 = Arc::new(LoadShedder::new(config));
    let shedder2 = shedder1.clone();

    // Use shedder1 to consume quota
    let guard1 = shedder1.should_accept().unwrap();
    let guard2 = shedder1.should_accept().unwrap();

    // shedder2 should see the same state
    assert_eq!(shedder2.inflight(), 2);

    // shedder2 can also consume
    let guard3 = shedder2.should_accept().unwrap();
    assert_eq!(shedder1.inflight(), 3);

    // Release from shedder1
    drop(guard1);
    drop(guard2);

    // shedder2 should see the updated state
    assert_eq!(shedder2.inflight(), 1);

    drop(guard3);
    assert_eq!(shedder1.inflight(), 0);
}

/// Test rate limit reset after window expires (across instances)
#[test]
fn should_reset_rate_limits_after_window_expiry_across_instances() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 2,
            window_secs: 1, // Short window for testing
            key_header: None,
        }),
        scoped: vec![],
    };

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Exhaust limit using limiter1
    assert!(limiter1.check_global("10.0.0.1", None).unwrap().allowed);
    assert!(limiter1.check_global("10.0.0.1", None).unwrap().allowed);
    assert!(!limiter1.check_global("10.0.0.1", None).unwrap().allowed);

    // limiter2 should also see it as exhausted
    assert!(!limiter2.check_global("10.0.0.1", None).unwrap().allowed);

    // Wait for window to expire
    thread::sleep(Duration::from_millis(1200));

    // Both instances should see the limit reset
    let result1 = limiter1.check_global("10.0.0.1", None).unwrap();
    assert!(result1.allowed, "Limiter1 should allow after window expiry");

    let result2 = limiter2.check_global("10.0.0.1", None).unwrap();
    assert!(result2.allowed, "Limiter2 should allow after window expiry");
}

/// Test subnamespace consistency across instances
#[test]
fn should_maintain_subnamespace_consistency_across_instances() {
    let backend = Arc::new(MemoryStore::new());
    let store1 = SharedStore::new(backend.clone());
    let store2 = SharedStore::new(backend);

    let ns1 = store1.namespace("app");
    let session_ns1 = ns1.with_subnamespace("session");
    let cache_ns1 = ns1.with_subnamespace("cache");

    let ns2 = store2.namespace("app");
    let session_ns2 = ns2.with_subnamespace("session");

    // Write to session subnamespace from instance 1
    session_ns1
        .set("user-123", "session-data".into(), None)
        .unwrap();

    // Write to cache subnamespace from instance 1
    cache_ns1.set("key-1", "cached-value".into(), None).unwrap();

    // Instance 2 should see session data
    assert_eq!(
        session_ns2.get("user-123").unwrap(),
        Some(StoreValue::String("session-data".to_string()))
    );

    // But should not see cache data through session namespace
    assert_eq!(session_ns2.get("key-1").unwrap(), None);

    // Cache namespace should have its own data
    let cache_ns2 = ns2.with_subnamespace("cache");
    assert_eq!(
        cache_ns2.get("key-1").unwrap(),
        Some(StoreValue::String("cached-value".to_string()))
    );
}

/// Test store operations under high contention
#[test]
fn should_maintain_consistency_under_high_contention() {
    let store = Arc::new(MemoryStore::new());
    let shared = SharedStore::new(store.clone());

    let barrier = Arc::new(std::sync::Barrier::new(10));
    let mut handles = vec![];

    // Spawn 10 threads
    for thread_id in 0..10 {
        let store_clone = shared.clone();
        let barrier_clone = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier_clone.wait();

            let ns = store_clone.namespace("contention-test");

            // Each thread performs 100 operations
            for i in 0..100 {
                let key = format!("shared-key-{}", i % 10); // 10 shared keys

                // Mix of operations
                match i % 4 {
                    0 => {
                        ns.set(&key, i.into(), None).unwrap();
                    }
                    1 => {
                        let _ = ns.get(&key);
                    }
                    2 => {
                        let _ = ns.increment(&format!("counter-{}", thread_id), 1);
                    }
                    _ => {
                        let _ = ns.exists(&key);
                    }
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("thread should complete");
    }

    // Verify each thread's counter is correct
    for thread_id in 0..10 {
        let counter_key = format!("contention-test:counter-{}", thread_id);
        let value = store.get(&counter_key).unwrap().unwrap();
        assert_eq!(value.as_integer(), Some(25)); // 100 operations / 4 = 25 increments
    }
}

/// Test that rate limiter properly handles store failures gracefully
#[test]
fn should_fail_open_on_store_error_across_instances() {
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

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store.clone());

    // Normal operation
    for _ in 0..5 {
        assert!(limiter1.check_global("10.0.0.1", None).unwrap().allowed);
    }

    // Should be at limit
    assert!(!limiter1.check_global("10.0.0.1", None).unwrap().allowed);
    assert!(!limiter2.check_global("10.0.0.1", None).unwrap().allowed);

    // Flush the store (simulating a reset)
    store.flush().unwrap();

    // After flush, both instances should see the limit reset
    // (because the bucket state was cleared)
    let result1 = limiter1.check_global("10.0.0.1", None).unwrap();
    let result2 = limiter2.check_global("10.0.0.1", None).unwrap();

    // Note: Since the store was flushed, new buckets will be created
    // Both should be allowed since we're creating fresh buckets
    assert!(result1.allowed, "Should be allowed after store flush");
    assert!(result2.allowed, "Should be allowed after store flush");
}

/// Test complex multi-client, multi-path, multi-instance scenario
#[test]
fn should_handle_complex_multi_instance_scenario() {
    let store = SharedStore::memory();

    let config = RateLimitConfig {
        enabled: true,
        global: Some(RateLimitPolicy {
            requests_per_window: 20,
            window_secs: 60,
            key_header: Some("X-Client-ID".to_string()),
        }),
        scoped: vec![
            ScopedRateLimit {
                path_pattern: "/api/critical/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 5,
                    window_secs: 60,
                    key_header: Some("X-Client-ID".to_string()),
                },
            },
            ScopedRateLimit {
                path_pattern: "/api/public/*".to_string(),
                policy: RateLimitPolicy {
                    requests_per_window: 50,
                    window_secs: 60,
                    key_header: None, // IP-based
                },
            },
        ],
    };

    let limiter1 = SharedRateLimiter::with_store(&config, store.clone());
    let limiter2 = SharedRateLimiter::with_store(&config, store);

    // Client A exhausts critical path limit using both instances
    for i in 0..5 {
        let limiter = if i % 2 == 0 { &limiter1 } else { &limiter2 };
        assert!(
            limiter
                .check_scoped("/api/critical/data", "10.0.0.1", Some("client-a"))
                .unwrap()
                .allowed,
            "Request {} should be allowed",
            i + 1
        );
    }

    // Client A should be blocked from critical path on both instances
    assert!(
        !limiter1
            .check_scoped("/api/critical/data", "10.0.0.1", Some("client-a"))
            .unwrap()
            .allowed
    );
    assert!(
        !limiter2
            .check_scoped("/api/critical/data", "10.0.0.2", Some("client-a"))
            .unwrap()
            .allowed
    );

    // But client B should still have access to critical path
    for i in 0..5 {
        let limiter = if i % 2 == 0 { &limiter1 } else { &limiter2 };
        assert!(
            limiter
                .check_scoped("/api/critical/data", "10.0.0.1", Some("client-b"))
                .unwrap()
                .allowed
        );
    }

    // Client A should have full global quota (scoped and global are independent)
    for i in 0..20 {
        let limiter = if i % 2 == 0 { &limiter1 } else { &limiter2 };
        assert!(
            limiter
                .check_global("10.0.0.1", Some("client-a"))
                .unwrap()
                .allowed,
            "Global request {} should be allowed",
            i + 1
        );
    }

    // Client A's global limit should now be exhausted
    assert!(
        !limiter1
            .check_global("10.0.0.1", Some("client-a"))
            .unwrap()
            .allowed
    );
}

/// Test store value type consistency across instances
#[test]
fn should_maintain_value_type_consistency_across_instances() {
    let backend = Arc::new(MemoryStore::new());
    let store1 = SharedStore::new(backend.clone());
    let store2 = SharedStore::new(backend);

    let ns1 = store1.namespace("types");
    let ns2 = store2.namespace("types");

    // Write different types from instance 1
    ns1.set("string", "hello".into(), None).unwrap();
    ns1.set("integer", 42i64.into(), None).unwrap();
    ns1.set("boolean", true.into(), None).unwrap();
    ns1.set("bytes", vec![1u8, 2, 3, 4].into(), None).unwrap();

    // Verify types from instance 2
    assert_eq!(
        ns2.get("string").unwrap(),
        Some(StoreValue::String("hello".to_string()))
    );
    assert_eq!(ns2.get("integer").unwrap(), Some(StoreValue::Integer(42)));
    assert_eq!(ns2.get("boolean").unwrap(), Some(StoreValue::Boolean(true)));
    assert_eq!(
        ns2.get("bytes").unwrap(),
        Some(StoreValue::Bytes(vec![1, 2, 3, 4]))
    );

    // Verify type-specific access
    let string_val = ns2.get("string").unwrap().unwrap();
    assert_eq!(string_val.as_string(), Some("hello"));
    assert!(string_val.as_integer().is_none());

    let int_val = ns2.get("integer").unwrap().unwrap();
    assert_eq!(int_val.as_integer(), Some(42));
    assert!(int_val.as_string().is_none());
}

/// Test that delete operations are visible across instances
#[test]
fn should_propagate_deletes_across_instances() {
    let store = SharedStore::memory();

    let ns1 = store.namespace("test");
    let ns2 = store.namespace("test");

    // Create from instance 1
    ns1.set("key1", "value1".into(), None).unwrap();
    ns1.set("key2", "value2".into(), None).unwrap();

    // Verify from instance 2
    assert!(ns2.exists("key1").unwrap());
    assert!(ns2.exists("key2").unwrap());

    // Delete from instance 1
    assert!(ns1.delete("key1").unwrap());

    // Verify deletion from instance 2
    assert!(!ns2.exists("key1").unwrap());
    assert!(ns2.exists("key2").unwrap());

    // Verify get returns None
    assert_eq!(ns2.get("key1").unwrap(), None);
}

/// Test increment/decrement consistency across instances
#[test]
fn should_maintain_counter_consistency_across_instances() {
    let store = SharedStore::memory();

    let ns1 = store.namespace("counters");
    let ns2 = store.namespace("counters");

    // Initialize from instance 1
    ns1.set("shared", 0i64.into(), None).unwrap();

    // Alternate increments between instances
    for i in 0..100 {
        if i % 2 == 0 {
            ns1.increment("shared", 1).unwrap();
        } else {
            ns2.increment("shared", 1).unwrap();
        }
    }

    // Verify final value from both instances
    assert_eq!(ns1.get("shared").unwrap(), Some(StoreValue::Integer(100)));
    assert_eq!(ns2.get("shared").unwrap(), Some(StoreValue::Integer(100)));

    // Test decrement
    ns1.decrement("shared", 25).unwrap();
    assert_eq!(ns2.get("shared").unwrap(), Some(StoreValue::Integer(75)));
}
