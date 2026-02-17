use super::node::{NodeArena, NodeId, UiNode};
use super::style::NodeStyle;

use super::super::signal::graph::EffectId;
#[cfg(test)]
use super::node::TextContent;
use mlua::{RegistryKey, Value};
use std::collections::{HashMap, HashSet};

/// Central registry for UI nodes with dirty tracking
pub struct UiRegistry {
    nodes: NodeArena,
    root: Option<NodeId>,
    /// Map from effect ID to the node it updates
    effect_to_node: HashMap<EffectId, NodeId>,
    /// Map from node to all effects that update it
    node_to_effects: HashMap<NodeId, Vec<EffectId>>,
    /// Set of nodes that have been modified and need re-rendering
    dirty_nodes: HashSet<NodeId>,
    /// Cleanup callbacks to run on destroy
    on_destroy_callbacks: Vec<RegistryKey>,
    /// Conditional node state tracking
    condition_state: HashMap<NodeId, bool>,
    /// List node items tracking
    list_items: HashMap<NodeId, Value>,
    /// Resolved style per node
    node_styles: HashMap<NodeId, NodeStyle>,
}

impl UiRegistry {
    pub fn new() -> Self {
        Self {
            nodes: NodeArena::new(),
            root: None,
            effect_to_node: HashMap::new(),
            node_to_effects: HashMap::new(),
            dirty_nodes: HashSet::new(),
            on_destroy_callbacks: Vec::new(),
            condition_state: HashMap::new(),
            list_items: HashMap::new(),
            node_styles: HashMap::new(),
        }
    }

    /// Add a cleanup callback to be run on destroy
    pub fn add_on_destroy_callback(&mut self, key: RegistryKey) {
        self.on_destroy_callbacks.push(key);
    }

    /// Take all on_destroy callbacks (for use by app cleanup)
    pub fn take_on_destroy_callbacks(&mut self) -> Vec<RegistryKey> {
        std::mem::take(&mut self.on_destroy_callbacks)
    }

    /// Create a new node directly
    pub fn create_node(&mut self, node: UiNode) -> NodeId {
        self.nodes.create(node)
    }

    /// Reserve a node ID (for use when creating effects that reference the node)
    pub fn reserve_node_id(&mut self) -> NodeId {
        self.nodes.reserve()
    }

    /// Finalize a reserved node
    pub fn finalize_node(&mut self, id: NodeId, node: UiNode) {
        self.nodes.finalize(id, node);
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<&UiNode> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to a node
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut UiNode> {
        self.nodes.get_mut(id)
    }

    /// Attach an effect to a node
    pub fn attach_effect(&mut self, node_id: NodeId, effect_id: EffectId) {
        self.effect_to_node.insert(effect_id, node_id);
        self.node_to_effects
            .entry(node_id)
            .or_insert_with(Vec::new)
            .push(effect_id);
    }

    /// Get all effects attached to a node
    pub fn get_effects_for_node(&self, node_id: NodeId) -> Vec<EffectId> {
        self.node_to_effects
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Mark a node as dirty (needs re-rendering)
    pub fn mark_dirty(&mut self, node_id: NodeId) {
        self.dirty_nodes.insert(node_id);
    }

    /// Update text content of a node and mark it dirty
    /// Returns true if the node was found and updated
    ///
    /// This handles both `UiNode::Text` and `UiNode::Input` (which stores its value as `TextContent`)
    pub fn update_text_content(&mut self, node_id: NodeId, new_value: String) -> bool {
        let node = self.nodes.get_mut(node_id);
        match node {
            Some(UiNode::Text { content }) => {
                content.update(new_value);
                self.mark_dirty(node_id);
                true
            }
            Some(UiNode::Input { value, .. }) => {
                value.update(new_value);
                self.mark_dirty(node_id);
                true
            }
            _ => false,
        }
    }

    /// Remove a node and dispose all its effects
    /// Note: Caller is responsible for calling runtime.dispose_effect for each effect
    pub fn remove_node(&mut self, node_id: NodeId) -> Option<(UiNode, Vec<EffectId>)> {
        let node = self.nodes.remove(node_id)?;

        // Get all effects associated with this node
        let effects = self.node_to_effects.remove(&node_id).unwrap_or_default();

        // Remove effect mappings
        for effect_id in &effects {
            self.effect_to_node.remove(effect_id);
        }

        // Remove from dirty set if present
        self.dirty_nodes.remove(&node_id);
        self.node_styles.remove(&node_id);
        self.condition_state.remove(&node_id);
        self.list_items.remove(&node_id);

        Some((node, effects))
    }

    /// Get style for a node, if present.
    pub fn get_node_style(&self, node_id: NodeId) -> Option<&NodeStyle> {
        self.node_styles.get(&node_id)
    }

    /// Set style for a node and mark it dirty if changed.
    pub fn set_node_style(&mut self, node_id: NodeId, style: NodeStyle) {
        let changed = self
            .node_styles
            .get(&node_id)
            .map(|existing| existing != &style)
            .unwrap_or(true);
        self.node_styles.insert(node_id, style);
        if changed {
            self.mark_dirty(node_id);
        }
    }

    /// Take all dirty nodes (clears the dirty set and returns it)
    pub fn take_dirty_nodes(&mut self) -> HashSet<NodeId> {
        std::mem::take(&mut self.dirty_nodes)
    }

    /// Get the root node ID if set
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Set the root node
    pub fn set_root(&mut self, node_id: NodeId) {
        self.root = Some(node_id);
    }

    /// Get all nodes (for debugging/testing)
    pub fn nodes(&self) -> &NodeArena {
        &self.nodes
    }

    // ===== Conditional node methods =====

    /// Get the condition state for a conditional node
    pub fn get_condition_state(&self, node_id: NodeId) -> Option<bool> {
        self.condition_state.get(&node_id).copied()
    }

    /// Set the condition state for a conditional node
    pub fn set_condition_state(&mut self, node_id: NodeId, state: bool) {
        self.condition_state.insert(node_id, state);
    }

    /// Get the child of a conditional node
    pub fn get_condition_child(&self, node_id: NodeId) -> Option<NodeId> {
        if let Some(UiNode::Conditional { child, .. }) = self.nodes.get(node_id) {
            *child
        } else {
            None
        }
    }

    /// Set the child of a conditional node
    pub fn set_condition_child(&mut self, node_id: NodeId, child_id: NodeId) {
        if let Some(UiNode::Conditional { child, .. }) = self.nodes.get_mut(node_id) {
            *child = Some(child_id);
        }
    }

    /// Remove the child of a conditional node
    pub fn remove_condition_child(&mut self, node_id: NodeId) -> Option<NodeId> {
        if let Some(UiNode::Conditional { child, .. }) = self.nodes.get_mut(node_id) {
            return child.take();
        }
        None
    }

    // ===== List node methods =====

    /// Get the items for a list node
    pub fn get_list_items(&self, node_id: NodeId) -> &Value {
        self.list_items
            .get(&node_id)
            .map(|v| v)
            .unwrap_or(&Value::Nil)
    }

    /// Set the items for a list node
    pub fn set_list_items(&mut self, node_id: NodeId, items: Value) {
        self.list_items.insert(node_id, items);
    }

    /// Get the children of a list node
    pub fn get_list_children(&self, node_id: NodeId) -> &[NodeId] {
        if let Some(UiNode::List { children, .. }) = self.nodes.get(node_id) {
            children
        } else {
            &[]
        }
    }

    /// Update the children of a list node
    pub fn update_list_children(&mut self, node_id: NodeId, children: Vec<NodeId>) {
        if let Some(UiNode::List {
            children: existing_children,
            ..
        }) = self.nodes.get_mut(node_id)
        {
            *existing_children = children;
            self.mark_dirty(node_id);
        }
    }
}

impl Default for UiRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Static("Hello".to_string()),
        };
        let id = registry.create_node(node);

