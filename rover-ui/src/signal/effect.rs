use super::arena::SignalId;
use super::graph::EffectId;
use mlua::RegistryKey;
use smallvec::SmallVec;

/// An effect runs side effects when its dependencies change
pub struct Effect {
    pub(crate) id: EffectId,
    pub(crate) callback: RegistryKey,
    pub(crate) cleanup: Option<RegistryKey>,
    pub(crate) dependencies: SmallVec<[SignalId; 4]>,
}

impl Effect {
    pub fn new(id: EffectId, callback: RegistryKey) -> Self {
        Self {
            id,
            callback,
            cleanup: None,
            dependencies: SmallVec::new(),
        }
    }

    pub fn set_cleanup(&mut self, cleanup: Option<RegistryKey>) {
        self.cleanup = cleanup;
    }

    pub fn cleanup(&self) -> Option<&RegistryKey> {
        self.cleanup.as_ref()
    }

    pub fn callback(&self) -> &RegistryKey {
        &self.callback
    }

    pub fn dependencies(&self) -> &[SignalId] {
        &self.dependencies
    }

    pub fn set_dependencies(&mut self, deps: SmallVec<[SignalId; 4]>) {
        self.dependencies = deps;
    }
}
