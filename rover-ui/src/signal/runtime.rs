use super::arena::{SignalArena, SignalId};
use super::derived::DerivedSignal;
use super::effect::Effect;
use super::graph::{DerivedId, EffectId, SubscriberGraph, SubscriberId};
use super::value::SignalValue;
use mlua::{Function, Lua, RegistryKey, Value};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Circular dependency detected in signal graph")]
    CircularDependency,

    #[error("Lua error: {0}")]
    LuaError(#[from] mlua::Error),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

/// Helper structs for tracking state
struct TrackingState {
    stack: Vec<SubscriberId>,
    reads: Vec<SignalId>,
}

struct BatchState {
    depth: u32,
    dirty_derived: Vec<DerivedId>,
    pending_effects: Vec<EffectId>,
    propagation_stack: Vec<SubscriberId>,
}

/// The main signal runtime that coordinates everything
pub struct SignalRuntime {
    arena: RefCell<SignalArena>,
    graph: RefCell<SubscriberGraph>,
    derived: RefCell<Vec<DerivedSignal>>,
    effects: RefCell<Vec<Effect>>,
    derived_free: RefCell<Vec<u32>>,
    effects_free: RefCell<Vec<u32>>,
    tracking: RefCell<TrackingState>,
    batch: RefCell<BatchState>,
}

impl SignalRuntime {
    pub fn new() -> Self {
        Self {
            arena: RefCell::new(SignalArena::new()),
            graph: RefCell::new(SubscriberGraph::new()),
            derived: RefCell::new(Vec::new()),
            effects: RefCell::new(Vec::new()),
            derived_free: RefCell::new(Vec::new()),
            effects_free: RefCell::new(Vec::new()),
            tracking: RefCell::new(TrackingState {
                stack: Vec::new(),
                reads: Vec::new(),
            }),
            batch: RefCell::new(BatchState {
                depth: 0,
                dirty_derived: Vec::new(),
                pending_effects: Vec::new(),
                propagation_stack: Vec::new(),
            }),
        }
    }

    pub fn create_signal(&self, value: SignalValue) -> SignalId {
        self.arena.borrow_mut().create(value)
    }

    pub fn get_signal(&self, lua: &Lua, id: SignalId) -> mlua::Result<Value> {
        {
            let mut tracking = self.tracking.borrow_mut();
            if !tracking.stack.is_empty() {
                tracking.reads.push(id);
            }
        }
        let arena = self.arena.borrow();
        arena.get(id).to_lua(lua)
    }

    pub fn set_signal(&self, id: SignalId, value: SignalValue) {
        let changed = self.arena.borrow_mut().set(id, value);
        if changed {
            self.notify_subscribers(id);
        }
    }

    pub fn create_derived(&self, compute_fn: RegistryKey) -> DerivedId {
        let id = if let Some(idx) = self.derived_free.borrow_mut().pop() {
            DerivedId(idx)
        } else {
            DerivedId(self.derived.borrow().len() as u32)
        };

        let derived = DerivedSignal::new(id, compute_fn);

        let mut derived_vec = self.derived.borrow_mut();
        if id.0 as usize >= derived_vec.len() {
            derived_vec.push(derived);
        } else {
            derived_vec[id.0 as usize] = derived;
        }
        drop(derived_vec);

        id
    }

    pub fn get_derived(&self, lua: &Lua, id: DerivedId) -> Result<Value> {
        let is_dirty = {
            let derived = self.derived.borrow();
            derived[id.0 as usize].is_dirty()
        };

        if is_dirty {
            self.compute_derived(lua, id)?;
        }

        let derived = self.derived.borrow();
        let value = derived[id.0 as usize].cached_value();
        Ok(value.to_lua(lua)?)
    }

    fn compute_derived(&self, lua: &Lua, id: DerivedId) -> Result<()> {
        let compute_fn: Function = {
            let derived = self.derived.borrow();
            let key = &derived[id.0 as usize].compute_fn;
            lua.registry_value(key)?
        };

        let subscriber = SubscriberId::Derived(id);
        {
            let mut tracking = self.tracking.borrow_mut();
            tracking.stack.push(subscriber);
            tracking.reads.clear();
        }

        let result = compute_fn
            .call::<Value>(())
            .map_err(RuntimeError::LuaError)?;

        let deps = {
            let mut tracking = self.tracking.borrow_mut();
            tracking.stack.pop();

            let mut deps: SmallVec<[SignalId; 4]> = SmallVec::new();
            let mut seen = HashSet::new();
            for &signal in &tracking.reads {
                if seen.insert(signal) {
                    deps.push(signal);
                }
            }
            tracking.reads.clear();
            deps
        };

        let value = SignalValue::from_lua(lua, result)?;

        {
            let mut graph = self.graph.borrow_mut();
            graph.clear_for(subscriber);
            for &signal in &deps {
                graph.subscribe(signal, subscriber);
            }
        }

        {
            let mut derived = self.derived.borrow_mut();
            derived[id.0 as usize].set_cached_value(value);
            derived[id.0 as usize].set_dependencies(deps);
        }

        Ok(())
    }

    pub fn create_effect(&self, lua: &Lua, callback: RegistryKey) -> Result<EffectId> {
        let id = if let Some(idx) = self.effects_free.borrow_mut().pop() {
            EffectId(idx)
        } else {
            EffectId(self.effects.borrow().len() as u32)
        };

        let effect = Effect::new(id, callback);

        let mut effects = self.effects.borrow_mut();
        if id.0 as usize >= effects.len() {
            effects.push(effect);
        } else {
            effects[id.0 as usize] = effect;
        }
        drop(effects);

        self.run_effect(lua, id)?;
        Ok(id)
    }

    pub fn dispose_effect(&self, lua: &Lua, id: EffectId) -> Result<()> {
        // Run cleanup while borrowing effects
        {
            let effects = self.effects.borrow();
            let effect = &effects[id.0 as usize];
            if let Some(ref cleanup_key) = effect.cleanup {
                let cleanup: Function = lua.registry_value(cleanup_key)?;
                cleanup.call::<()>(()).map_err(RuntimeError::LuaError)?;
            }
        }

        let subscriber = SubscriberId::Effect(id);
        self.graph.borrow_mut().clear_for(subscriber);
        self.effects_free.borrow_mut().push(id.0);

        Ok(())
    }

    fn run_effect(&self, lua: &Lua, id: EffectId) -> Result<()> {
        // Run cleanup if present
        {
            let effects = self.effects.borrow();
            let effect = &effects[id.0 as usize];
            if let Some(ref cleanup_key) = effect.cleanup {
                let cleanup: Function = lua.registry_value(cleanup_key)?;
                cleanup.call::<()>(()).map_err(RuntimeError::LuaError)?;
            }
        }

        // Get and call callback
        let subscriber = SubscriberId::Effect(id);
        {
            let mut tracking = self.tracking.borrow_mut();
            tracking.stack.push(subscriber);
            tracking.reads.clear();
        }

        let result = {
            let effects = self.effects.borrow();
            let effect = &effects[id.0 as usize];
            let callback: Function = lua.registry_value(&effect.callback)?;
            callback.call::<Value>(()).map_err(RuntimeError::LuaError)?
        };

        let deps = {
            let mut tracking = self.tracking.borrow_mut();
            tracking.stack.pop();

            let mut deps: SmallVec<[SignalId; 4]> = SmallVec::new();
            let mut seen = HashSet::new();
            for &signal in &tracking.reads {
                if seen.insert(signal) {
                    deps.push(signal);
                }
            }
            tracking.reads.clear();
            deps
        };

        let cleanup = if let Value::Function(f) = result {
            Some(lua.create_registry_value(f)?)
        } else {
            None
        };

        {
            let mut graph = self.graph.borrow_mut();
            graph.clear_for(subscriber);
            for &signal in &deps {
                graph.subscribe(signal, subscriber);
            }
        }

        {
            let mut effects = self.effects.borrow_mut();
            effects[id.0 as usize].set_cleanup(cleanup);
        }

        Ok(())
    }

    pub fn begin_batch(&self) {
        let mut batch = self.batch.borrow_mut();
        batch.depth += 1;
    }

    pub fn end_batch(&self, lua: &Lua) -> Result<()> {
        let mut batch = self.batch.borrow_mut();
        if batch.depth == 0 {
            return Ok(());
        }

        batch.depth -= 1;

        if batch.depth == 0 {
            let mut pending = std::mem::take(&mut batch.pending_effects);
            drop(batch);

            pending.sort_unstable();
            pending.dedup();

            for effect_id in pending {
                self.run_effect(lua, effect_id)?;
            }
        }

        Ok(())
    }

    fn notify_subscribers(&self, signal: SignalId) {
        let subscribers = self.graph.borrow().get_subscribers(signal).to_vec();

        for subscriber in subscribers {
            match subscriber {
                SubscriberId::Derived(id) => self.mark_derived_dirty(id),
                SubscriberId::Effect(id) => self.schedule_effect(id),
            }
        }
    }

    fn mark_derived_dirty(&self, id: DerivedId) {
        {
            let mut batch = self.batch.borrow_mut();
            if batch.propagation_stack.contains(&SubscriberId::Derived(id)) {
                return;
            }
            batch.propagation_stack.push(SubscriberId::Derived(id));
        }

        let is_dirty = {
            let derived = self.derived.borrow();
            derived[id.0 as usize].is_dirty()
        };

        if !is_dirty {
            {
                let mut derived = self.derived.borrow_mut();
                derived[id.0 as usize].mark_dirty();
            }

            {
                let mut batch = self.batch.borrow_mut();
                batch.dirty_derived.push(id);
            }

            let subscribers = {
                let graph = self.graph.borrow();
                graph.get_subscribers(SignalId(id.0)).to_vec()
            };

            for subscriber in subscribers {
                match subscriber {
                    SubscriberId::Derived(child_id) => {
                        let already_dirty = {
                            let d = self.derived.borrow();
                            d[child_id.0 as usize].is_dirty()
                        };

                        if !already_dirty {
                            self.mark_derived_dirty(child_id);
                        }
                    }
                    SubscriberId::Effect(effect_id) => self.schedule_effect(effect_id),
                }
            }
        }

        let mut batch = self.batch.borrow_mut();
        batch.propagation_stack.pop();
    }

    fn schedule_effect(&self, id: EffectId) {
        let mut batch = self.batch.borrow_mut();
        if batch.depth > 0 {
            batch.pending_effects.push(id);
        } else {
            batch.pending_effects.push(id);
        }
    }
}

impl Default for SignalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_signal() {
        let lua = Lua::new();
        let rt = SignalRuntime::new();
        let id = rt.create_signal(SignalValue::Int(42));

        let value = rt.get_signal(&lua, id).unwrap();
        match value {
            Value::Integer(42) => {}
            _ => panic!("Expected Integer(42)"),
        }
    }

    #[test]
    fn test_set_signal() {
        let lua = Lua::new();
        let rt = SignalRuntime::new();
        let id = rt.create_signal(SignalValue::Int(42));

        rt.set_signal(id, SignalValue::Int(100));

        let value = rt.get_signal(&lua, id).unwrap();
        match value {
            Value::Integer(100) => {}
            _ => panic!("Expected Integer(100)"),
        }
    }

    #[test]
    fn test_batch() {
        let lua = Lua::new();
        let rt = SignalRuntime::new();

        rt.begin_batch();
        rt.begin_batch();
        assert_eq!(rt.batch.borrow().depth, 2);

        let _ = rt.end_batch(&lua);
        assert_eq!(rt.batch.borrow().depth, 1);
    }
}
