use super::super::signal::graph::EffectId;

/// Unique identifier for a UI node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);

/// Arena-based storage for UI nodes
pub struct NodeArena {
    nodes: Vec<Option<UiNode>>,
    free_list: Vec<u32>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Reserve a node ID without finalizing the node
    pub fn reserve(&mut self) -> NodeId {
        if let Some(idx) = self.free_list.pop() {
            NodeId(idx)
        } else {
            let idx = self.nodes.len() as u32;
            self.nodes.push(None);
            NodeId(idx)
        }
    }

    /// Finalize a reserved node with its content
    pub fn finalize(&mut self, id: NodeId, node: UiNode) {
        self.nodes[id.0 as usize] = Some(node);
    }

    /// Create a node directly (reserve + finalize in one step)
    pub fn create(&mut self, node: UiNode) -> NodeId {
        let id = self.reserve();
        self.finalize(id, node);
        id
    }

    /// Get a node by ID
    pub fn get(&self, id: NodeId) -> Option<&UiNode> {
        self.nodes.get(id.0 as usize).and_then(|n| n.as_ref())
    }

    /// Get a mutable reference to a node
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut UiNode> {
        self.nodes.get_mut(id.0 as usize).and_then(|n| n.as_mut())
    }

    /// Remove a node and return it
    pub fn remove(&mut self, id: NodeId) -> Option<UiNode> {
        if let Some(slot) = self.nodes.get_mut(id.0 as usize) {
            let node = slot.take();
            if node.is_some() {
                self.free_list.push(id.0);
            }
            node
        } else {
            None
        }
    }
}

impl Default for NodeArena {
    fn default() -> Self {
        Self::new()
    }
}

/// UI node types
#[derive(Debug, Clone)]
pub enum UiNode {
    Text {
        content: TextContent,
    },
    Column {
        children: Vec<NodeId>,
    },
    Row {
        children: Vec<NodeId>,
    },
    View {
        children: Vec<NodeId>,
    },
}

/// Text content can be static or reactive
#[derive(Debug, Clone)]
pub enum TextContent {
    /// Static text - rendered once, no reactivity overhead
    Static(String),
    /// Reactive text - backed by a signal/derived with an effect
    Reactive {
        current_value: String,
        effect_id: EffectId,
    },
}

impl TextContent {
    /// Get the current text value regardless of whether it's static or reactive
    pub fn value(&self) -> &str {
        match self {
            TextContent::Static(s) => s,
            TextContent::Reactive { current_value, .. } => current_value,
        }
    }

    /// Update the text content (only valid for Reactive variant)
    pub fn update(&mut self, new_value: String) {
        if let TextContent::Reactive { current_value, .. } = self {
            *current_value = new_value;
        }
    }

    /// Get the effect ID if this is reactive text
    pub fn effect_id(&self) -> Option<EffectId> {
        match self {
            TextContent::Static(_) => None,
            TextContent::Reactive { effect_id, .. } => Some(*effect_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_arena_create() {
        let mut arena = NodeArena::new();
        let node = UiNode::Text {
            content: TextContent::Static("Hello".to_string()),
        };
        let id = arena.create(node);

        assert_eq!(id, NodeId(0));
        assert!(arena.get(id).is_some());
    }

    #[test]
    fn test_node_arena_reserve_finalize() {
        let mut arena = NodeArena::new();
        let id = arena.reserve();

        assert_eq!(id, NodeId(0));
        assert!(arena.get(id).is_none()); // Not finalized yet

        let node = UiNode::Text {
            content: TextContent::Static("Test".to_string()),
        };
        arena.finalize(id, node);

        assert!(arena.get(id).is_some());
    }

    #[test]
    fn test_node_arena_remove_reuse() {
        let mut arena = NodeArena::new();
        let node1 = UiNode::Text {
            content: TextContent::Static("First".to_string()),
        };
        let id1 = arena.create(node1);

        arena.remove(id1);
        assert!(arena.get(id1).is_none());

        // Next create should reuse the freed slot
        let node2 = UiNode::Text {
            content: TextContent::Static("Second".to_string()),
        };
        let id2 = arena.create(node2);

        assert_eq!(id1, id2); // Same ID reused
    }

    #[test]
    fn test_text_content_value() {
        let static_text = TextContent::Static("Static".to_string());
        assert_eq!(static_text.value(), "Static");

        let reactive_text = TextContent::Reactive {
            current_value: "Reactive".to_string(),
            effect_id: EffectId(0),
        };
        assert_eq!(reactive_text.value(), "Reactive");
    }

    #[test]
    fn test_text_content_update() {
        let mut text = TextContent::Reactive {
            current_value: "Old".to_string(),
            effect_id: EffectId(0),
        };

        text.update("New".to_string());
        assert_eq!(text.value(), "New");
    }
}
