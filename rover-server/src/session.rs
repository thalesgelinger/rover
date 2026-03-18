use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::store::{NamespacedStore, SharedStore, StoreResult, StoreValue};

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Session cookie name
    pub cookie_name: String,
    /// Session TTL in seconds
    pub ttl_secs: u64,
    /// Whether to use secure cookies (HTTPS only)
    pub secure: bool,
    /// Whether cookies are HTTP-only
    pub http_only: bool,
    /// SameSite cookie attribute
    pub same_site: SameSite,
    /// Domain for the cookie
    pub domain: Option<String>,
    /// Path for the cookie
    pub path: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            cookie_name: "rover_session".to_string(),
            ttl_secs: 3600, // 1 hour
            secure: true,
            http_only: true,
            same_site: SameSite::Lax,
            domain: None,
            path: "/".to_string(),
        }
    }
}

/// SameSite cookie attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl SameSite {
    pub fn as_str(&self) -> &'static str {
        match self {
            SameSite::Strict => "Strict",
            SameSite::Lax => "Lax",
            SameSite::None => "None",
        }
    }
}

/// Session data container
#[derive(Debug, Clone)]
pub struct SessionData {
    data: HashMap<String, StoreValue>,
    created_at: u64,
    last_accessed: u64,
}

impl SessionData {
    fn new() -> Self {
        let now = unix_secs();
        Self {
            data: HashMap::new(),
            created_at: now,
            last_accessed: now,
        }
    }

    fn touch(&mut self) {
        self.last_accessed = unix_secs();
    }

    pub fn get(&self, key: &str) -> Option<&StoreValue> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<StoreValue>) {
        self.data.insert(key.into(), value.into());
        self.touch();
    }

    pub fn remove(&mut self, key: &str) -> Option<StoreValue> {
        let result = self.data.remove(key);
        self.touch();
        result
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    pub fn last_accessed(&self) -> u64 {
        self.last_accessed
    }
}

/// Session handle for a specific session
#[derive(Clone)]
pub struct Session {
    id: String,
    data: SessionData,
    store: NamespacedStore,
    config: Arc<SessionConfig>,
    modified: bool,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("data", &self.data)
            .field("modified", &self.modified)
            .finish_non_exhaustive()
    }
}

