use rover_ui::ui::{NodeId, UiNode, UiRegistry};

/// Screen position and dimensions for a laid-out node.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub row: u16,
    pub col: u16,
    pub width: u16,
    pub height: u16,
}

/// Vec-indexed layout map.
///
/// Uses `NodeId::index()` for O(1) lookup with no hashing overhead.
/// Sparse slots are `None`. The Vec grows to accommodate the highest NodeId seen.
pub struct LayoutMap {
    entries: Vec<Option<LayoutRect>>,
}

impl LayoutMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Pre-allocate capacity for at least `n` node slots.
    pub fn with_capacity(n: usize) -> Self {
        Self {
            entries: vec![None; n],
        }
    }

    /// Insert or update a layout entry.
    #[inline]
    pub fn set(&mut self, id: NodeId, rect: LayoutRect) {
        let idx = id.index();
        if idx >= self.entries.len() {
            self.entries.resize(idx + 1, None);
        }
        self.entries[idx] = Some(rect);
    }

    /// Look up a layout entry by NodeId.
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&LayoutRect> {
        self.entries.get(id.index()).and_then(|e| e.as_ref())
    }

    /// Remove a layout entry.
    #[inline]
    pub fn remove(&mut self, id: NodeId) {
        let idx = id.index();
        if idx < self.entries.len() {
            self.entries[idx] = None;
        }
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for LayoutMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute the full layout for the node tree rooted at `root`.
///
/// Layout rules (intentionally minimal):
/// - **Column / View**: children stack vertically. Each child starts on the
///   next row after the previous child ends.
/// - **Row**: children are placed horizontally. Each child starts at the
///   column after the previous child ends.
/// - **Text**: leaf node, width = string length in bytes (ASCII assumption
///   for now), height = 1.
/// - **Button**: rendered as `[label]`, width = label.len() + 2, height = 1.
/// - **Input**: rendered as current value text, width = value.len(), height = 1.
/// - **Checkbox**: rendered as `[x]` or `[ ]`, width = 3, height = 1.
/// - **Conditional**: if child is present, laid out in-place; otherwise zero-size.
/// - **List**: children stack vertically like Column.
/// - **Image**: placeholder, zero-size for now.
///
/// Returns the bounding box (width, height) of the subtree.
pub fn compute_layout(
    registry: &UiRegistry,
    root: NodeId,
    origin_row: u16,
    origin_col: u16,
    layout: &mut LayoutMap,
) -> (u16, u16) {
    let node = match registry.get_node(root) {
        Some(n) => n,
        None => return (0, 0),
    };

    match node {
        UiNode::Text { content } => {
            let text = content.value();
            let width = text.len() as u16;
            let height = if width > 0 { 1 } else { 0 };
            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width,
                    height,
                },
            );
            (width, height)
        }

        UiNode::Column { children } | UiNode::View { children } => {
            // Clone children vec to release the borrow on registry
            let children = children.clone();
            let mut total_height: u16 = 0;
            let mut max_width: u16 = 0;

            for child_id in &children {
                let (w, h) = compute_layout(
                    registry,
                    *child_id,
                    origin_row + total_height,
                    origin_col,
                    layout,
                );
                max_width = max_width.max(w);
                total_height = total_height.saturating_add(h);
            }

            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width: max_width,
                    height: total_height,
                },
            );
            (max_width, total_height)
        }

        UiNode::Row { children } => {
            let children = children.clone();
            let mut total_width: u16 = 0;
            let mut max_height: u16 = 0;

            for child_id in &children {
                let (w, h) = compute_layout(
                    registry,
                    *child_id,
                    origin_row,
                    origin_col + total_width,
                    layout,
                );
                total_width = total_width.saturating_add(w);
                max_height = max_height.max(h);
            }

            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width: total_width,
                    height: max_height,
                },
            );
            (total_width, max_height)
        }

        UiNode::Button { label, .. } => {
            // Render as [label]
            let width = label.len() as u16 + 2;
            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width,
                    height: 1,
                },
            );
            (width, 1)
        }

        UiNode::Input { value, .. } => {
            let width = value.value().len().max(1) as u16;
            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width,
                    height: 1,
                },
            );
            (width, 1)
        }

        UiNode::Checkbox { .. } => {
            // Render as [x] or [ ]
            let width = 3;
            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width,
                    height: 1,
                },
            );
            (width, 1)
        }

        UiNode::Conditional { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(registry, child_id, origin_row, origin_col, layout);
                layout.set(
                    root,
                    LayoutRect {
                        row: origin_row,
                        col: origin_col,
                        width: w,
                        height: h,
                    },
                );
                (w, h)
            } else {
                layout.set(
                    root,
                    LayoutRect {
                        row: origin_row,
                        col: origin_col,
                        width: 0,
                        height: 0,
                    },
                );
                (0, 0)
            }
        }

        UiNode::KeyArea { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(registry, child_id, origin_row, origin_col, layout);
                layout.set(
                    root,
                    LayoutRect {
                        row: origin_row,
                        col: origin_col,
                        width: w,
                        height: h,
                    },
                );
                (w, h)
            } else {
                layout.set(
                    root,
                    LayoutRect {
                        row: origin_row,
                        col: origin_col,
                        width: 0,
                        height: 0,
                    },
                );
                (0, 0)
            }
        }

        UiNode::List { children, .. } => {
            // Like Column: stack children vertically
            let children = children.clone();
            let mut total_height: u16 = 0;
            let mut max_width: u16 = 0;

            for child_id in &children {
                let (w, h) = compute_layout(
                    registry,
                    *child_id,
                    origin_row + total_height,
                    origin_col,
                    layout,
                );
                max_width = max_width.max(w);
                total_height = total_height.saturating_add(h);
            }

            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width: max_width,
                    height: total_height,
                },
            );
            (max_width, total_height)
        }

        UiNode::Image { .. } => {
            // Placeholder — images not supported in terminal yet
            layout.set(
                root,
                LayoutRect {
                    row: origin_row,
                    col: origin_col,
                    width: 0,
                    height: 0,
                },
            );
            (0, 0)
        }
    }
}