        assert!(registry.get_node(id).is_some());
    }

    #[test]
    fn test_reserve_finalize() {
        let mut registry = UiRegistry::new();
        let id = registry.reserve_node_id();

        assert!(registry.get_node(id).is_none());

        let node = UiNode::Text {
            content: TextContent::Static("Test".to_string()),
        };
        registry.finalize_node(id, node);

        assert!(registry.get_node(id).is_some());
    }

    #[test]
    fn test_attach_effect() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Reactive {
                current_value: "Test".to_string(),
                effect_id: EffectId(0),
                signal_id: None,
            },
        };
        let node_id = registry.create_node(node);
        let effect_id = EffectId(0);

        registry.attach_effect(node_id, effect_id);

        assert_eq!(registry.effect_to_node.get(&effect_id), Some(&node_id));
    }

    #[test]
    fn test_update_text_content() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Reactive {
                current_value: "Old".to_string(),
                effect_id: EffectId(0),
                signal_id: None,
            },
        };
        let node_id = registry.create_node(node);

        let updated = registry.update_text_content(node_id, "New".to_string());
        assert!(updated);

        if let Some(UiNode::Text { content }) = registry.get_node(node_id) {
            assert_eq!(content.value(), "New");
        } else {
            panic!("Expected Text node");
        }

        // Should be marked dirty
        assert!(registry.dirty_nodes.contains(&node_id));
    }

    #[test]
    fn test_mark_dirty_and_take() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Static("Test".to_string()),
        };
        let node_id = registry.create_node(node);

        registry.mark_dirty(node_id);
        assert!(registry.dirty_nodes.contains(&node_id));

        let dirty = registry.take_dirty_nodes();
        assert_eq!(dirty.len(), 1);
        assert!(dirty.contains(&node_id));
        assert!(registry.dirty_nodes.is_empty());
    }

    #[test]
    fn test_remove_node() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Reactive {
                current_value: "Test".to_string(),
                effect_id: EffectId(0),
                signal_id: None,
            },
        };
        let node_id = registry.create_node(node);
        let effect_id = EffectId(0);

        registry.attach_effect(node_id, effect_id);
        registry.mark_dirty(node_id);

        let removed = registry.remove_node(node_id);
        assert!(removed.is_some());

        let (_, effects) = removed.unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], effect_id);

        // Should be cleaned up
        assert!(registry.get_node(node_id).is_none());
        assert!(!registry.dirty_nodes.contains(&node_id));
        assert!(registry.effect_to_node.get(&effect_id).is_none());
    }
}
