use super::node::{NodeArena, NodeId, TextContent, UiNode};
use super::super::signal::graph::EffectId;
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
}

impl UiRegistry {
    pub fn new() -> Self {
        Self {
            nodes: NodeArena::new(),
            root: None,
            effect_to_node: HashMap::new(),
            node_to_effects: HashMap::new(),
            dirty_nodes: HashSet::new(),
        }
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

    /// Mark a node as dirty (needs re-rendering)
    pub fn mark_dirty(&mut self, node_id: NodeId) {
        self.dirty_nodes.insert(node_id);
    }

    /// Update text content of a node and mark it dirty
    /// Returns true if the node was found and updated
    pub fn update_text_content(&mut self, node_id: NodeId, new_value: String) -> bool {
        if let Some(UiNode::Text { content }) = self.nodes.get_mut(node_id) {
            content.update(new_value);
            self.mark_dirty(node_id);
            true
        } else {
            false
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

        Some((node, effects))
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
