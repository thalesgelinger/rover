use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Error types for store operations
#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    /// Key not found
    NotFound,
    /// Store is unavailable (for external backends)
    Unavailable,
    /// Operation timed out
    Timeout,
    /// Serialization error
    Serialization(String),
    /// Other errors
    Other(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound => write!(f, "key not found"),
            StoreError::Unavailable => write!(f, "store unavailable"),
            StoreError::Timeout => write!(f, "operation timed out"),
            StoreError::Serialization(msg) => write!(f, "serialization error: {}", msg),
            StoreError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StoreError {}

/// Result type for store operations
pub type StoreResult<T> = Result<T, StoreError>;

/// Value stored in the store
#[derive(Debug, Clone, PartialEq)]
pub enum StoreValue {
    String(String),
    Bytes(Vec<u8>),
    Integer(i64),
    Boolean(bool),
}

impl StoreValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            StoreValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            StoreValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            StoreValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            StoreValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

impl From<String> for StoreValue {
    fn from(s: String) -> Self {
        StoreValue::String(s)
    }
}

impl From<&str> for StoreValue {
    fn from(s: &str) -> Self {
        StoreValue::String(s.to_string())
    }
}

impl From<Vec<u8>> for StoreValue {
    fn from(v: Vec<u8>) -> Self {
        StoreValue::Bytes(v)
    }
}

impl From<i64> for StoreValue {
    fn from(i: i64) -> Self {
        StoreValue::Integer(i)
    }
}

impl From<bool> for StoreValue {
    fn from(b: bool) -> Self {
        StoreValue::Boolean(b)
    }
}

/// Entry in the store with optional expiration
#[derive(Debug, Clone)]
struct StoreEntry {
    value: StoreValue,
    expires_at: Option<Instant>,
}

impl StoreEntry {
    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Instant::now() >= exp)
            .unwrap_or(false)
    }
}

/// Core store trait for pluggable backends
pub trait StoreBackend: Send + Sync {
    /// Get a value by key
    fn get(&self, key: &str) -> StoreResult<Option<StoreValue>>;

    /// Set a value with optional TTL
    fn set(&self, key: &str, value: StoreValue, ttl: Option<Duration>) -> StoreResult<()>;

    /// Delete a key
    fn delete(&self, key: &str) -> StoreResult<bool>;

    /// Check if a key exists
    fn exists(&self, key: &str) -> StoreResult<bool>;

    /// Increment a counter
    fn increment(&self, key: &str, delta: i64) -> StoreResult<i64>;

    /// Decrement a counter
    fn decrement(&self, key: &str, delta: i64) -> StoreResult<i64>;

    /// Flush all keys (mainly for testing)
    fn flush(&self) -> StoreResult<()>;
}

/// In-memory store implementation
pub struct MemoryStore {
    data: Mutex<HashMap<String, StoreEntry>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self) {
        let mut data = self.data.lock().unwrap();
        data.retain(|_, entry| !entry.is_expired());
    }

    /// Get the number of entries (for testing)
    pub fn len(&self) -> usize {
        let data = self.data.lock().unwrap();
        data.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreBackend for MemoryStore {
    fn get(&self, key: &str) -> StoreResult<Option<StoreValue>> {
        let data = self.data.lock().unwrap();

        if let Some(entry) = data.get(key) {
            if entry.is_expired() {
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }

        Ok(None)
    }

    fn set(&self, key: &str, value: StoreValue, ttl: Option<Duration>) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();

        let expires_at = ttl.map(|duration| Instant::now() + duration);

        data.insert(key.to_string(), StoreEntry { value, expires_at });

        Ok(())
    }

    fn delete(&self, key: &str) -> StoreResult<bool> {
        let mut data = self.data.lock().unwrap();
        Ok(data.remove(key).is_some())
    }

    fn exists(&self, key: &str) -> StoreResult<bool> {
        let data = self.data.lock().unwrap();

        if let Some(entry) = data.get(key) {
            return Ok(!entry.is_expired());
        }

        Ok(false)
    }

    fn increment(&self, key: &str, delta: i64) -> StoreResult<i64> {
        let mut data = self.data.lock().unwrap();

        let entry = data.get_mut(key);

        match entry {
            Some(entry) if !entry.is_expired() => match &mut entry.value {
                StoreValue::Integer(val) => {
                    *val += delta;
                    Ok(*val)
                }
                _ => Err(StoreError::Other(
                    "cannot increment non-integer value".to_string(),
                )),
            },
            _ => {
                // Key doesn't exist or is expired - create new
                data.insert(
                    key.to_string(),
                    StoreEntry {
                        value: StoreValue::Integer(delta),
                        expires_at: None,
                    },
                );
                Ok(delta)
            }
        }
    }

    fn decrement(&self, key: &str, delta: i64) -> StoreResult<i64> {
        self.increment(key, -delta)
    }

    fn flush(&self) -> StoreResult<()> {
        let mut data = self.data.lock().unwrap();
        data.clear();
        Ok(())
    }
}

/// Store with namespace support and fallback semantics
#[derive(Clone)]
pub struct NamespacedStore {
    backend: Arc<dyn StoreBackend>,
    namespace: String,
    fallback_on_error: bool,
}

impl std::fmt::Debug for NamespacedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamespacedStore")
            .field("namespace", &self.namespace)
            .field("fallback_on_error", &self.fallback_on_error)
            .finish_non_exhaustive()
    }
}