/// Get the renderable text for a node (leaf content only).
/// Returns `None` for container nodes.
pub fn node_content(node: &UiNode) -> Option<String> {
    match node {
        UiNode::Text { content } => Some(content.value().to_string()),
        UiNode::Button { label, .. } => Some(format!("[{}]", label)),
        UiNode::Input { value, .. } => Some(value.value().to_string()),
        UiNode::Checkbox { checked, .. } => Some(if *checked { "[x]" } else { "[ ]" }.to_string()),
        // Container nodes have no direct content
        UiNode::Column { .. }
        | UiNode::Row { .. }
        | UiNode::View { .. }
        | UiNode::Conditional { .. }
        | UiNode::KeyArea { .. }
        | UiNode::List { .. }
        | UiNode::Image { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::{TextContent, UiNode};

    /// Helper to build a registry with a given tree and return (registry, root_id)
    fn build_registry(root_node: UiNode) -> (UiRegistry, NodeId) {
        let mut registry = UiRegistry::new();
        let id = registry.create_node(root_node);
        registry.set_root(id);
        (registry, id)
    }

    #[test]
    fn test_single_text_node() {
        let (registry, root) = build_registry(UiNode::Text {
            content: TextContent::Static("Hello".into()),
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 5);
        assert_eq!(h, 1);

        let rect = layout.get(root).unwrap();
        assert_eq!(rect.row, 0);
        assert_eq!(rect.col, 0);
        assert_eq!(rect.width, 5);
        assert_eq!(rect.height, 1);
    }

    #[test]
    fn test_empty_text_node() {
        let (registry, root) = build_registry(UiNode::Text {
            content: TextContent::Static("".into()),
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_column_layout() {
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("Line 1".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("Longer line 2".into()),
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![t1, t2],
        });
        registry.set_root(col);

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, col, 0, 0, &mut layout);

        // Width = max child width = "Longer line 2".len() = 13
        assert_eq!(w, 13);
        // Height = sum of child heights = 1 + 1 = 2
        assert_eq!(h, 2);

        // First child at (0, 0)
        let r1 = layout.get(t1).unwrap();
        assert_eq!(r1.row, 0);
        assert_eq!(r1.col, 0);

        // Second child at (1, 0) — stacked below
        let r2 = layout.get(t2).unwrap();
        assert_eq!(r2.row, 1);
        assert_eq!(r2.col, 0);
    }

    #[test]
    fn test_row_layout() {
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("AB".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("CDE".into()),
        });
        let row = registry.create_node(UiNode::Row {
            children: vec![t1, t2],
        });
        registry.set_root(row);

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, row, 0, 0, &mut layout);

        // Width = sum = 2 + 3 = 5
        assert_eq!(w, 5);
        // Height = max = 1
        assert_eq!(h, 1);

        let r1 = layout.get(t1).unwrap();
        assert_eq!(r1.row, 0);
        assert_eq!(r1.col, 0);

        let r2 = layout.get(t2).unwrap();
        assert_eq!(r2.row, 0);
        assert_eq!(r2.col, 2); // after "AB"
    }

    #[test]
    fn test_nested_column_in_row() {
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("A".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("B".into()),
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![t1, t2],
        });
        let t3 = registry.create_node(UiNode::Text {
            content: TextContent::Static("CD".into()),
        });
        let row = registry.create_node(UiNode::Row {
            children: vec![col, t3],
        });
        registry.set_root(row);

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, row, 0, 0, &mut layout);

        // Column has width=1, height=2
        // t3 placed at col=1, width=2
        // Row total: width=3, height=2
        assert_eq!(w, 3);
        assert_eq!(h, 2);

        assert_eq!(layout.get(t1).unwrap().row, 0);
        assert_eq!(layout.get(t1).unwrap().col, 0);
        assert_eq!(layout.get(t2).unwrap().row, 1);
        assert_eq!(layout.get(t2).unwrap().col, 0);
        assert_eq!(layout.get(t3).unwrap().row, 0);
        assert_eq!(layout.get(t3).unwrap().col, 1);
    }

    #[test]
    fn test_layout_with_origin_offset() {
        let (registry, root) = build_registry(UiNode::Text {
            content: TextContent::Static("Hi".into()),
        });

        let mut layout = LayoutMap::new();
        compute_layout(&registry, root, 5, 10, &mut layout);

        let rect = layout.get(root).unwrap();
        assert_eq!(rect.row, 5);
        assert_eq!(rect.col, 10);
        assert_eq!(rect.width, 2);
    }

    #[test]
    fn test_button_layout() {
        let (registry, root) = build_registry(UiNode::Button {
            label: "OK".into(),
            on_click: None,
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        // [OK] = 4 chars
        assert_eq!(w, 4);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_checkbox_layout() {
        let (registry, root) = build_registry(UiNode::Checkbox {
            checked: true,
            on_toggle: None,
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 3);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_conditional_with_child() {
        use rover_ui::signal::graph::EffectId;

        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("Visible".into()),
        });
        let cond = registry.create_node(UiNode::Conditional {
            condition_effect: EffectId(0),
            child: Some(child),
        });
        registry.set_root(cond);

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, cond, 0, 0, &mut layout);

        assert_eq!(w, 7); // "Visible"
        assert_eq!(h, 1);
    }

    #[test]
    fn test_conditional_without_child() {
        use rover_ui::signal::graph::EffectId;

        let (registry, root) = build_registry(UiNode::Conditional {
            condition_effect: EffectId(0),
            child: None,
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_layout_map_set_get_remove() {
        let mut map = LayoutMap::new();
        let id = NodeId::from_u32(5);

        assert!(map.get(id).is_none());

        map.set(
            id,
            LayoutRect {
                row: 1,
                col: 2,
                width: 10,
                height: 1,
            },
        );
        assert_eq!(map.get(id).unwrap().row, 1);
        assert_eq!(map.get(id).unwrap().col, 2);

        map.remove(id);
        assert!(map.get(id).is_none());
    }

    #[test]
    fn test_node_content_text() {
        let node = UiNode::Text {
            content: TextContent::Static("hello".into()),
        };
        assert_eq!(node_content(&node).unwrap(), "hello");
    }

    #[test]
    fn test_node_content_button() {
        let node = UiNode::Button {
            label: "OK".into(),
            on_click: None,
        };
        assert_eq!(node_content(&node).unwrap(), "[OK]");
    }

    #[test]
    fn test_node_content_checkbox() {
        let checked = UiNode::Checkbox {
            checked: true,
            on_toggle: None,
        };
        assert_eq!(node_content(&checked).unwrap(), "[x]");

        let unchecked = UiNode::Checkbox {
            checked: false,
            on_toggle: None,
        };
        assert_eq!(node_content(&unchecked).unwrap(), "[ ]");
    }

    #[test]
    fn test_node_content_container_returns_none() {
        let col = UiNode::Column { children: vec![] };
        assert!(node_content(&col).is_none());

        let row = UiNode::Row { children: vec![] };
        assert!(node_content(&row).is_none());
    }
}
