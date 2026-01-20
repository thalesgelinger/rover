use super::value::SignalValue;

/// Unique identifier for a signal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalId(pub(crate) u32);

/// Arena-based storage for signal values
pub struct SignalArena {
    values: Vec<SignalValue>,
    versions: Vec<u64>,
    free_list: Vec<u32>,
}

impl SignalArena {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            versions: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Create a new signal with the given value
    pub fn create(&mut self, value: SignalValue) -> SignalId {
        if let Some(index) = self.free_list.pop() {
            // Reuse recycled slot
            self.values[index as usize] = value;
            self.versions[index as usize] = 0;
            SignalId(index)
        } else {
            // Allocate new slot
            let index = self.values.len() as u32;
            self.values.push(value);
            self.versions.push(0);
            SignalId(index)
        }
    }

    /// Get the current value of a signal
    pub fn get(&self, id: SignalId) -> &SignalValue {
        &self.values[id.0 as usize]
    }

    /// Set the value of a signal, returns true if value changed
    pub fn set(&mut self, id: SignalId, value: SignalValue) -> bool {
        let idx = id.0 as usize;
        let old_value = &self.values[idx];

        if old_value.eq_value(&value) {
            // Value didn't change
            false
        } else {
            // Value changed, update and bump version
            self.values[idx] = value;
            self.versions[idx] = self.versions[idx].wrapping_add(1);
            true
        }
    }

    /// Get the current version of a signal (for external change detection)
    pub fn version(&self, id: SignalId) -> u64 {
        self.versions[id.0 as usize]
    }

    /// Dispose of a signal, recycling its slot
    pub fn dispose(&mut self, id: SignalId) {
        let idx = id.0 as usize;
        self.values[idx] = SignalValue::Nil;
        self.versions[idx] = 0;
        self.free_list.push(id.0);
    }

    /// Get the total number of allocated signals (including freed ones)
    pub fn capacity(&self) -> usize {
        self.values.len()
    }

    /// Get the number of active signals
    pub fn len(&self) -> usize {
        self.values.len() - self.free_list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SignalArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_signal() {
        let mut arena = SignalArena::new();
        let id = arena.create(SignalValue::Int(42));

        match arena.get(id) {
            SignalValue::Int(42) => {}
            _ => panic!("Expected Int(42)"),
        }
    }

    #[test]
    fn test_set_signal() {
        let mut arena = SignalArena::new();
        let id = arena.create(SignalValue::Int(42));

        let changed = arena.set(id, SignalValue::Int(100));
        assert!(changed);

        match arena.get(id) {
            SignalValue::Int(100) => {}
            _ => panic!("Expected Int(100)"),
        }
    }

    #[test]
    fn test_set_same_value() {
        let mut arena = SignalArena::new();
        let id = arena.create(SignalValue::Int(42));

        let changed = arena.set(id, SignalValue::Int(42));
        assert!(!changed);
    }

    #[test]
    fn test_version_tracking() {
        let mut arena = SignalArena::new();
        let id = arena.create(SignalValue::Int(0));

        let v0 = arena.version(id);
        arena.set(id, SignalValue::Int(1));
        let v1 = arena.version(id);
        arena.set(id, SignalValue::Int(2));
        let v2 = arena.version(id);

        assert_ne!(v0, v1);
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_dispose_and_reuse() {
        let mut arena = SignalArena::new();
        let id1 = arena.create(SignalValue::Int(1));
        let id2 = arena.create(SignalValue::Int(2));

        assert_eq!(arena.len(), 2);

        arena.dispose(id1);
        assert_eq!(arena.len(), 1);

        // Next allocation should reuse id1's slot
        let id3 = arena.create(SignalValue::Int(3));
        assert_eq!(id3, id1);
        assert_eq!(arena.len(), 2);
    }
}
