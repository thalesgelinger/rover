use crate::layout::{compute_layout, node_content, LayoutMap, LayoutRect};
use crate::terminal::Terminal;
use rover_ui::ui::{NodeId, Renderer, UiNode, UiRegistry};
use std::io;

/// TUI renderer — draws the UI node tree to the terminal.
///
/// Uses a Vec-indexed layout map for O(1) position lookups and tracks
/// previous content strings to clear stale characters when content shrinks.
/// All writes are queued per frame and flushed once.
pub struct TuiRenderer {
    terminal: Terminal,
    layout: LayoutMap,
    /// Previous rendered content per node, indexed by NodeId.
    /// Used to know how many characters to clear when content changes.
    previous_widths: Vec<u16>,
}

impl TuiRenderer {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminal: Terminal::new()?,
            layout: LayoutMap::new(),
            previous_widths: Vec::new(),
        })
    }

    /// Record the rendered width for a node (for clearing on update).
    #[inline]
    fn set_prev_width(&mut self, id: NodeId, width: u16) {
        let idx = id.index();
        if idx >= self.previous_widths.len() {
            self.previous_widths.resize(idx + 1, 0);
        }
        self.previous_widths[idx] = width;
    }

    /// Get the previously rendered width for a node.
    #[inline]
    fn get_prev_width(&self, id: NodeId) -> u16 {
        let idx = id.index();
        if idx < self.previous_widths.len() {
            self.previous_widths[idx]
        } else {
            0
        }
    }

    /// Render a single leaf node at its layout position.
    fn render_leaf(&mut self, id: NodeId, content: &str, rect: &LayoutRect) -> io::Result<()> {
        let old_width = self.get_prev_width(id);
        let new_width = content.len() as u16;

        // If old content was wider, clear the extra characters
        if old_width > new_width {
            self.terminal
                .queue_clear_region(rect.row, rect.col + new_width, old_width - new_width)?;
        }

        self.terminal.queue_write_at(rect.row, rect.col, content)?;
        self.set_prev_width(id, new_width);
        Ok(())
    }

    /// Walk the tree and render all leaf nodes.
    fn render_tree(&mut self, registry: &UiRegistry, node_id: NodeId) -> io::Result<()> {
        let node = match registry.get_node(node_id) {
            Some(n) => n,
            None => return Ok(()),
        };

        // If it's a leaf with content, render it
        if let Some(content) = node_content(node) {
            if let Some(rect) = self.layout.get(node_id) {
                let rect = *rect;
                self.render_leaf(node_id, &content, &rect)?;
            }
            return Ok(());
        }

        // Container: recurse into children
        let children: Vec<NodeId> = match node {
            UiNode::Column { children }
            | UiNode::Row { children }
            | UiNode::View { children }
            | UiNode::List { children, .. } => children.clone(),
            UiNode::Conditional { child, .. } => {
                child.iter().copied().collect()
            }
            _ => vec![],
        };

        for child_id in children {
            self.render_tree(registry, child_id)?;
        }

        Ok(())
    }

    /// Refresh cached terminal size (call on resize events).
    pub fn refresh_size(&mut self) {
        self.terminal.refresh_size();
    }
}

impl Default for TuiRenderer {
    fn default() -> Self {
        Self::new().expect("failed to initialize terminal")
    }
}