impl Session {
    fn new(
        id: String,
        data: SessionData,
        store: NamespacedStore,
        config: Arc<SessionConfig>,
    ) -> Self {
        Self {
            id,
            data,
            store,
            config,
            modified: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn get(&self, key: &str) -> Option<&StoreValue> {
        self.data.get(key)
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(|v| v.as_string())
    }

    pub fn get_integer(&self, key: &str) -> Option<i64> {
        self.data.get(key).and_then(|v| v.as_integer())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.get(key).and_then(|v| v.as_bool())
    }

    pub fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        self.data.get(key).and_then(|v| v.as_bytes())
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<StoreValue>) {
        self.data.set(key, value);
        self.modified = true;
    }

    pub fn remove(&mut self, key: &str) -> Option<StoreValue> {
        let result = self.data.remove(key);
        if result.is_some() {
            self.modified = true;
        }
        result
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn created_at(&self) -> u64 {
        self.data.created_at()
    }

    pub fn last_accessed(&self) -> u64 {
        self.data.last_accessed()
    }

    /// Save session to store
    pub fn save(&mut self) -> StoreResult<()> {
        if !self.modified {
            return Ok(());
        }

        let serialized = serialize_session(&self.data);
        let ttl = Some(Duration::from_secs(self.config.ttl_secs));
        self.store.set(&self.id, serialized.into(), ttl)?;
        self.modified = false;
        Ok(())
    }

    /// Delete this session from the store
    pub fn destroy(&mut self) -> StoreResult<bool> {
        self.data = SessionData::new();
        self.modified = false;
        self.store.delete(&self.id)
    }

    /// Regenerate session ID (for security after login)
    pub fn regenerate_id(&mut self) -> StoreResult<String> {
        // Delete old session
        let _ = self.store.delete(&self.id);

        // Generate new ID
        self.id = generate_session_id();
        self.modified = true;

        // Save with new ID
        self.save()?;

        Ok(self.id.clone())
    }

    /// Get the cookie string for this session
    pub fn cookie_string(&self) -> String {
        let mut cookie = format!(
            "{}={}; Path={}; SameSite={}",
            self.config.cookie_name,
            self.id,
            self.config.path,
            self.config.same_site.as_str()
        );

        if self.config.http_only {
            cookie.push_str("; HttpOnly");
        }

        if self.config.secure {
            cookie.push_str("; Secure");
        }

        if let Some(ref domain) = self.config.domain {
            cookie.push_str(&format!("; Domain={}", domain));
        }

        cookie
    }
}

/// Session store manager
#[derive(Clone)]
pub struct SessionStore {
    store: NamespacedStore,
    config: Arc<SessionConfig>,
}

impl std::fmt::Debug for SessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStore")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl SessionStore {
    /// Create a new session store with memory backend
    pub fn new(config: SessionConfig) -> Self {
        let store = SharedStore::memory();
        Self::with_store(config, store)
    }

    /// Create a new session store with custom store backend
    pub fn with_store(config: SessionConfig, store: SharedStore) -> Self {
        Self {
            store: store.namespace("session"),
            config: Arc::new(config),
        }
    }

    /// Create a new session
    pub fn create_session(&self) -> Session {
        let id = generate_session_id();
        let data = SessionData::new();

        Session::new(id, data, self.store.clone(), self.config.clone())
    }

    /// Get an existing session by ID
    pub fn get_session(&self, session_id: &str) -> StoreResult<Option<Session>> {
        if let Some(value) = self.store.get(session_id)? {
            if let Some(bytes) = value.as_bytes() {
                if let Some(data) = deserialize_session(bytes) {
                    return Ok(Some(Session::new(
                        session_id.to_string(),
                        data,
                        self.store.clone(),
                        self.config.clone(),
                    )));
                }
            }
        }

        Ok(None)
    }

    /// Get or create a session
    pub fn get_or_create(&self, session_id: Option<&str>) -> StoreResult<Session> {
        if let Some(id) = session_id {
            if let Some(session) = self.get_session(id)? {
                return Ok(session);
            }
        }

        Ok(self.create_session())
    }

    /// Delete a session by ID
    pub fn delete_session(&self, session_id: &str) -> StoreResult<bool> {
        self.store.delete(session_id)
    }

    /// Check if a session exists
    pub fn session_exists(&self, session_id: &str) -> StoreResult<bool> {
        self.store.exists(session_id)
    }

    /// Get the session config
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Get the cookie name
    pub fn cookie_name(&self) -> &str {
        &self.config.cookie_name
    }
}

fn generate_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = unix_secs();
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    let random = rand::random::<u64>();

    // Create a base64-encoded session ID
    let bytes = [
        &timestamp.to_be_bytes()[..],
        &counter.to_be_bytes()[..],
        &random.to_be_bytes()[..],
    ]
    .concat();

    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut result = String::with_capacity((bytes.len() * 4 + 2) / 3);