impl NamespacedStore {
    pub fn new(
        backend: Arc<dyn StoreBackend>,
        namespace: impl Into<String>,
        fallback_on_error: bool,
    ) -> Self {
        Self {
            backend,
            namespace: namespace.into(),
            fallback_on_error,
        }
    }

    fn namespaced_key(&self, key: &str) -> String {
        format!("{}:{}", self.namespace, key)
    }

    fn handle_error<T>(&self, result: StoreResult<T>) -> StoreResult<T> {
        if self.fallback_on_error {
            // In fallback mode, treat errors as cache misses
            match result {
                Err(StoreError::Unavailable) | Err(StoreError::Timeout) => {
                    Err(StoreError::NotFound)
                }
                other => other,
            }
        } else {
            result
        }
    }

    pub fn get(&self, key: &str) -> StoreResult<Option<StoreValue>> {
        let ns_key = self.namespaced_key(key);
        self.handle_error(self.backend.get(&ns_key))
    }

    pub fn set(&self, key: &str, value: StoreValue, ttl: Option<Duration>) -> StoreResult<()> {
        let ns_key = self.namespaced_key(key);
        self.backend.set(&ns_key, value, ttl)
    }

    pub fn delete(&self, key: &str) -> StoreResult<bool> {
        let ns_key = self.namespaced_key(key);
        self.backend.delete(&ns_key)
    }

    pub fn exists(&self, key: &str) -> StoreResult<bool> {
        let ns_key = self.namespaced_key(key);
        self.handle_error(self.backend.exists(&ns_key))
    }

    pub fn increment(&self, key: &str, delta: i64) -> StoreResult<i64> {
        let ns_key = self.namespaced_key(key);
        self.backend.increment(&ns_key, delta)
    }

    pub fn decrement(&self, key: &str, delta: i64) -> StoreResult<i64> {
        let ns_key = self.namespaced_key(key);
        self.backend.decrement(&ns_key, delta)
    }

    pub fn with_subnamespace(&self, sub: impl AsRef<str>) -> NamespacedStore {
        let new_namespace = format!("{}:{}", self.namespace, sub.as_ref());
        NamespacedStore::new(self.backend.clone(), new_namespace, self.fallback_on_error)
    }
}

/// Shared store that can be used across instances
#[derive(Clone)]
pub struct SharedStore {
    inner: Arc<dyn StoreBackend>,
}

impl SharedStore {
    pub fn new(backend: Arc<dyn StoreBackend>) -> Self {
        Self { inner: backend }
    }

    pub fn memory() -> Self {
        Self::new(Arc::new(MemoryStore::new()))
    }

    pub fn namespace(&self, ns: impl Into<String>) -> NamespacedStore {
        NamespacedStore::new(self.inner.clone(), ns, true)
    }

    pub fn namespace_strict(&self, ns: impl Into<String>) -> NamespacedStore {
        NamespacedStore::new(self.inner.clone(), ns, false)
    }

    pub fn get(&self, key: &str) -> StoreResult<Option<StoreValue>> {
        self.inner.get(key)
    }

    pub fn set(&self, key: &str, value: StoreValue, ttl: Option<Duration>) -> StoreResult<()> {
        self.inner.set(key, value, ttl)
    }

    pub fn delete(&self, key: &str) -> StoreResult<bool> {
        self.inner.delete(key)
    }

