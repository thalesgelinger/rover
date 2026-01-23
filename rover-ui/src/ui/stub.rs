use super::node::{NodeId, UiNode};
use super::renderer::Renderer;
use super::registry::UiRegistry;

/// Debug/testing renderer that prints updates to console
///
/// This renderer doesn't actually render anything visually - it just
/// prints node operations for testing and debugging purposes.
pub struct StubRenderer {
    /// Track if we've mounted yet
    mounted: bool,
}

impl StubRenderer {
    pub fn new() -> Self {
        Self { mounted: false }
    }

    fn print_node(&self, registry: &UiRegistry, node_id: NodeId, indent: usize) {
        let indent_str = "  ".repeat(indent);

        if let Some(node) = registry.get_node(node_id) {
            match node {
                UiNode::Text { content } => {
                    println!(
                        "{}Text(id={:?}): \"{}\"",
                        indent_str,
                        node_id,
                        content.value()
                    );
                }
                UiNode::Column { children } => {
                    println!("{}Column(id={:?}) {{", indent_str, node_id);
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    println!("{}}}", indent_str);
                }
                UiNode::Row { children } => {
                    println!("{}Row(id={:?}) {{", indent_str, node_id);
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    println!("{}}}", indent_str);
                }
                UiNode::View { children } => {
                    println!("{}View(id={:?}) {{", indent_str, node_id);
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    println!("{}}}", indent_str);
                }
            }
        }
    }
}

impl Default for StubRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for StubRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        println!("=== StubRenderer: MOUNT ===");
        if let Some(root_id) = registry.root() {
            self.print_node(registry, root_id, 0);
        } else {
            println!("(no root node)");
        }
        println!("===========================\n");
        self.mounted = true;
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        if dirty_nodes.is_empty() {
            return;
        }

        println!("=== StubRenderer: UPDATE ===");
        println!("Dirty nodes: {}", dirty_nodes.len());

        for &node_id in dirty_nodes {
            if let Some(node) = registry.get_node(node_id) {
                match node {
                    UiNode::Text { content } => {
                        println!("  Updated Text(id={:?}): \"{}\"", node_id, content.value());
                    }
                    UiNode::Column { .. } => {
                        println!("  Updated Column(id={:?})", node_id);
                    }
                    UiNode::Row { .. } => {
                        println!("  Updated Row(id={:?})", node_id);
                    }
                    UiNode::View { .. } => {
                        println!("  Updated View(id={:?})", node_id);
                    }
                }
            }
        }
        println!("============================\n");
    }

    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId) {
        println!("=== StubRenderer: NODE ADDED ===");
        self.print_node(registry, node_id, 0);
        println!("================================\n");
    }

    fn node_removed(&mut self, node_id: NodeId) {
        println!("=== StubRenderer: NODE REMOVED ===");
        println!("  Removed node: {:?}", node_id);
        println!("===================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::node::TextContent;

    #[test]
    fn test_stub_renderer_mount() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Static("Hello, World!".to_string()),
        };
        let root_id = registry.create_node(node);
        registry.set_root(root_id);

        let mut renderer = StubRenderer::new();
        renderer.mount(&registry);

        assert!(renderer.mounted);
    }

    #[test]
    fn test_stub_renderer_update() {
        let mut registry = UiRegistry::new();
        let node = UiNode::Text {
            content: TextContent::Static("Initial".to_string()),
        };
        let node_id = registry.create_node(node);

        let mut renderer = StubRenderer::new();
        registry.mark_dirty(node_id);
        let dirty = registry.take_dirty_nodes();

        renderer.update(&registry, &dirty.into_iter().collect::<Vec<_>>());
    }
}