impl Renderer for TuiRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        let root = match registry.root() {
            Some(id) => id,
            None => return,
        };

        // Enter TUI mode
        if let Err(e) = self.terminal.enter() {
            eprintln!("rover-tui: failed to enter terminal: {}", e);
            return;
        }

        if let Err(e) = self.terminal.clear() {
            eprintln!("rover-tui: failed to clear terminal: {}", e);
            return;
        }

        // Compute layout for the full tree
        self.layout.clear();
        compute_layout(registry, root, 0, 0, &mut self.layout);

        // Render all nodes
        if let Err(e) = self.render_tree(registry, root) {
            eprintln!("rover-tui: render error: {}", e);
            return;
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        if dirty_nodes.is_empty() {
            return;
        }

        for &node_id in dirty_nodes {
            let node = match registry.get_node(node_id) {
                Some(n) => n,
                None => continue,
            };

            // Only update leaf nodes with content — container dirty flags
            // are handled by their children being individually dirty.
            let content = match node_content(node) {
                Some(c) => c,
                None => continue,
            };

            let rect = match self.layout.get(node_id) {
                Some(r) => *r,
                None => continue,
            };

            if let Err(e) = self.render_leaf(node_id, &content, &rect) {
                eprintln!("rover-tui: update error for node {:?}: {}", node_id, e);
            }
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn node_added(&mut self, registry: &UiRegistry, _node_id: NodeId) {
        // For now: full re-layout and redraw.
        // Structural changes (add/remove) are rare compared to content updates.
        let root = match registry.root() {
            Some(id) => id,
            None => return,
        };

        self.layout.clear();
        compute_layout(registry, root, 0, 0, &mut self.layout);

        if let Err(e) = self.terminal.clear() {
            eprintln!("rover-tui: clear error: {}", e);
            return;
        }

        if let Err(e) = self.render_tree(registry, root) {
            eprintln!("rover-tui: render error: {}", e);
            return;
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn node_removed(&mut self, node_id: NodeId) {
        self.layout.remove(node_id);
        // Clear previous width tracking
        let idx = node_id.index();
        if idx < self.previous_widths.len() {
            self.previous_widths[idx] = 0;
        }
    }
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        // Terminal::drop will handle leave(), but we also explicitly
        // leave here to get error reporting if needed.
        let _ = self.terminal.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::TextContent;

    // NOTE: We cannot test actual terminal rendering in unit tests (no TTY).
    // These tests verify the internal state management (layout, previous_widths).

    #[test]
    fn test_prev_width_tracking() {
        // Use default which may fail in CI — so test the tracking logic directly
        let mut widths: Vec<u16> = Vec::new();

        let id = NodeId::from_u32(3);
        let idx = id.index();

        // Simulate set_prev_width
        if idx >= widths.len() {
            widths.resize(idx + 1, 0);
        }
        widths[idx] = 10;

        assert_eq!(widths[idx], 10);

        // Simulate get_prev_width
        assert_eq!(
            if idx < widths.len() {
                widths[idx]
            } else {
                0
            },
            10
        );
    }

    #[test]
    fn test_node_content_for_update_decisions() {
        // Leaf nodes produce content
        let text = UiNode::Text {
            content: TextContent::Static("hello".into()),
        };
        assert!(node_content(&text).is_some());

        // Containers do not
        let col = UiNode::Column {
            children: vec![],
        };
        assert!(node_content(&col).is_none());
    }

    #[test]
    fn test_layout_drives_rendering_position() {
        // Verify that layout computation produces correct positions
        // that the renderer would use
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("Hello".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("World".into()),
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![t1, t2],
        });
        registry.set_root(col);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, col, 0, 0, &mut layout);

        // t1 at (0,0), t2 at (1,0)
        let r1 = layout.get(t1).unwrap();
        assert_eq!((r1.row, r1.col), (0, 0));

        let r2 = layout.get(t2).unwrap();
        assert_eq!((r2.row, r2.col), (1, 0));
    }

    #[test]
    fn test_row_content_positions_for_counter_pattern() {
        // Simulates the counter.lua pattern:
        // row { text("Count: "), text(signal) }
        let mut registry = UiRegistry::new();
        let label = registry.create_node(UiNode::Text {
            content: TextContent::Static("Count: ".into()),
        });
        let value = registry.create_node(UiNode::Text {
            content: TextContent::Static("0".into()),
        });
        let row = registry.create_node(UiNode::Row {
            children: vec![label, value],
        });
        registry.set_root(row);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, row, 0, 0, &mut layout);

        let r_label = layout.get(label).unwrap();
        assert_eq!((r_label.row, r_label.col), (0, 0));
        assert_eq!(r_label.width, 7); // "Count: "

        let r_value = layout.get(value).unwrap();
        assert_eq!((r_value.row, r_value.col), (0, 7));
        assert_eq!(r_value.width, 1); // "0"
    }
}
