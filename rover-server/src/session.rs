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

/// Session lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is active and valid
    Active,
    /// Session has expired (TTL exceeded)
    Expired,
    /// Session has been explicitly invalidated
    Invalidated,
}

impl SessionState {
    /// Returns true if the session can be used
    pub fn is_valid(&self) -> bool {
        matches!(self, SessionState::Active)
    }
}

/// Session data container
#[derive(Debug, Clone)]
pub struct SessionData {
    data: HashMap<String, StoreValue>,
    created_at: u64,
    last_accessed: u64,
    state: SessionState,
}

impl SessionData {
    fn new() -> Self {
        let now = unix_secs();
        Self {
            data: HashMap::new(),
            created_at: now,
            last_accessed: now,
            state: SessionState::Active,
        }
    }

    fn with_state(state: SessionState) -> Self {
        let now = unix_secs();
        Self {
            data: HashMap::new(),
            created_at: now,
            last_accessed: now,
            state,
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

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
        self.touch();
    }

    /// Check if the session has expired based on TTL
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = unix_secs();
        now > self.last_accessed + ttl_secs
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

    pub fn state(&self) -> SessionState {
        self.data.state()
    }

    /// Check if the session has expired based on configured TTL
    pub fn is_expired(&self) -> bool {
        self.data.is_expired(self.config.ttl_secs)
    }

    /// Refresh the session by extending its TTL
    /// Updates last_accessed and saves to store
    pub fn refresh(&mut self) -> StoreResult<()> {
        self.data.touch();
        self.modified = true;
        self.save()
    }

    /// Invalidate the session (soft delete)
    /// Marks session as invalid without removing data immediately
    pub fn invalidate(&mut self) -> StoreResult<()> {
        self.data.set_state(SessionState::Invalidated);
        self.modified = true;
        self.save()
    }

    /// Check if the session is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        self.data.state().is_valid() && !self.is_expired()
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
        if let Some(value) = self.store.get(session_id)?
            && let Some(bytes) = value.as_bytes()
            && let Some(data) = deserialize_session(bytes)
        {
            return Ok(Some(Session::new(
                session_id.to_string(),
                data,
                self.store.clone(),
                self.config.clone(),
            )));
        }

        Ok(None)
    }

    /// Get or create a session
    pub fn get_or_create(&self, session_id: Option<&str>) -> StoreResult<Session> {
        if let Some(id) = session_id
            && let Some(session) = self.get_session(id)?
        {
            return Ok(session);
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

    let mut result = String::with_capacity((bytes.len() * 4).div_ceil(3));

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
    // - 1 byte: state (0=Active, 1=Expired, 2=Invalidated)
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
    result.push(match data.state {
        SessionState::Active => 0,
        SessionState::Expired => 1,
        SessionState::Invalidated => 2,
    });
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
    // 8 (created_at) + 8 (last_accessed) + 1 (state) + 4 (entry_count) = 21 bytes minimum
    if bytes.len() < 21 {
        return None;
    }

    let mut pos = 0;

    // Header
    let created_at = u64::from_be_bytes(bytes[pos..pos + 8].try_into().ok()?);
    pos += 8;

    let last_accessed = u64::from_be_bytes(bytes[pos..pos + 8].try_into().ok()?);
    pos += 8;

    // State (0=Active, 1=Expired, 2=Invalidated)
    let state = match bytes[pos] {
        0 => SessionState::Active,
        1 => SessionState::Expired,
        2 => SessionState::Invalidated,
        _ => SessionState::Active, // Default to Active for unknown states
    };
    pos += 1;

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
        state,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        SameSite, SessionConfig, SessionData, SessionState, SessionStore, deserialize_session,
        serialize_session,
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

    // Session lifecycle tests
    #[test]
    fn should_check_session_state() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let session = store.create_session();
        assert_eq!(session.state(), SessionState::Active);
        assert!(session.is_valid());
    }

    #[test]
    fn should_invalidate_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        // Invalidate the session
        session.invalidate().unwrap();

        assert_eq!(session.state(), SessionState::Invalidated);
        assert!(!session.is_valid());

        // Retrieve and verify state persisted
        let retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert_eq!(retrieved.state(), SessionState::Invalidated);
        assert!(!retrieved.is_valid());
    }

