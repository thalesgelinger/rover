use super::arena::{SignalArena, SignalId};
use super::derived::DerivedSignal;
use super::effect::Effect;
use super::graph::{DerivedId, EffectId, SubscriberGraph, SubscriberId};
use super::value::SignalValue;
use crate::node::{Node, NodeArena, NodeId, RenderCommand, SignalOrDerived, TextContent};
use crate::platform::tui::PlatformEvent;
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
    pending_node_updates: Vec<NodeId>,
    propagation_stack: Vec<SubscriberId>,
    render_commands: Vec<RenderCommand>,
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
    node_bindings: RefCell<Vec<(SignalOrDerived, NodeId)>>,
    pub node_arena: RefCell<NodeArena>,
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
                pending_node_updates: Vec::new(),
                propagation_stack: Vec::new(),
                render_commands: Vec::new(),
            }),
            node_bindings: RefCell::new(Vec::new()),
            node_arena: RefCell::new(NodeArena::new()),
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

    pub fn read_signal_display(&self, id: SignalId) -> Option<String> {
        let arena = self.arena.borrow();
        Some(arena.get(id).to_display_string())
    }

    pub fn read_signal_bool(&self, id: SignalId) -> Option<bool> {
        let arena = self.arena.borrow();
        arena.get(id).as_boolean()
    }

    /// Check if there are pending updates that need processing
    /// Returns true if the platform should call process_node_updates() and take_render_commands()
    pub fn tick(&self) -> bool {
        let batch = self.batch.borrow();
        !batch.pending_node_updates.is_empty() || !batch.render_commands.is_empty()
    }

    pub fn read_derived_display(&self, id: DerivedId) -> Option<String> {
        let derived = self.derived.borrow();
        derived
            .get(id.0 as usize)
            .map(|d| d.cached_value().to_display_string())
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
                SubscriberId::Node(node) => self.schedule_node_update(node),
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
                    SubscriberId::Node(_) => {}
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

    pub fn subscribe_node(&self, source: SignalOrDerived, node: NodeId) {
        let subscriber = SubscriberId::Node(node);
        match source {
            SignalOrDerived::Signal(signal_id) => {
                self.graph.borrow_mut().subscribe(signal_id, subscriber);
            }
            SignalOrDerived::Derived(derived_id) => {
                // Derived signals use SignalId in the graph (using DerivedId.0 as SignalId.0)
                self.graph.borrow_mut().subscribe(SignalId(derived_id.0), subscriber);
            }
        }
        self.node_bindings.borrow_mut().push((source, node));

        // Schedule initial update for the node
        self.schedule_node_update(node);
    }

    pub fn schedule_node_update(&self, node: NodeId) {
        let mut batch = self.batch.borrow_mut();
        if !batch.pending_node_updates.contains(&node) {
            batch.pending_node_updates.push(node);
        }
    }

    pub fn push_render_command(&self, command: RenderCommand) {
        self.batch.borrow_mut().render_commands.push(command);
    }

    /// Process all pending node updates and generate render commands
    pub fn process_node_updates(&self) {
        let pending = {
            let mut batch = self.batch.borrow_mut();
            std::mem::take(&mut batch.pending_node_updates)
        };

        for node_id in pending {
            // First, read the node info we need
            let node_info = {
                let arena = self.node_arena.borrow();
                arena.get(node_id).map(|node| match node {
                    Node::Text(text_node) => {
                        let content = text_node.content.clone();
                        (0, content, None, None, None, false)
                    }
                    Node::Conditional(cond_node) => {
                        (1, TextContent::Static("".into()), Some(cond_node.condition_signal), cond_node.true_branch, cond_node.false_branch, cond_node.visible)
                    }
                    Node::Each(_) => (2, TextContent::Static("".into()), None, None, None, false),
                    Node::Column(_) | Node::Row(_) => (3, TextContent::Static("".into()), None, None, None, false),
                })
            };

            if let Some((node_type, content, condition_signal, true_branch, false_branch, old_visible)) = node_info {
                match node_type {
                    0 => {
                        // Text node
                        let value = match &content {
                            TextContent::Static(s) => s.to_string(),
                            TextContent::Signal(signal_id) => {
                                self.read_signal_display(*signal_id).unwrap_or_default()
                            }
                            TextContent::Derived(derived_id) => {
                                self.read_derived_display(*derived_id).unwrap_or_default()
                            }
                        };
                        self.push_render_command(RenderCommand::UpdateText {
                            node: node_id,
                            value,
                        });
                    }
                    1 => {
                        // Conditional node
                        if let Some(signal_id) = condition_signal {
                            let new_visible = self.read_signal_bool(signal_id).unwrap_or(false);

                            if new_visible != old_visible {
                                // Update the node's visible state
                                {
                                    let mut arena = self.node_arena.borrow_mut();
                                    if let Some(Node::Conditional(cond)) = arena.get_mut(node_id) {
                                        cond.visible = new_visible;
                                    }
                                }

                                // Generate render commands
                                if new_visible {
                                    if let Some(true_branch) = true_branch {
                                        self.push_render_command(RenderCommand::Show { node: true_branch });
                                    }
                                    if let Some(false_branch) = false_branch {
                                        self.push_render_command(RenderCommand::Hide { node: false_branch });
                                    }
                                } else {
                                    if let Some(true_branch) = true_branch {
                                        self.push_render_command(RenderCommand::Hide { node: true_branch });
                                    }
                                    if let Some(false_branch) = false_branch {
                                        self.push_render_command(RenderCommand::Show { node: false_branch });
                                    }
                                }
                            }
                        }
                    }
                    2 => {
                        // Each node - TODO
                    }
                    3 => {
                        // Container nodes don't need direct updates
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn take_render_commands(&self) -> Vec<RenderCommand> {
        std::mem::take(&mut self.batch.borrow_mut().render_commands)
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

    #[test]
    fn test_granular_update_only_affected_nodes() {
        let rt = SignalRuntime::new();

        // Create two signals - one that will change, one that won't
        let count = rt.create_signal(SignalValue::Int(0));
        let static_val = rt.create_signal(SignalValue::Int(999));

        // Create text nodes bound to each signal
        let dynamic_text = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Signal(count)))
        };

        let static_text = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Signal(static_val)))
        };

        let _unbound_text = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Static("Always Static".into())))
        };

        // Subscribe nodes to their signals
        rt.subscribe_node(SignalOrDerived::Signal(count), dynamic_text);
        rt.subscribe_node(SignalOrDerived::Signal(static_val), static_text);
        // _unbound_text is not subscribed to anything

        // Process any initial updates
        rt.process_node_updates();
        rt.take_render_commands();

        // Now change only the count signal
        rt.set_signal(count, SignalValue::Int(42));

        // Process updates
        rt.process_node_updates();

        // Should only have update for dynamic_text, not static_text or unbound_text
        let commands = rt.take_render_commands();
        assert_eq!(commands.len(), 1, "Expected exactly 1 render command");

        match &commands[0] {
            RenderCommand::UpdateText { node, value } => {
                assert_eq!(*node, dynamic_text, "Expected update for dynamic_text node");
                assert_eq!(value, "42", "Expected value to be '42'");
            }
            _ => panic!("Expected UpdateText command"),
        }
    }

    #[test]
    fn test_conditional_visibility() {
        let rt = SignalRuntime::new();

        // Create a boolean signal for the condition
        let show = rt.create_signal(SignalValue::Bool(true));

        // Create text nodes for true and false branches
        let true_text = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Static("Visible!".into())))
        };

        let false_text = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Static("Hidden!".into())))
        };

        // Create conditional node
        let cond_node = {
            let mut arena = rt.node_arena.borrow_mut();
            let node_id = arena.create(Node::conditional(show));
            if let Some(Node::Conditional(cond)) = arena.get_mut(node_id) {
                cond.true_branch = Some(true_text);
                cond.false_branch = Some(false_text);
                cond.visible = false; // Start with false so first update triggers
            }
            node_id
        };

        // Subscribe the conditional node to the signal
        rt.subscribe_node(SignalOrDerived::Signal(show), cond_node);

        // Initial state: signal is true
        rt.process_node_updates();
        let commands = rt.take_render_commands();

        // Should show true branch and hide false branch
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().any(|c| matches!(c, RenderCommand::Show { node } if *node == true_text)));
        assert!(commands.iter().any(|c| matches!(c, RenderCommand::Hide { node } if *node == false_text)));

        // Now toggle to false
        rt.set_signal(show, SignalValue::Bool(false));
        rt.process_node_updates();
        let commands = rt.take_render_commands();

        // Should hide true branch and show false branch
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().any(|c| matches!(c, RenderCommand::Hide { node } if *node == true_text)));
        assert!(commands.iter().any(|c| matches!(c, RenderCommand::Show { node } if *node == false_text)));
    }

    #[test]
    fn test_text_node_with_derived_signal() {
        use std::rc::Rc;
        let lua = Lua::new();
        let rt = Rc::new(SignalRuntime::new());

        // Create a signal
        let count = rt.create_signal(SignalValue::Int(5));

        // Create a derived that doubles the count
        let doubled = {
            let derived_fn = lua
                .create_function({
                    let rt_clone = rt.clone();
                    move |lua: &Lua, ()| {
                        let val = rt_clone.get_signal(lua, count)?;
                        match val {
                            Value::Integer(n) => Ok(Value::Integer(n * 2)),
                            _ => Ok(Value::Nil),
                        }
                    }
                })
                .unwrap();
            let key = lua.create_registry_value(derived_fn).unwrap();
            rt.create_derived(key)
        };

        // Create a text node bound to the derived
        let text_node = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Derived(doubled)))
        };

        // Subscribe node to derived
        rt.subscribe_node(SignalOrDerived::Derived(doubled), text_node);

        // Initial computation
        let _ = rt.get_derived(&lua, doubled);
        rt.process_node_updates();
        let commands = rt.take_render_commands();

        // Should have initial update
        if !commands.is_empty() {
            match &commands[0] {
                RenderCommand::UpdateText { node, value } => {
                    assert_eq!(*node, text_node);
                    assert_eq!(value, "10");
                }
                _ => {}
            }
        }

        // Change the source signal
        rt.set_signal(count, SignalValue::Int(10));

        // Recompute derived
        let _ = rt.get_derived(&lua, doubled);
        rt.process_node_updates();
        let commands = rt.take_render_commands();

        // Should have update with new value
        assert!(!commands.is_empty());
        match &commands[0] {
            RenderCommand::UpdateText { node, value } => {
                assert_eq!(*node, text_node);
                assert_eq!(value, "20");
            }
            _ => panic!("Expected UpdateText command"),
        }
    }

    #[test]
    fn test_multiple_nodes_same_signal() {
        let rt = SignalRuntime::new();

        let count = rt.create_signal(SignalValue::Int(0));

        // Create multiple nodes bound to the same signal
        let text1 = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Signal(count)))
        };

        let text2 = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Signal(count)))
        };

        let text3 = {
            let mut arena = rt.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Signal(count)))
        };

        rt.subscribe_node(SignalOrDerived::Signal(count), text1);
        rt.subscribe_node(SignalOrDerived::Signal(count), text2);
        rt.subscribe_node(SignalOrDerived::Signal(count), text3);

        // Process initial updates
        rt.process_node_updates();
        rt.take_render_commands();

        // Change the signal
        rt.set_signal(count, SignalValue::Int(100));
        rt.process_node_updates();

        let commands = rt.take_render_commands();

        // All three nodes should be updated
        assert_eq!(commands.len(), 3, "Expected 3 UpdateText commands");

        for cmd in &commands {
            match cmd {
                RenderCommand::UpdateText { node, value } => {
                    assert!(
                        *node == text1 || *node == text2 || *node == text3,
                        "Unexpected node in command"
                    );
                    assert_eq!(value, "100");
                }
                _ => panic!("Expected UpdateText command"),
            }
        }
    }
}