    pub fn exists(&self, key: &str) -> StoreResult<bool> {
        self.inner.exists(key)
    }

    pub fn increment(&self, key: &str, delta: i64) -> StoreResult<i64> {
        self.inner.increment(key, delta)
    }

    pub fn decrement(&self, key: &str, delta: i64) -> StoreResult<i64> {
        self.inner.decrement(key, delta)
    }

    pub fn flush(&self) -> StoreResult<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn should_store_and_retrieve_string_value() {
        let store = MemoryStore::new();

        store.set("key1", "hello".into(), None).unwrap();

        let value = store.get("key1").unwrap();
        assert_eq!(value, Some(StoreValue::String("hello".to_string())));
    }

    #[test]
    fn should_store_and_retrieve_bytes_value() {
        let store = MemoryStore::new();

        store.set("key1", vec![1u8, 2, 3].into(), None).unwrap();

        let value = store.get("key1").unwrap();
        assert_eq!(value, Some(StoreValue::Bytes(vec![1, 2, 3])));
    }

    #[test]
    fn should_store_and_retrieve_integer_value() {
        let store = MemoryStore::new();

        store.set("key1", 42i64.into(), None).unwrap();

        let value = store.get("key1").unwrap();
        assert_eq!(value, Some(StoreValue::Integer(42)));
    }

    #[test]
    fn should_store_and_retrieve_boolean_value() {
        let store = MemoryStore::new();

        store.set("key1", true.into(), None).unwrap();

        let value = store.get("key1").unwrap();
        assert_eq!(value, Some(StoreValue::Boolean(true)));
    }