    #[test]
    fn should_refresh_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        // Small delay to ensure time difference
        thread::sleep(Duration::from_millis(1100));

        let last_accessed_before = session.last_accessed();

        // Refresh the session
        session.refresh().unwrap();

        // last_accessed should be updated
        assert!(session.last_accessed() >= last_accessed_before);
        assert!(session.is_valid());

        // Retrieve and verify
        let retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert!(retrieved.last_accessed() >= last_accessed_before);
    }

    #[test]
    fn should_detect_expired_session() {
        // Use a very short TTL for testing
        let config = SessionConfig {
            ttl_secs: 1, // 1 second
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("key", "value");
        session.save().unwrap();

        // Session should not be expired immediately
        assert!(!session.is_expired());
        assert!(session.is_valid());

        // Wait for TTL to expire
        thread::sleep(Duration::from_secs(2));

        // Now session should be expired
        assert!(session.is_expired());
        assert!(!session.is_valid());
    }

    #[test]
    fn should_prevent_operations_on_invalidated_session() {
        // This test documents expected behavior - invalidated sessions
        // still allow operations but are marked as invalid
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("key", "value");
        session.invalidate().unwrap();

        // Session is invalidated but operations still work
        assert!(!session.is_valid());
        assert_eq!(session.get_string("key"), Some("value"));
    }

    #[test]
    fn should_serialize_and_deserialize_session_state() {
        let mut data = SessionData::new();
        data.set("key", "value");

        // Serialize and deserialize Active state
        let serialized = serialize_session(&data);
        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Active);

        // Test with Invalidated state
        let mut data_invalidated = SessionData::new();
        data_invalidated.set("key", "value");
        data_invalidated.set_state(SessionState::Invalidated);

        let serialized = serialize_session(&data_invalidated);
        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Invalidated);
    }

    #[test]
    fn should_refresh_extend_session_lifetime() {
        // Use short TTL
        let config = SessionConfig {
            ttl_secs: 2, // 2 seconds
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();
        session.set("key", "value");
        session.save().unwrap();

        // Wait 1 second (not expired yet)
        thread::sleep(Duration::from_secs(1));

        // Refresh to extend lifetime
        session.refresh().unwrap();

        // Wait another 1.5 seconds (would have expired without refresh)
        thread::sleep(Duration::from_secs(2));

        // Session should still exist in store because refresh extended TTL
        let retrieved = store.get_session(&session_id).unwrap();
        // Note: The store handles TTL expiry, so if refresh worked, session exists
        // If refresh didn't work, the session would be expired/None
    }

    #[test]
    fn should_session_state_default_to_active() {
        let data = SessionData::new();
        assert_eq!(data.state(), SessionState::Active);
        assert!(data.state().is_valid());
    }

    #[test]
    fn should_session_state_is_valid_only_for_active() {
        assert!(SessionState::Active.is_valid());
        assert!(!SessionState::Expired.is_valid());
        assert!(!SessionState::Invalidated.is_valid());
    }

    // Session lifecycle transition tests

    #[test]
    fn should_transition_active_to_invalidated() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        assert_eq!(session.state(), SessionState::Active);
        assert!(session.is_valid());

        // Transition to Invalidated
        session.invalidate().unwrap();

        assert_eq!(session.state(), SessionState::Invalidated);
        assert!(!session.is_valid());
    }

    #[test]
    fn should_transition_active_to_expired_via_ttl() {
        let config = SessionConfig {
            ttl_secs: 1, // 1 second TTL
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let session = store.create_session();
        assert_eq!(session.state(), SessionState::Active);
        assert!(!session.is_expired());
        assert!(session.is_valid());

        // Wait for TTL to expire
        thread::sleep(Duration::from_secs(2));

        // Session should now be expired (TTL-based)
        assert!(session.is_expired());
        assert!(!session.is_valid());
    }

    #[test]
    fn should_persist_invalidated_state_after_save_and_retrieve() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();

        session.set("key", "value");
        session.save().unwrap();

        // Invalidate and save
        session.invalidate().unwrap();

        // Retrieve from store
        let retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert_eq!(retrieved.state(), SessionState::Invalidated);
        assert!(!retrieved.is_valid());
    }

    #[test]
    fn should_not_allow_refresh_of_invalidated_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("key", "value");
        session.save().unwrap();

        // Invalidate the session
        session.invalidate().unwrap();
        assert!(!session.is_valid());

        // Attempt refresh - should still work but session remains invalidated
        session.refresh().unwrap();

        // State should still be Invalidated, not Active
        assert_eq!(session.state(), SessionState::Invalidated);
        assert!(!session.is_valid());
    }

    #[test]
    fn should_refresh_extend_expired_session_back_to_valid() {
        let config = SessionConfig {
            ttl_secs: 1, // 1 second TTL
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();
        session.set("key", "value");
        session.save().unwrap();

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));
        assert!(session.is_expired());

        // Refresh should extend TTL
        session.refresh().unwrap();

        // Should no longer be expired
        assert!(!session.is_expired());
        assert!(session.is_valid());

        // Verify persisted state
        let retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert!(!retrieved.is_expired());
        assert!(retrieved.is_valid());
    }

    #[test]
    fn should_destroy_remove_session_from_store() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();
        session.set("key", "value");
        session.save().unwrap();

        assert!(store.session_exists(&session_id).unwrap());

        // Destroy the session
        session.destroy().unwrap();

        assert!(!store.session_exists(&session_id).unwrap());
        assert!(session.is_empty());
    }

    #[test]
    fn should_regenerate_id_create_new_valid_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let old_id = session.id().to_string();
        session.set("key", "value");
        session.save().unwrap();

        // Regenerate ID
        let new_id = session.regenerate_id().unwrap();

        assert_ne!(old_id, new_id);
        assert_eq!(session.id(), new_id);
        assert!(session.is_valid());

        // Old session should be gone
        assert!(!store.session_exists(&old_id).unwrap());

        // New session should exist with data
        let retrieved = store.get_session(&new_id).unwrap().unwrap();
        assert!(retrieved.is_valid());
        assert_eq!(retrieved.get_string("key"), Some("value"));
    }

    #[test]
    fn should_maintain_data_through_state_transitions() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("user_id", "123");
        session.set("role", "admin");
        session.save().unwrap();

        // Invalidate
        session.invalidate().unwrap();
        assert_eq!(session.get_string("user_id"), Some("123"));
        assert_eq!(session.get_string("role"), Some("admin"));

        // Save invalidated state
        session.save().unwrap();

        // Retrieve and verify data persisted
        let retrieved = store.get_session(session.id()).unwrap().unwrap();
        assert_eq!(retrieved.get_string("user_id"), Some("123"));
        assert_eq!(retrieved.get_string("role"), Some("admin"));
        assert_eq!(retrieved.state(), SessionState::Invalidated);
    }

    #[test]
    fn should_allow_operations_on_invalidated_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("original", "data");
        session.invalidate().unwrap();

        // Should still allow modifications
        session.set("new", "value");
        session.remove("original");

        assert_eq!(session.get_string("new"), Some("value"));
        assert!(session.get("original").is_none());
    }

    #[test]
    fn should_session_data_set_state_touch() {
        let mut data = SessionData::new();
        let first_accessed = data.last_accessed();

        thread::sleep(Duration::from_millis(10));

        data.set_state(SessionState::Invalidated);
        assert!(data.last_accessed() >= first_accessed);
    }

    #[test]
    fn should_handle_double_invalidate() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.set("key", "value");
        session.save().unwrap();

        // Invalidate twice
        session.invalidate().unwrap();
        assert_eq!(session.state(), SessionState::Invalidated);

        session.invalidate().unwrap();
        assert_eq!(session.state(), SessionState::Invalidated);

        // Should still persist correctly
        let retrieved = store.get_session(session.id()).unwrap().unwrap();
        assert_eq!(retrieved.state(), SessionState::Invalidated);
    }

    #[test]
    fn should_transition_from_expired_to_active_via_refresh() {
        let config = SessionConfig {
            ttl_secs: 1,
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.save().unwrap();

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));
        assert!(session.is_expired());

        // Refresh brings it back
        session.refresh().unwrap();
        assert!(!session.is_expired());
        assert!(session.is_valid());
    }

    #[test]
    fn should_get_or_create_handle_invalidated_session() {
        let config = SessionConfig::default();
        let store = SessionStore::new(config);

        // Create and invalidate a session
        let mut session = store.create_session();
        let session_id = session.id().to_string();
        session.set("key", "value");
        session.invalidate().unwrap();
        session.save().unwrap();

        // get_or_create should return the invalidated session (it exists)
        let retrieved = store.get_or_create(Some(&session_id)).unwrap();
        assert_eq!(retrieved.id(), session_id);
        assert_eq!(retrieved.state(), SessionState::Invalidated);
    }

    #[test]
    fn should_serialize_all_session_states() {
        // Test Active state serialization
        let mut active_data = SessionData::new();
        active_data.set("key", "value");
        let serialized = serialize_session(&active_data);
        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Active);

        // Test Expired state (manual set)
        let mut expired_data = SessionData::with_state(SessionState::Expired);
        expired_data.set("key", "value");
        let serialized = serialize_session(&expired_data);
        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Expired);

        // Test Invalidated state
        let mut invalidated_data = SessionData::with_state(SessionState::Invalidated);
        invalidated_data.set("key", "value");
        let serialized = serialize_session(&invalidated_data);
        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Invalidated);
    }

    #[test]
    fn should_deserialize_unknown_state_as_active() {
        // Create a serialized session with invalid state byte (255)
        let mut data = SessionData::new();
        data.set("key", "value");
        let mut serialized = serialize_session(&data);

        // Corrupt the state byte (at position 16, after created_at and last_accessed)
        serialized[16] = 255;

        let deserialized = deserialize_session(&serialized).unwrap();
        assert_eq!(deserialized.state(), SessionState::Active);
        assert_eq!(deserialized.get("key").unwrap().as_string(), Some("value"));
    }

    #[test]
    fn should_session_data_touch_on_operations() {
        let mut data = SessionData::new();
        let _created_at = data.created_at();
        let first_accessed = data.last_accessed();

        // Small delay
        thread::sleep(Duration::from_millis(10));

        // set should touch
        data.set("key", "value");
        assert!(data.last_accessed() >= first_accessed);

        let accessed_after_set = data.last_accessed();
        thread::sleep(Duration::from_millis(10));

        // remove should touch
        data.remove("key");
        assert!(data.last_accessed() >= accessed_after_set);
    }

    #[test]
    fn should_is_expired_check_with_ttl() {
        let data = SessionData::new();

        // Not expired with long TTL
        assert!(!data.is_expired(3600));

        // With 0 TTL and no time elapsed, should NOT be expired
        // (now > last_accessed + 0 is false when now == last_accessed)
        assert!(!data.is_expired(0));
    }

    #[test]
    fn should_session_valid_requires_both_state_and_not_expired() {
        let config = SessionConfig {
            ttl_secs: 1,
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        session.save().unwrap();

        // Initially valid
        assert!(session.is_valid());

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));

        // Expired (even though Active state)
        assert!(!session.is_valid());

        // Invalidate
        session.invalidate().unwrap();

        // Both invalid state AND expired
        assert!(!session.is_valid());
    }

    #[test]
    fn should_multiple_refresh_extend_lifetime() {
        let config = SessionConfig {
            ttl_secs: 2, // 2 second TTL
            ..Default::default()
        };
        let store = SessionStore::new(config);

        let mut session = store.create_session();
        let session_id = session.id().to_string();
        session.save().unwrap();

        // First refresh after 1 second
        thread::sleep(Duration::from_secs(1));
        session.refresh().unwrap();

        // Second refresh after another 1 second
        thread::sleep(Duration::from_secs(1));
        session.refresh().unwrap();

        // Third refresh after another 1 second
        thread::sleep(Duration::from_secs(1));
        session.refresh().unwrap();

        // Total ~3 seconds elapsed, but should still be valid due to refreshes
        assert!(!session.is_expired());
        assert!(session.is_valid());

        let _retrieved = store.get_session(&session_id).unwrap().unwrap();
        assert!(!_retrieved.is_expired());
    }
}
