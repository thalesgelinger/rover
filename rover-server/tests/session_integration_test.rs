//! Integration tests for login-style session workflows
//!
//! These tests verify session issuance, persistence, and invalidation
//! in scenarios typical of user authentication flows.

use std::thread;
use std::time::Duration;

use rover_server::session::{SameSite, SessionConfig, SessionState, SessionStore};
use rover_server::store::SharedStore;

/// Test basic session creation and ID generation
#[test]
fn should_issue_new_session_with_unique_id() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    let session1 = store.create_session();
    let session2 = store.create_session();
    let session3 = store.create_session();

    // Each session should have a unique, non-empty ID
    assert!(!session1.id().is_empty(), "Session 1 should have an ID");
    assert!(!session2.id().is_empty(), "Session 2 should have an ID");
    assert!(!session3.id().is_empty(), "Session 3 should have an ID");

    // All IDs should be unique
    assert_ne!(session1.id(), session2.id(), "Session IDs should be unique");
    assert_ne!(session2.id(), session3.id(), "Session IDs should be unique");
    assert_ne!(session1.id(), session3.id(), "Session IDs should be unique");
}

/// Test session persistence with user data
#[test]
fn should_persist_session_data_across_retrieval() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Create a session with user data
    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "user_123");
    session.set("username", "alice");
    session.set("role", "admin");
    session.set("login_count", 42i64);
    session.set("is_verified", true);

    // Save the session
    session.save().unwrap();

    // Retrieve the session
    let retrieved = store
        .get_session(&session_id)
        .unwrap()
        .expect("Session should exist");

    // Verify all data persisted
    assert_eq!(retrieved.get_string("user_id"), Some("user_123"));
    assert_eq!(retrieved.get_string("username"), Some("alice"));
    assert_eq!(retrieved.get_string("role"), Some("admin"));
    assert_eq!(retrieved.get_integer("login_count"), Some(42));
    assert_eq!(retrieved.get_bool("is_verified"), Some(true));
}

/// Test login-style session regeneration (security best practice)
#[test]
fn should_regenerate_session_id_after_login() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Create initial session (e.g., anonymous browsing)
    let mut session = store.create_session();
    let old_id = session.id().to_string();

    // Add some pre-login data
    session.set("cart_items", 3i64);
    session.save().unwrap();

    // Simulate login - regenerate session ID
    let new_id = session.regenerate_id().unwrap();

    // Verify ID changed
    assert_ne!(
        old_id, new_id,
        "Session ID should change after regeneration"
    );
    assert_eq!(session.id(), new_id, "Session should have new ID");

    // Old session should no longer exist
    assert!(
        !store.session_exists(&old_id).unwrap(),
        "Old session ID should be invalidated"
    );

    // New session should exist with data preserved
    let retrieved = store
        .get_session(&new_id)
        .unwrap()
        .expect("New session should exist");
    assert_eq!(
        retrieved.get_integer("cart_items"),
        Some(3),
        "Data should persist after regeneration"
    );
}

/// Test explicit session invalidation (logout)
#[test]
fn should_invalidate_session_on_logout() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Create authenticated session
    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "user_456");
    session.set("auth_token", "secret_token_123");
    session.save().unwrap();

    // Verify session is initially valid
    assert!(session.is_valid(), "Session should be valid initially");
    assert_eq!(session.state(), SessionState::Active);

    // Simulate logout - invalidate session
    session.invalidate().unwrap();

    // Verify session is now invalid
    assert!(
        !session.is_valid(),
        "Session should be invalid after logout"
    );
    assert_eq!(session.state(), SessionState::Invalidated);

    // Verify invalidation persisted
    let retrieved = store
        .get_session(&session_id)
        .unwrap()
        .expect("Session should exist");
    assert!(!retrieved.is_valid(), "Retrieved session should be invalid");
    assert_eq!(retrieved.state(), SessionState::Invalidated);
}

