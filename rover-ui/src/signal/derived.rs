use super::arena::SignalId;
use super::graph::DerivedId;
use super::value::SignalValue;
use mlua::RegistryKey;
use smallvec::SmallVec;

/// A derived signal computes its value from other signals
pub struct DerivedSignal {
    pub(crate) id: DerivedId,
    pub(crate) compute_fn: RegistryKey,
    pub(crate) cached_value: SignalValue,
    pub(crate) dirty: bool,
    pub(crate) dependencies: SmallVec<[SignalId; 4]>,
}

impl DerivedSignal {
    pub fn new(id: DerivedId, compute_fn: RegistryKey) -> Self {
        Self {
            id,
            compute_fn,
            cached_value: SignalValue::Nil,
            dirty: true,
            dependencies: SmallVec::new(),
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn cached_value(&self) -> &SignalValue {
        &self.cached_value
    }

    pub fn set_cached_value(&mut self, value: SignalValue) {
        self.cached_value = value;
        self.dirty = false;
    }

    pub fn dependencies(&self) -> &[SignalId] {
        &self.dependencies
    }

    pub fn set_dependencies(&mut self, deps: SmallVec<[SignalId; 4]>) {
        self.dependencies = deps;
    }
}
