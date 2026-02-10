use super::node::{NodeId, TextContent, UiNode};
use super::registry::UiRegistry;
use super::renderer::Renderer;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

/// Debug/testing renderer that prints updates to console
///
/// This renderer doesn't actually render anything visually - it just
/// prints node operations for testing and debugging purposes.
pub struct StubRenderer {
    /// Track if we've mounted yet
    mounted: bool,
    /// Track previous values for old→new transitions
    previous_values: HashMap<NodeId, String>,
    /// Start time for timestamps
    start_time: Instant,
    /// Optional log buffer for testing
    log_buffer: Option<Rc<RefCell<Vec<String>>>>,
}

impl StubRenderer {
    pub fn new() -> Self {
        Self {
            mounted: false,
            previous_values: HashMap::new(),
            start_time: Instant::now(),
            log_buffer: None,
        }
    }

    /// Create a StubRenderer with a log buffer for testing
    pub fn with_buffer(buffer: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            mounted: false,
            previous_values: HashMap::new(),
            start_time: Instant::now(),
            log_buffer: Some(buffer),
        }
    }

    /// Get timestamp string relative to start_time
    fn timestamp(&self) -> String {
        let elapsed = self.start_time.elapsed();
        format!("[{:>6.2}s]", elapsed.as_secs_f64())
    }

    /// Log a message with timestamp, to buffer and/or console
    fn log(&self, msg: &str) {
        let timestamped = format!("{} {}", self.timestamp(), msg);
        if let Some(buffer) = &self.log_buffer {
            buffer.borrow_mut().push(timestamped.clone());
        }
        println!("{}", timestamped);
    }

    fn print_node(&mut self, registry: &UiRegistry, node_id: NodeId, indent: usize) {
        let indent_str = "  ".repeat(indent);

        if let Some(node) = registry.get_node(node_id) {
            match node {
                UiNode::Text { content } => {
                    let value = content.value().to_string();
                    // Store value for future old→new comparisons
                    self.previous_values.insert(node_id, value.clone());

                    let msg = match content {
                        TextContent::Reactive { effect_id, .. } => {
                            format!(
                                "{}Text(id={:?}, effect={:?}): \"{}\"",
                                indent_str, node_id, effect_id, value
                            )
                        }
                        TextContent::Static(_) => {
                            format!("{}Text(id={:?}): \"{}\"", indent_str, node_id, value)
                        }
                    };
                    self.log(&msg);
                }
                UiNode::Column { children } => {
                    self.log(&format!("{}Column(id={:?}) {{", indent_str, node_id));
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::Row { children } => {
                    self.log(&format!("{}Row(id={:?}) {{", indent_str, node_id));
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::View { children } => {
                    self.log(&format!("{}View(id={:?}) {{", indent_str, node_id));
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::Stack { children } => {
                    self.log(&format!("{}Stack(id={:?}) {{", indent_str, node_id));
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::FullScreen { child } => {
                    self.log(&format!("{}FullScreen(id={:?}) {{", indent_str, node_id));
                    if let Some(child_id) = child {
                        self.print_node(registry, *child_id, indent + 1);
                    } else {
                        self.log(&format!("{}  (empty)", indent_str));
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::Button { label, on_click } => {
                    let event_info = if on_click.is_some() {
                        " [clickable]"
                    } else {
                        ""
                    };
                    self.log(&format!(
                        "{}Button(id={:?}): \"{}\"{}",
                        indent_str, node_id, label, event_info
                    ));
                }
                UiNode::Input {
                    value, on_change, ..
                } => {
                    let event_info = if on_change.is_some() {
                        " [changeable]"
                    } else {
                        ""
                    };
                    self.log(&format!(
                        "{}Input(id={:?}): \"{}\"{}",
                        indent_str,
                        node_id,
                        value.value(),
                        event_info
                    ));
                }
                UiNode::Checkbox { checked, on_toggle } => {
                    let event_info = if on_toggle.is_some() {
                        " [toggleable]"
                    } else {
                        ""
                    };
                    let check_str = if *checked { "☑" } else { "☐" };
                    self.log(&format!(
                        "{}Checkbox(id={:?}): {}{}",
                        indent_str, node_id, check_str, event_info
                    ));
                }
                UiNode::Image { src } => {
                    self.log(&format!(
                        "{}Image(id={:?}): src=\"{}\"",
                        indent_str, node_id, src
                    ));
                }
                UiNode::Conditional {
                    condition_effect,
                    child,
                } => {
                    self.log(&format!(
                        "{}Conditional(id={:?}, effect={:?}) {{",
                        indent_str, node_id, condition_effect
                    ));
                    if let Some(child_id) = child {
                        self.print_node(registry, *child_id, indent + 1);
                    } else {
                        self.log(&format!("{}  (hidden)", indent_str));
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::KeyArea { child, .. } => {
                    self.log(&format!("{}KeyArea(id={:?}) {{", indent_str, node_id));
                    if let Some(child_id) = child {
                        self.print_node(registry, *child_id, indent + 1);
                    } else {
                        self.log(&format!("{}  (empty)", indent_str));
                    }
                    self.log(&format!("{}}}", indent_str));
                }
                UiNode::List {
                    items_effect,
                    children,
                } => {
                    self.log(&format!(
                        "{}List(id={:?}, effect={:?}) {{",
                        indent_str, node_id, items_effect
                    ));
                    for &child_id in children {
                        self.print_node(registry, child_id, indent + 1);
                    }
                    self.log(&format!("{}}}", indent_str));
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
        self.log("=== StubRenderer: MOUNT ===");
        if let Some(root_id) = registry.root() {
            self.print_node(registry, root_id, 0);
        } else {
            self.log("(no root node)");
        }
        self.log("===========================\n");
        self.mounted = true;
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        if dirty_nodes.is_empty() {
            return;
        }

        self.log("=== StubRenderer: UPDATE ===");
        self.log(&format!("Dirty nodes: {}", dirty_nodes.len()));

        for &node_id in dirty_nodes {
            if let Some(node) = registry.get_node(node_id) {
                match node {
                    UiNode::Text { content } => {
                        let new_value = content.value();
                        let old_value = self.previous_values.get(&node_id);

                        let msg = match (old_value, content) {
                            (Some(old), TextContent::Reactive { effect_id, .. }) => {
                                format!(
                                    "  Updated Text(id={:?}, effect={:?}): \"{}\" → \"{}\"",
                                    node_id, effect_id, old, new_value
                                )
                            }
                            (Some(old), TextContent::Static(_)) => {
                                format!(
                                    "  Updated Text(id={:?}): \"{}\" → \"{}\"",
                                    node_id, old, new_value
                                )
                            }
                            (None, TextContent::Reactive { effect_id, .. }) => {
                                format!(
                                    "  Updated Text(id={:?}, effect={:?}): \"{}\"",
                                    node_id, effect_id, new_value
                                )
                            }
                            (None, TextContent::Static(_)) => {
                                format!("  Updated Text(id={:?}): \"{}\"", node_id, new_value)
                            }
                        };
                        self.log(&msg);

                        // Update previous value
                        self.previous_values.insert(node_id, new_value.to_string());
                    }
                    UiNode::Column { .. } => {
                        self.log(&format!("  Updated Column(id={:?})", node_id));
                    }
                    UiNode::Row { .. } => {
                        self.log(&format!("  Updated Row(id={:?})", node_id));
                    }
                    UiNode::View { .. } => {
                        self.log(&format!("  Updated View(id={:?})", node_id));
                    }
                    UiNode::Stack { .. } => {
                        self.log(&format!("  Updated Stack(id={:?})", node_id));
                    }
                    UiNode::FullScreen { child } => {
                        self.log(&format!("  Updated FullScreen(id={:?}) {{", node_id));
                        if let Some(child_id) = child {
                            self.print_node(registry, *child_id, 3);
                        } else {
                            self.log("    (empty)");
                        }
                        self.log("  }");
                    }
                    UiNode::Button { label, .. } => {
                        self.log(&format!(
                            "  Updated Button(id={:?}): \"{}\"",
                            node_id, label
                        ));
                    }
                    UiNode::Input { value, .. } => {
                        let new_value = value.value();
                        let old_value = self.previous_values.get(&node_id);
                        if let Some(old) = old_value {
                            self.log(&format!(
                                "  Updated Input(id={:?}): \"{}\" → \"{}\"",
                                node_id, old, new_value
                            ));
                        } else {
                            self.log(&format!(
                                "  Updated Input(id={:?}): \"{}\"",
                                node_id, new_value
                            ));
                        }
                        self.previous_values.insert(node_id, new_value.to_string());
                    }
                    UiNode::Checkbox { checked, .. } => {
                        let check_str = if *checked { "☑" } else { "☐" };
                        self.log(&format!(
                            "  Updated Checkbox(id={:?}): {}",
                            node_id, check_str
                        ));
                    }
                    UiNode::Image { src } => {
                        self.log(&format!(
                            "  Updated Image(id={:?}): src=\"{}\"",
                            node_id, src
                        ));
                    }
                    UiNode::Conditional { child, .. } => {
                        self.log(&format!("  Updated Conditional(id={:?}) {{", node_id));
                        if let Some(child_id) = child {
                            self.print_node(registry, *child_id, 3);
                        } else {
                            self.log("    (hidden)");
                        }
                        self.log("  }");
                    }
                    UiNode::KeyArea { child, .. } => {
                        self.log(&format!("  Updated KeyArea(id={:?}) {{", node_id));
                        if let Some(child_id) = child {
                            self.print_node(registry, *child_id, 3);
                        } else {
                            self.log("    (empty)");
                        }
                        self.log("  }");
                    }
                    UiNode::List { children, .. } => {
                        self.log(&format!("  Updated List(id={:?}) {{", node_id));
                        for &child_id in children {
                            self.print_node(registry, child_id, 3);
                        }
                        self.log("  }");
                    }
                }
            }
        }
        self.log("============================\n");
    }

    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId) {
        self.log("=== StubRenderer: NODE ADDED ===");
        self.print_node(registry, node_id, 0);
        self.log("================================\n");
    }

    fn node_removed(&mut self, node_id: NodeId) {
        self.log("=== StubRenderer: NODE REMOVED ===");
        self.log(&format!("  Removed node: {:?}", node_id));
        // Clean up previous value tracking
        self.previous_values.remove(&node_id);
        self.log("===================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::TextContent;
    use super::*;

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