/// Test session expiration based on TTL
#[test]
fn should_expire_session_after_ttl() {
    // Use a very short TTL for testing
    let config = SessionConfig {
        ttl_secs: 1, // 1 second TTL
        ..Default::default()
    };
    let store = SessionStore::new(config);

    // Create session
    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "user_789");
    session.save().unwrap();

    // Verify session is valid initially
    assert!(
        !session.is_expired(),
        "Session should not be expired immediately"
    );
    assert!(session.is_valid(), "Session should be valid initially");

    // Wait for TTL to expire
    thread::sleep(Duration::from_secs(2));

    // Session should now be expired (based on in-memory state)
    assert!(session.is_expired(), "Session should be expired after TTL");
    assert!(
        !session.is_valid(),
        "Session should be invalid when expired"
    );

    // Retrieve from store - should return None since TTL expired and store purged it
    let retrieved = store.get_session(&session_id).unwrap();
    assert!(
        retrieved.is_none(),
        "Expired session should be purged from store"
    );
}

/// Test session refresh extends expiration
#[test]
fn should_extend_session_lifetime_on_refresh() {
    let config = SessionConfig {
        ttl_secs: 2, // 2 second TTL
        ..Default::default()
    };
    let store = SessionStore::new(config);

    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "user_999");
    session.save().unwrap();

    // Wait 1 second
    thread::sleep(Duration::from_secs(1));

    // Refresh session (extends TTL)
    session.refresh().unwrap();

    // Wait another 1.5 seconds (would have expired without refresh)
    thread::sleep(Duration::from_millis(1500));

    // Session should still be valid
    assert!(
        !session.is_expired(),
        "Session should not be expired after refresh"
    );
    assert!(session.is_valid(), "Session should still be valid");

    // Verify persisted state
    let retrieved = store
        .get_session(&session_id)
        .unwrap()
        .expect("Session should exist");
    assert!(
        !retrieved.is_expired(),
        "Retrieved session should not be expired"
    );
}

/// Test get_or_create with existing session
#[test]
fn should_get_existing_session_or_create_new() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Create and save a session
    let mut session = store.create_session();
    let session_id = session.id().to_string();
    session.set("user_id", "existing_user");
    session.save().unwrap();

    // get_or_create with existing ID should return the session
    let retrieved = store.get_or_create(Some(&session_id)).unwrap();
    assert_eq!(
        retrieved.id(),
        session_id,
        "Should retrieve existing session"
    );
    assert_eq!(retrieved.get_string("user_id"), Some("existing_user"));

    // get_or_create with non-existent ID should create new session
    let new_session = store.get_or_create(Some("nonexistent_id")).unwrap();
    assert_ne!(
        new_session.id(),
        "nonexistent_id",
        "Should create new session with different ID"
    );

    // get_or_create with None should create new session
    let another_session = store.get_or_create(None).unwrap();
    assert!(
        !another_session.id().is_empty(),
        "Should create new session"
    );
}

/// Test session destruction (hard delete)
#[test]
fn should_destroy_session_completely() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "user_to_delete");
    session.save().unwrap();

    // Verify session exists
    assert!(
        store.session_exists(&session_id).unwrap(),
        "Session should exist"
    );

    // Destroy session
    session.destroy().unwrap();

    // Verify session is completely gone
    assert!(
        !store.session_exists(&session_id).unwrap(),
        "Session should not exist after destroy"
    );
    assert!(session.is_empty(), "Session data should be cleared");
}

/// Test session cookie generation with various configurations
#[test]
fn should_generate_correct_session_cookie() {
    let config = SessionConfig {
        cookie_name: "auth_session".to_string(),
        secure: true,
        http_only: true,
        same_site: SameSite::Strict,
        path: "/app".to_string(),
        domain: Some("example.com".to_string()),
        ttl_secs: 3600,
    };
    let store = SessionStore::new(config);

    let session = store.create_session();
    let cookie = session.cookie_string();

    // Verify cookie attributes
    assert!(
        cookie.contains("auth_session="),
        "Cookie should have correct name"
    );
    assert!(
        cookie.contains("Path=/app"),
        "Cookie should have correct path"
    );
    assert!(
        cookie.contains("SameSite=Strict"),
        "Cookie should have SameSite=Strict"
    );
    assert!(cookie.contains("HttpOnly"), "Cookie should be HttpOnly");
    assert!(cookie.contains("Secure"), "Cookie should be Secure");
    assert!(
        cookie.contains("Domain=example.com"),
        "Cookie should have domain"
    );
}