    #[test]
    fn should_return_none_for_missing_key() {
        let store = MemoryStore::new();

        let value = store.get("missing").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn should_delete_key() {
        let store = MemoryStore::new();

        store.set("key1", "value".into(), None).unwrap();
        assert!(store.exists("key1").unwrap());

        let deleted = store.delete("key1").unwrap();
        assert!(deleted);
        assert!(!store.exists("key1").unwrap());
    }

    #[test]
    fn should_delete_return_false_for_missing_key() {
        let store = MemoryStore::new();

        let deleted = store.delete("missing").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn should_expire_key_after_ttl() {
        let store = MemoryStore::new();

        store
            .set("key1", "value".into(), Some(Duration::from_millis(50)))
            .unwrap();

        assert!(store.exists("key1").unwrap());

        sleep(Duration::from_millis(60));

        assert!(!store.exists("key1").unwrap());
        assert_eq!(store.get("key1").unwrap(), None);
    }

    #[test]
    fn should_not_expire_without_ttl() {
        let store = MemoryStore::new();

        store.set("key1", "value".into(), None).unwrap();

        sleep(Duration::from_millis(50));

        assert!(store.exists("key1").unwrap());
    }

    #[test]
    fn should_increment_counter() {
        let store = MemoryStore::new();

        let val = store.increment("counter", 1).unwrap();
        assert_eq!(val, 1);

        let val = store.increment("counter", 5).unwrap();
        assert_eq!(val, 6);
    }

    #[test]
    fn should_decrement_counter() {
        let store = MemoryStore::new();

        store.set("counter", 10i64.into(), None).unwrap();

        let val = store.decrement("counter", 3).unwrap();
        assert_eq!(val, 7);
    }

    #[test]
    fn should_increment_create_if_missing() {
        let store = MemoryStore::new();

        let val = store.increment("new_counter", 5).unwrap();
        assert_eq!(val, 5);
    }

    #[test]
    fn should_error_on_increment_non_integer() {
        let store = MemoryStore::new();

        store.set("key1", "string".into(), None).unwrap();

        let result = store.increment("key1", 1);
        assert!(result.is_err());
    }

    #[test]
    fn should_cleanup_expired_entries() {
        let store = MemoryStore::new();

        store
            .set("key1", "value1".into(), Some(Duration::from_millis(10)))
            .unwrap();
        store.set("key2", "value2".into(), None).unwrap();
        store
            .set("key3", "value3".into(), Some(Duration::from_millis(10)))
            .unwrap();

        assert_eq!(store.len(), 3);

        sleep(Duration::from_millis(20));
        store.cleanup_expired();

        assert_eq!(store.len(), 1);
        assert!(store.exists("key2").unwrap());
    }

    #[test]
    fn should_flush_all_keys() {
        let store = MemoryStore::new();

        store.set("key1", "value1".into(), None).unwrap();
        store.set("key2", "value2".into(), None).unwrap();

        store.flush().unwrap();

        assert!(!store.exists("key1").unwrap());
        assert!(!store.exists("key2").unwrap());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn should_namespace_keys() {
        let store = Arc::new(MemoryStore::new());
        let ns_store = NamespacedStore::new(store.clone(), "session", false);

        ns_store.set("user1", "data".into(), None).unwrap();

        // Should be accessible via namespaced store
        let value = ns_store.get("user1").unwrap();
        assert_eq!(value, Some(StoreValue::String("data".to_string())));

        // Should not be accessible without namespace
        let value = store.get("user1").unwrap();
        assert_eq!(value, None);

        // Should be accessible with full key
        let value = store.get("session:user1").unwrap();
        assert_eq!(value, Some(StoreValue::String("data".to_string())));
    }

    #[test]
    fn should_isolate_namespaces() {
        let store = Arc::new(MemoryStore::new());
        let ns1 = NamespacedStore::new(store.clone(), "ns1", false);
        let ns2 = NamespacedStore::new(store.clone(), "ns2", false);

        ns1.set("key", "value1".into(), None).unwrap();
        ns2.set("key", "value2".into(), None).unwrap();

        assert_eq!(
            ns1.get("key").unwrap(),
            Some(StoreValue::String("value1".to_string()))
        );
        assert_eq!(
            ns2.get("key").unwrap(),
            Some(StoreValue::String("value2".to_string()))
        );
    }

    #[test]
    fn should_create_subnamespace() {
        let store = Arc::new(MemoryStore::new());
        let ns = NamespacedStore::new(store, "app", false);
        let sub = ns.with_subnamespace("session");

        sub.set("key", "value".into(), None).unwrap();

        let value = sub.get("key").unwrap();
        assert_eq!(value, Some(StoreValue::String("value".to_string())));
    }

    #[test]
    fn should_shared_store_work_with_namespace() {
        let shared = SharedStore::memory();
        let ns = shared.namespace("ratelimit");

        ns.set("ip:127.0.0.1", 100i64.into(), None).unwrap();

        let value = ns.get("ip:127.0.0.1").unwrap();
        assert_eq!(value, Some(StoreValue::Integer(100)));
    }

    #[test]
    fn should_namespace_increment_work() {
        let store = Arc::new(MemoryStore::new());
        let ns = NamespacedStore::new(store, "counters", false);

        let val = ns.increment("requests", 1).unwrap();
        assert_eq!(val, 1);

        let val = ns.increment("requests", 1).unwrap();
        assert_eq!(val, 2);
    }

    #[test]
    fn should_handle_expired_entry_on_get() {
        let store = MemoryStore::new();

        store
            .set("key1", "value".into(), Some(Duration::from_millis(10)))
            .unwrap();

        sleep(Duration::from_millis(20));

        // Should return None for expired key
        let value = store.get("key1").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn should_namespace_delete_work() {
        let store = Arc::new(MemoryStore::new());
        let ns = NamespacedStore::new(store, "test", false);

        ns.set("key1", "value".into(), None).unwrap();
        assert!(ns.exists("key1").unwrap());

        ns.delete("key1").unwrap();
        assert!(!ns.exists("key1").unwrap());
    }

    #[test]
    fn should_namespace_decrement_work() {
        let store = Arc::new(MemoryStore::new());
        let ns = NamespacedStore::new(store, "test", false);

        ns.set("counter", 10i64.into(), None).unwrap();

        let val = ns.decrement("counter", 3).unwrap();
        assert_eq!(val, 7);
    }

    #[test]
    fn should_adapter_trait_allow_polymorphic_usage() {
        fn use_backend(backend: &dyn StoreBackend) -> StoreResult<Option<StoreValue>> {
            backend.set("key", "value".into(), None)?;
            backend.get("key")
        }

        let store = MemoryStore::new();
        let result = use_backend(&store).unwrap();
        assert_eq!(result, Some(StoreValue::String("value".to_string())));
    }

    #[test]
    fn should_adapter_work_with_arc_dyn() {
        let backend: Arc<dyn StoreBackend> = Arc::new(MemoryStore::new());
        let ns = NamespacedStore::new(backend, "test", false);

        ns.set("key", "value".into(), None).unwrap();
        let result = ns.get("key").unwrap();
        assert_eq!(result, Some(StoreValue::String("value".to_string())));
    }

    #[test]
    fn should_adapter_trait_maintain_consistency() {
        let store = MemoryStore::new();

        // Set and verify
        store.set("key1", "value1".into(), None).unwrap();
        assert_eq!(
            store.get("key1").unwrap(),
            Some(StoreValue::String("value1".to_string()))
        );

        // Update same key
        store.set("key1", "value2".into(), None).unwrap();
        assert_eq!(
            store.get("key1").unwrap(),
            Some(StoreValue::String("value2".to_string()))
        );

        // Delete and verify
        assert!(store.delete("key1").unwrap());
        assert_eq!(store.get("key1").unwrap(), None);

        // Delete non-existent
        assert!(!store.delete("key1").unwrap());
    }

    #[test]
    fn should_adapter_trait_handle_all_operations() {
        let store = MemoryStore::new();

        // get
        assert_eq!(store.get("missing").unwrap(), None);

        // set
        store.set("key", "value".into(), None).unwrap();

        // exists
        assert!(store.exists("key").unwrap());
        assert!(!store.exists("missing").unwrap());

        // increment
        assert_eq!(store.increment("counter", 5).unwrap(), 5);
        assert_eq!(store.increment("counter", 3).unwrap(), 8);

        // decrement
        assert_eq!(store.decrement("counter", 2).unwrap(), 6);

        // delete
        assert!(store.delete("key").unwrap());
        assert!(!store.exists("key").unwrap());

        // flush
        store.set("k1", "v1".into(), None).unwrap();
        store.set("k2", "v2".into(), None).unwrap();
        store.flush().unwrap();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn should_adapter_trait_provide_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MemoryStore>();
        assert_send_sync::<NamespacedStore>();
        assert_send_sync::<SharedStore>();
    }

    #[test]
    fn should_ttl_expire_with_zero_duration() {
        let store = MemoryStore::new();

        // Set with zero TTL should expire immediately or very quickly
        store
            .set("key1", "value".into(), Some(Duration::from_nanos(1)))
            .unwrap();

        // Small sleep to ensure expiry
        sleep(Duration::from_millis(1));

        assert!(!store.exists("key1").unwrap());
        assert_eq!(store.get("key1").unwrap(), None);
    }

    #[test]
    fn should_ttl_preserve_non_expired_keys() {
        let store = MemoryStore::new();

        // Set multiple keys with different TTLs (use longer durations for reliability)
        store
            .set("short", "s".into(), Some(Duration::from_millis(100)))
            .unwrap();
        store
            .set("medium", "m".into(), Some(Duration::from_millis(500)))
            .unwrap();
        store
            .set("long", "l".into(), Some(Duration::from_secs(10)))
            .unwrap();
        store.set("forever", "f".into(), None).unwrap();

        // Wait for short to expire
        sleep(Duration::from_millis(200));

        assert!(!store.exists("short").unwrap());
        assert!(store.exists("medium").unwrap());
        assert!(store.exists("long").unwrap());
        assert!(store.exists("forever").unwrap());

        // Wait for medium to expire
        sleep(Duration::from_millis(400));

        assert!(!store.exists("medium").unwrap());
        assert!(store.exists("long").unwrap());
        assert!(store.exists("forever").unwrap());
    }

    #[test]
    fn should_ttl_extend_on_update() {
        let store = MemoryStore::new();

        // Set with short TTL
        store
            .set("key", "value1".into(), Some(Duration::from_millis(200)))
            .unwrap();

        // Wait but not long enough to expire
        sleep(Duration::from_millis(100));

        // Update with longer TTL
        store
            .set("key", "value2".into(), Some(Duration::from_millis(500)))
            .unwrap();

        // Wait past original TTL
        sleep(Duration::from_millis(150));

        // Should still exist because TTL was extended
        assert!(store.exists("key").unwrap());
        assert_eq!(
            store.get("key").unwrap(),
            Some(StoreValue::String("value2".to_string()))
        );
    }

    #[test]
    fn should_namespace_isolate_operations() {
        let backend = Arc::new(MemoryStore::new());
        let ns1 = NamespacedStore::new(backend.clone(), "ns1", false);
        let ns2 = NamespacedStore::new(backend.clone(), "ns2", false);
        let ns3 = NamespacedStore::new(backend.clone(), "ns3", false);

        // Perform different operations in different namespaces
        ns1.set("shared_key", "ns1_value".into(), None).unwrap();
        ns2.set("shared_key", "ns2_value".into(), None).unwrap();
        ns3.set("other_key", "ns3_value".into(), None).unwrap();

        // Verify isolation
        assert_eq!(
            ns1.get("shared_key").unwrap(),
            Some(StoreValue::String("ns1_value".to_string()))
        );
        assert_eq!(
            ns2.get("shared_key").unwrap(),
            Some(StoreValue::String("ns2_value".to_string()))
        );
        assert_eq!(
            ns3.get("other_key").unwrap(),
            Some(StoreValue::String("ns3_value".to_string()))
        );

        // Verify cross-namespace inaccessibility
        assert_eq!(ns1.get("other_key").unwrap(), None);
        assert_eq!(ns2.get("other_key").unwrap(), None);
        assert_eq!(ns3.get("shared_key").unwrap(), None);

        // Verify counters are isolated
        ns1.increment("counter", 5).unwrap();
        ns2.increment("counter", 10).unwrap();

        assert_eq!(ns1.get("counter").unwrap().unwrap().as_integer(), Some(5));
        assert_eq!(ns2.get("counter").unwrap().unwrap().as_integer(), Some(10));
    }

    #[test]
    fn should_namespace_delete_isolated() {
        let backend = Arc::new(MemoryStore::new());
        let ns1 = NamespacedStore::new(backend.clone(), "ns1", false);
        let ns2 = NamespacedStore::new(backend.clone(), "ns2", false);

        ns1.set("key", "value1".into(), None).unwrap();
        ns2.set("key", "value2".into(), None).unwrap();

        // Delete from ns1 only
        assert!(ns1.delete("key").unwrap());

        // ns2 should still have the key
        assert_eq!(
            ns2.get("key").unwrap(),
            Some(StoreValue::String("value2".to_string()))
        );

        // ns1 should not
        assert_eq!(ns1.get("key").unwrap(), None);
    }

    #[test]
    fn should_namespace_ttl_isolated() {
        let backend = Arc::new(MemoryStore::new());
        let ns1 = NamespacedStore::new(backend.clone(), "ns1", false);
        let ns2 = NamespacedStore::new(backend.clone(), "ns2", false);

        // Same key, different TTLs
        ns1.set("key", "value1".into(), Some(Duration::from_millis(50)))
            .unwrap();
        ns2.set("key", "value2".into(), Some(Duration::from_secs(10)))
            .unwrap();

        // Wait for ns1's TTL to expire
        sleep(Duration::from_millis(100));

        // ns1's key should be expired
        assert!(!ns1.exists("key").unwrap());

        // ns2's key should still exist
        assert!(ns2.exists("key").unwrap());
    }

    #[test]
    fn should_namespaced_store_handle_errors_with_fallback() {
        let backend = Arc::new(MemoryStore::new());
        // Create with fallback enabled
        let ns_fallback = NamespacedStore::new(backend.clone(), "test", true);
        // Create with fallback disabled
        let ns_strict = NamespacedStore::new(backend, "test", false);

        // Both should work normally for successful operations
        ns_fallback.set("key", "value".into(), None).unwrap();
        ns_strict.set("key2", "value2".into(), None).unwrap();

        assert!(ns_fallback.exists("key").unwrap());
        assert!(ns_strict.exists("key2").unwrap());
    }

    #[test]
    fn should_shared_store_adapter_implement_all_operations() {
        let shared = SharedStore::memory();

        // Test all StoreBackend operations through SharedStore
        shared.set("string", "hello".into(), None).unwrap();
        shared.set("int", 42i64.into(), None).unwrap();
        shared.set("bool", true.into(), None).unwrap();
        shared.set("bytes", vec![1u8, 2, 3].into(), None).unwrap();

        assert_eq!(
            shared.get("string").unwrap(),
            Some(StoreValue::String("hello".to_string()))
        );
        assert_eq!(shared.get("int").unwrap(), Some(StoreValue::Integer(42)));
        assert_eq!(shared.get("bool").unwrap(), Some(StoreValue::Boolean(true)));
        assert_eq!(
            shared.get("bytes").unwrap(),
            Some(StoreValue::Bytes(vec![1, 2, 3]))
        );

        assert!(shared.exists("string").unwrap());

        assert_eq!(shared.increment("counter", 5).unwrap(), 5);
        assert_eq!(shared.decrement("counter", 2).unwrap(), 3);

        assert!(shared.delete("string").unwrap());
        assert!(!shared.exists("string").unwrap());
    }
}