    for chunk in bytes.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            _ => [chunk[0], chunk[1], chunk[2]],
        };

        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);

        result.push(BASE64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(BASE64_CHARS[((n >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(BASE64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        }

        if chunk.len() > 2 {
            result.push(BASE64_CHARS[(n & 0x3F) as usize] as char);
        }
    }

    result
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn serialize_session(data: &SessionData) -> Vec<u8> {
    // Simple serialization format:
    // - 8 bytes: created_at (u64, big-endian)
    // - 8 bytes: last_accessed (u64, big-endian)
    // - 4 bytes: number of entries (u32, big-endian)
    // For each entry:
    //   - 2 bytes: key length (u16)
    //   - N bytes: key
    //   - 1 byte: value type (0=String, 1=Bytes, 2=Integer, 3=Bool)
    //   - For String: 4 bytes length + N bytes data
    //   - For Bytes: 4 bytes length + N bytes data
    //   - For Integer: 8 bytes
    //   - For Bool: 1 byte

    let mut result = Vec::new();

    // Header
    result.extend_from_slice(&data.created_at.to_be_bytes());
    result.extend_from_slice(&data.last_accessed.to_be_bytes());
    result.extend_from_slice(&(data.data.len() as u32).to_be_bytes());

    // Entries
    for (key, value) in &data.data {
        // Key
        result.extend_from_slice(&(key.len() as u16).to_be_bytes());
        result.extend_from_slice(key.as_bytes());

        // Value type and data
        match value {
            StoreValue::String(s) => {
                result.push(0);
                result.extend_from_slice(&(s.len() as u32).to_be_bytes());
                result.extend_from_slice(s.as_bytes());
            }
            StoreValue::Bytes(b) => {
                result.push(1);
                result.extend_from_slice(&(b.len() as u32).to_be_bytes());
                result.extend_from_slice(b);
            }
            StoreValue::Integer(i) => {
                result.push(2);
                result.extend_from_slice(&i.to_be_bytes());
            }
            StoreValue::Boolean(b) => {
                result.push(3);
                result.push(*b as u8);
            }
        }
    }

    result
}

fn deserialize_session(bytes: &[u8]) -> Option<SessionData> {
    if bytes.len() < 20 {
        return None;
    }

    let mut pos = 0;

    // Header
    let created_at = u64::from_be_bytes(bytes[pos..pos + 8].try_into().ok()?);
    pos += 8;

    let last_accessed = u64::from_be_bytes(bytes[pos..pos + 8].try_into().ok()?);
    pos += 8;

    let entry_count = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    let mut data = HashMap::new();

    for _ in 0..entry_count {
        if pos + 2 > bytes.len() {
            return None;
        }

        // Key length
        let key_len = u16::from_be_bytes(bytes[pos..pos + 2].try_into().ok()?) as usize;
        pos += 2;

        if pos + key_len > bytes.len() {
            return None;
        }

        // Key
        let key = String::from_utf8(bytes[pos..pos + key_len].to_vec()).ok()?;
        pos += key_len;

        if pos + 1 > bytes.len() {
            return None;
        }

        // Value type
        let value_type = bytes[pos];
        pos += 1;

        // Value data
        let value = match value_type {
            0 => {
                // String
                if pos + 4 > bytes.len() {
                    return None;
                }
                let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;

                if pos + len > bytes.len() {
                    return None;
                }
                let s = String::from_utf8(bytes[pos..pos + len].to_vec()).ok()?;
                pos += len;
                StoreValue::String(s)
            }
            1 => {
                // Bytes
                if pos + 4 > bytes.len() {
                    return None;
                }
                let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;

                if pos + len > bytes.len() {
                    return None;
                }
                let b = bytes[pos..pos + len].to_vec();
                pos += len;
                StoreValue::Bytes(b)
            }
            2 => {
                // Integer
                if pos + 8 > bytes.len() {
                    return None;
                }
                let i = i64::from_be_bytes(bytes[pos..pos + 8].try_into().ok()?);
                pos += 8;
                StoreValue::Integer(i)
            }
            3 => {
                // Boolean
                if pos + 1 > bytes.len() {
                    return None;
                }
                let b = bytes[pos] != 0;
                pos += 1;
                StoreValue::Boolean(b)
            }
            _ => return None,
        };

        data.insert(key, value);
    }

    Some(SessionData {
        data,
        created_at,
        last_accessed,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        SameSite, SessionConfig, SessionData, SessionStore, deserialize_session, serialize_session,
    };
    use crate::rate_limiter::{RateLimitConfig, RateLimitPolicy, SharedRateLimiter};
    use crate::store::SharedStore;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn should_create_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let session = store.create_session();
        assert!(!session.id().is_empty());
        assert!(session.is_empty());
        assert_eq!(session.len(), 0);
    }

    #[test]
    fn should_set_and_get_session_data() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("user_id", "123");
        session.set("count", 42i64);
        session.set("active", true);

        assert_eq!(session.get_string("user_id"), Some("123"));
        assert_eq!(session.get_integer("count"), Some(42));
        assert_eq!(session.get_bool("active"), Some(true));
        assert_eq!(session.len(), 3);
    }

    #[test]
    fn should_save_and_retrieve_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        // Retrieve session
        let retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert_eq!(retrieved.get_string("key"), Some("value"));
    }

    #[test]
    fn should_return_none_for_nonexistent_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let result = store.get_session("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn should_delete_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        assert!(store.session_exists(&session_id).unwrap());

        let deleted = store.delete_session(&session_id).unwrap();
        assert!(deleted);

        assert!(!store.session_exists(&session_id).unwrap());
    }

    #[test]
    fn should_destroy_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        session.destroy().unwrap();

        assert!(!store.session_exists(&session_id).unwrap());
        assert!(session.is_empty());
    }

    #[test]
    fn should_regenerate_session_id() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let old_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        let new_id = session.regenerate_id().unwrap();

        assert_ne!(old_id, new_id);
        assert_eq!(session.id(), new_id);

        // Old session should no longer exist
        assert!(!store.session_exists(&old_id).unwrap());

        // New session should have the data
        let retrieved = store.get_session(&new_id).unwrap().unwrap();
        assert_eq!(retrieved.get_string("key"), Some("value"));
    }

    #[test]
    fn should_get_or_create_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        // Create new session when ID is None
        let session = store.get_or_create(None).unwrap();
        assert!(!session.id().is_empty());

        // Create new session when ID doesn't exist
        let session = store.get_or_create(Some("nonexistent")).unwrap();
        assert_ne!(session.id(), "nonexistent");

        // Get existing session
        let mut existing = store.create_session();
        existing.set("data", "test");
        existing.save().unwrap();
        let id = existing.id().to_string();

        let retrieved = store.get_or_create(Some(&id)).unwrap();
        assert_eq!(retrieved.id(), id);
        assert_eq!(retrieved.get_string("data"), Some("test"));
    }

    #[test]
    fn should_generate_unique_session_ids() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let session1 = store.create_session();
        let session2 = store.create_session();
        let session3 = store.create_session();

        assert_ne!(session1.id(), session2.id());
        assert_ne!(session2.id(), session3.id());
        assert_ne!(session1.id(), session3.id());
    }

    #[test]
    fn should_generate_cookie_string() {
        let config = SessionConfig {
            cookie_name: "my_session".to_string(),
            secure: true,
            http_only: true,
            same_site: SameSite::Strict,
            path: "/api".to_string(),
            domain: Some("example.com".to_string()),
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let session = store.create_session();
        let cookie = session.cookie_string();

        assert!(cookie.contains("my_session="));
        assert!(cookie.contains("Path=/api"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("Domain=example.com"));
    }

    #[test]
    fn should_not_save_unmodified_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        // Save without modifications - should succeed without writing
        session.save().unwrap();

        // Session shouldn't exist in store
        assert!(!store.session_exists(&session_id).unwrap());

        // Modify and save
        session.set("key", "value");
        session.save().unwrap();

        // Now it should exist
        assert!(store.session_exists(&session_id).unwrap());
    }

    #[test]
    fn should_track_session_timestamps() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let created_at = session.created_at();

        // Small delay
        thread::sleep(Duration::from_millis(10));

        session.set("key", "value");
        let last_accessed = session.last_accessed();

        assert!(last_accessed >= created_at);
    }

    #[test]
    fn should_remove_session_data() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("key1", "value1");
        session.set("key2", "value2");

        assert_eq!(session.len(), 2);

        let removed = session.remove("key1");
        assert!(removed.is_some());
        assert_eq!(session.len(), 1);
        assert!(!session.contains_key("key1"));
        assert!(session.contains_key("key2"));

        // Removing non-existent key returns None
        let removed = session.remove("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn should_serialize_and_deserialize_session() {
        let mut data = SessionData::new();
        data.set("string_key", "string_value");
        data.set("int_key", 42i64);
        data.set("bool_key", true);
        data.set("bytes_key", vec![1u8, 2, 3, 4]);

        let serialized = serialize_session(&data);
        let deserialized = deserialize_session(&serialized).unwrap();

        assert_eq!(
            deserialized.get("string_key").unwrap().as_string(),
            Some("string_value")
        );
        assert_eq!(deserialized.get("int_key").unwrap().as_integer(), Some(42));
        assert_eq!(deserialized.get("bool_key").unwrap().as_bool(), Some(true));
        assert_eq!(
            deserialized.get("bytes_key").unwrap().as_bytes(),
            Some(&[1u8, 2, 3, 4][..])
        );
    }

    #[test]
    fn should_handle_empty_session_serialization() {
        let data = SessionData::new();
        let serialized = serialize_session(&data);
        let deserialized = deserialize_session(&serialized).unwrap();

        assert!(deserialized.is_empty());
        assert_eq!(deserialized.len(), 0);
    }

    #[test]
    fn should_share_session_store_across_instances() {
        let config = SessionConfig::default();
        let shared_store = SharedStore::memory();

        let store1 = SessionStore::with_store(config.clone(), shared_store.clone());
        let store2 = SessionStore::with_store(config, shared_store);

        let mut session = store1.create_session();
        let session_id = session.id().to_string();
        session.set("shared", "data");
        session.save().unwrap();

        // Store2 should be able to retrieve the session
        let retrieved = store2.get_session(&session_id).unwrap().unwrap();
        assert_eq!(retrieved.get_string("shared"), Some("data"));
    }

    #[test]
    fn should_isolate_sessions_from_rate_limiter() {
        let shared_store = SharedStore::memory();

        let session_config = SessionConfig::default();
        let session_store = SessionStore::with_store(session_config, shared_store.clone());

        let rate_limit_config = RateLimitConfig {
            enabled: true,
            global: Some(RateLimitPolicy {
                requests_per_window: 5,
                window_secs: 60,
                key_header: None,
            }),
            scoped: vec![],
        };
        let rate_limiter = SharedRateLimiter::with_store(&rate_limit_config, shared_store);

        // Create session
        let mut session = session_store.create_session();
        session.set("user", "test");
        session.save().unwrap();

        // Rate limiter should not affect session data
        let result = rate_limiter.check_global("127.0.0.1", None).unwrap();
        assert!(result.allowed);

        // Session should still be retrievable
        let retrieved = session_store.get_session(session.id()).unwrap().unwrap();
        assert_eq!(retrieved.get_string("user"), Some("test"));
    }

    #[test]
    fn should_default_config() {
        let config = SessionConfig::default();

        assert_eq!(config.cookie_name, "rover_session");
        assert_eq!(config.ttl_secs, 3600);
        assert!(config.secure);
        assert!(config.http_only);
        assert_eq!(config.same_site, SameSite::Lax);
        assert_eq!(config.path, "/");
        assert!(config.domain.is_none());
    }

    #[test]
    fn should_samesite_as_str() {
        assert_eq!(SameSite::Strict.as_str(), "Strict");
        assert_eq!(SameSite::Lax.as_str(), "Lax");
        assert_eq!(SameSite::None.as_str(), "None");
    }

    #[test]
    fn should_return_false_for_nonexistent_delete() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let deleted = store.delete_session("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn should_deserialize_corrupted_data_return_none() {
        // Too short
        assert!(deserialize_session(&[1, 2, 3]).is_none());

        // Valid header but corrupted entries
        let mut data = vec![0u8; 20];
        data[19] = 255; // Invalid entry count will cause failure when trying to parse
        assert!(deserialize_session(&data).is_none());
    }

    #[test]
    fn should_create_cookie_without_optional_domain() {
        let config = SessionConfig {
            cookie_name: "test".to_string(),
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            domain: None,
            path: "/".to_string(),
            ttl_secs: 3600,
        };
        let store = SessionStore::new(config);

        let session = store.create_session();
        let cookie = session.cookie_string();

        assert!(!cookie.contains("Domain="));
        assert!(!cookie.contains("HttpOnly"));
        assert!(!cookie.contains("Secure"));
    }
}