/// Test session sharing across multiple store instances
#[test]
fn should_share_sessions_across_store_instances() {
    let shared_store = SharedStore::memory();
    let config = SessionConfig::default();

    // Create two session stores sharing the same backend
    let store1 = SessionStore::with_store(config.clone(), shared_store.clone());
    let store2 = SessionStore::with_store(config, shared_store);

    // Create session with store1
    let mut session = store1.create_session();
    let session_id = session.id().to_string();
    session.set("shared_data", "test_value");
    session.save().unwrap();

    // store2 should be able to retrieve it
    let retrieved = store2
        .get_session(&session_id)
        .unwrap()
        .expect("Session should be accessible from store2");
    assert_eq!(retrieved.get_string("shared_data"), Some("test_value"));

    // Invalidate from store2
    let mut session2 = store2.get_session(&session_id).unwrap().unwrap();
    session2.invalidate().unwrap();

    // store1 should see the invalidation
    let check = store1.get_session(&session_id).unwrap().unwrap();
    assert!(
        !check.is_valid(),
        "Session should be invalid in store1 after store2 invalidated it"
    );
}

/// Test complete login workflow
#[test]
fn should_handle_complete_login_workflow() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Step 1: Anonymous user gets a session
    let mut session = store.create_session();
    let anon_id = session.id().to_string();
    session.set("cart", "items:3");
    session.save().unwrap();

    // Step 2: User logs in - regenerate session ID for security
    let user_id = "user_12345";
    let username = "john_doe";

    // Preserve anonymous data (will be transferred to new session)
    let _cart_data = session.get_string("cart").unwrap().to_string();

    // Regenerate session ID
    let new_id = session.regenerate_id().unwrap();
    assert_ne!(anon_id, new_id, "Session ID should change on login");

    // Add authentication data
    session.set("user_id", user_id);
    session.set("username", username);
    session.set("authenticated_at", unix_secs() as i64);
    session.set("auth_method", "password");
    session.save().unwrap();

    // Step 3: Verify old session is gone, new session has all data
    assert!(
        !store.session_exists(&anon_id).unwrap(),
        "Anonymous session should be gone"
    );

    let auth_session = store
        .get_session(&new_id)
        .unwrap()
        .expect("Auth session should exist");
    assert_eq!(auth_session.get_string("user_id"), Some(user_id));
    assert_eq!(auth_session.get_string("username"), Some(username));
    assert_eq!(
        auth_session.get_string("cart"),
        Some("items:3"),
        "Cart data should persist"
    );
    assert!(auth_session.is_valid(), "Session should be valid");

    // Step 4: User logs out - invalidate session
    let mut logout_session = store.get_session(&new_id).unwrap().unwrap();
    logout_session.invalidate().unwrap();

    // Step 5: Verify session is invalid
    let after_logout = store.get_session(&new_id).unwrap().unwrap();
    assert!(
        !after_logout.is_valid(),
        "Session should be invalid after logout"
    );
}

/// Test session state transitions
#[test]
fn should_handle_session_state_transitions() {
    let config = SessionConfig {
        ttl_secs: 2, // 2 second TTL for testing
        ..Default::default()
    };
    let store = SessionStore::new(config);

    // Create session - starts Active
    let mut session = store.create_session();
    let session_id = session.id().to_string();
    session.save().unwrap();

    assert_eq!(
        session.state(),
        SessionState::Active,
        "New session should be Active"
    );
    assert!(session.is_valid(), "Active session should be valid");

    // Test invalidation while session is still active
    session.invalidate().unwrap();
    assert_eq!(
        session.state(),
        SessionState::Invalidated,
        "Session should be Invalidated"
    );
    assert!(
        !session.is_valid(),
        "Invalidated session should not be valid"
    );

    // Save invalidated state
    session.save().unwrap();

    // Retrieve and verify still invalidated
    let invalidated = store
        .get_session(&session_id)
        .unwrap()
        .expect("Session should exist");
    assert_eq!(
        invalidated.state(),
        SessionState::Invalidated,
        "Retrieved session should be Invalidated"
    );
    assert!(
        !invalidated.is_valid(),
        "Invalidated session should not be valid"
    );

    // Test expiration detection on in-memory session
    let mut short_ttl_session = store.create_session();
    short_ttl_session.set("data", "value");
    short_ttl_session.save().unwrap();

    // Wait for expiration
    thread::sleep(Duration::from_secs(3));

    // The in-memory session object should detect it's expired
    assert!(
        short_ttl_session.is_expired(),
        "Session should detect expiration"
    );
    assert!(
        !short_ttl_session.is_valid(),
        "Expired session should not be valid"
    );
}

/// Test concurrent session creation from multiple threads
#[test]
fn should_handle_concurrent_session_creation() {
    let shared_store = SharedStore::memory();
    let config = SessionConfig::default();
    let store = SessionStore::with_store(config, shared_store);

    let mut handles = vec![];
    let session_ids = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn threads that each create their own sessions
    for thread_id in 0..10 {
        let store_clone = store.clone();
        let ids_clone = session_ids.clone();
        let handle = thread::spawn(move || {
            for i in 0..5 {
                let mut session = store_clone.create_session();
                let id = session.id().to_string();
                session.set("thread_id", thread_id as i64);
                session.set("index", i as i64);
                session.save().unwrap();

                ids_clone.lock().unwrap().push(id);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify all 50 sessions were created and are retrievable
    let ids = session_ids.lock().unwrap();
    assert_eq!(ids.len(), 50, "Should have created 50 sessions");

    // Verify each session has correct data
    for id in ids.iter() {
        let session = store
            .get_session(id)
            .unwrap()
            .expect("Session should exist");
        let thread_id = session
            .get_integer("thread_id")
            .expect("Should have thread_id");
        assert!(
            (0..10).contains(&thread_id),
            "thread_id should be in range 0-9"
        );
    }
}

/// Test that invalidated sessions cannot be made valid again
#[test]
fn should_not_allow_invalidated_session_to_become_valid() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    let mut session = store.create_session();
    let session_id = session.id().to_string();

    session.set("user_id", "test_user");
    session.save().unwrap();

    // Invalidate the session
    session.invalidate().unwrap();
    assert!(
        !session.is_valid(),
        "Session should be invalid after invalidate"
    );

    // Try to refresh - should still be invalid
    session.refresh().unwrap();
    assert!(
        !session.is_valid(),
        "Invalidated session should remain invalid after refresh"
    );
    assert_eq!(
        session.state(),
        SessionState::Invalidated,
        "State should remain Invalidated"
    );

    // Verify persisted
    let retrieved = store.get_session(&session_id).unwrap().unwrap();
    assert!(
        !retrieved.is_valid(),
        "Retrieved session should still be invalid"
    );
    assert_eq!(retrieved.state(), SessionState::Invalidated);
}

/// Test session data modification after retrieval
#[test]
fn should_allow_modifying_retrieved_session() {
    let config = SessionConfig::default();
    let store = SessionStore::new(config);

    // Create and save initial session
    let mut session = store.create_session();
    let session_id = session.id().to_string();
    session.set("initial", "data");
    session.save().unwrap();

    // Retrieve and modify
    let mut retrieved = store.get_session(&session_id).unwrap().unwrap();
    retrieved.set("new_key", "new_value");
    retrieved.set("initial", "modified");
    retrieved.remove("initial");
    retrieved.save().unwrap();

    // Retrieve again and verify changes
    let final_session = store.get_session(&session_id).unwrap().unwrap();
    assert_eq!(final_session.get_string("new_key"), Some("new_value"));
    assert!(
        final_session.get("initial").is_none(),
        "Removed key should not exist"
    );
}

/// Helper function to get current unix timestamp
fn unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
