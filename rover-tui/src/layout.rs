use rover_ui::ui::{NodeId, NodeStyle, PositionType, StyleOp, StyleSize, UiNode, UiRegistry};
use unicode_width::UnicodeWidthStr;

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
/// - **Text**: leaf node, width = max display width among lines,
///   height = number of lines.
/// - **Button**: rendered as `[label]`, width = label.len() + 2, height = 1.
/// - **Input**: rendered as current value text, width = value.len(), height = 1.
/// - **Checkbox**: rendered as `[x]` or `[ ]`, width = 3, height = 1.
/// - **Conditional**: if child is present, laid out in-place; otherwise zero-size.
/// - **List**: transparent helper; its children are laid out by parent.
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

    let style = registry.get_node_style(root).cloned().unwrap_or_default();
    let inset = style_inset(&style);

    match node {
        UiNode::Text { content } => {
            let text = normalize_text_content(content.value());
            let (content_width, content_height) = text_dimensions(&text);
            let mut width = content_width.saturating_add(inset.saturating_mul(2));
            let mut height = content_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);
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
            let children = flatten_list_nodes(registry, children);
            let mut total_height: u16 = 0;
            let mut max_width: u16 = 0;
            let inner_row = origin_row.saturating_add(inset);
            let inner_col = origin_col.saturating_add(inset);

            for child_id in &children {
                let child_style = registry
                    .get_node_style(*child_id)
                    .cloned()
                    .unwrap_or_default();
                if child_style.position == PositionType::Fixed {
                    let _ = compute_layout(registry, *child_id, inner_row, inner_col, layout);
                    continue;
                }
                let (w, h) = compute_layout(
                    registry,
                    *child_id,
                    inner_row.saturating_add(total_height),
                    inner_col,
                    layout,
                );
                max_width = max_width.max(w);
                total_height = total_height.saturating_add(h);
            }

            let mut width = max_width.saturating_add(inset.saturating_mul(2));
            let mut height = total_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);

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

        UiNode::Stack { children } => {
            let children = flatten_list_nodes(registry, children);
            let mut max_width: u16 = 0;
            let mut max_height: u16 = 0;
            let inner_row = origin_row.saturating_add(inset);
            let inner_col = origin_col.saturating_add(inset);

            for child_id in &children {
                let child_style = registry
                    .get_node_style(*child_id)
                    .cloned()
                    .unwrap_or_default();
                if child_style.position == PositionType::Absolute {
                    let child_row =
                        inner_row.saturating_add(child_style.top.unwrap_or(0).max(0) as u16);
                    let child_col =
                        inner_col.saturating_add(child_style.left.unwrap_or(0).max(0) as u16);
                    let (w, h) = compute_layout(registry, *child_id, child_row, child_col, layout);
                    let rel_w = child_col.saturating_sub(inner_col).saturating_add(w);
                    let rel_h = child_row.saturating_sub(inner_row).saturating_add(h);
                    max_width = max_width.max(rel_w);
                    max_height = max_height.max(rel_h);
                } else if child_style.position == PositionType::Fixed {
                    let _ = compute_layout(registry, *child_id, inner_row, inner_col, layout);
                } else {
                    let (w, h) = compute_layout(registry, *child_id, inner_row, inner_col, layout);
                    max_width = max_width.max(w);
                    max_height = max_height.max(h);
                }
            }

            let mut width = max_width.saturating_add(inset.saturating_mul(2));
            let mut height = max_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);

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

        UiNode::FullScreen { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(
                    registry,
                    child_id,
                    origin_row.saturating_add(inset),
                    origin_col.saturating_add(inset),
                    layout,
                );
                let mut width = w.saturating_add(inset.saturating_mul(2));
                let mut height = h.saturating_add(inset.saturating_mul(2));
                apply_size_overrides(&style, &mut width, &mut height);
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
            } else {
                let mut width = inset.saturating_mul(2);
                let mut height = inset.saturating_mul(2);
                apply_size_overrides(&style, &mut width, &mut height);
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
        }

        UiNode::Row { children } => {
            let children = flatten_list_nodes(registry, children);
            let mut total_width: u16 = 0;
            let mut max_height: u16 = 0;
            let inner_row = origin_row.saturating_add(inset);
            let inner_col = origin_col.saturating_add(inset);

            for child_id in &children {
                let child_style = registry
                    .get_node_style(*child_id)
                    .cloned()
                    .unwrap_or_default();
                if child_style.position == PositionType::Fixed {
                    let _ = compute_layout(registry, *child_id, inner_row, inner_col, layout);
                    continue;
                }
                let (w, h) = compute_layout(
                    registry,
                    *child_id,
                    inner_row,
                    inner_col.saturating_add(total_width),
                    layout,
                );
                total_width = total_width.saturating_add(w);
                max_height = max_height.max(h);
            }

            let mut width = total_width.saturating_add(inset.saturating_mul(2));
            let mut height = max_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);

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

        UiNode::Button { label, .. } => {
            // Render as [label]
            let content_width = label.len() as u16 + 2;
            let content_height: u16 = 1;
            let mut width = content_width.saturating_add(inset.saturating_mul(2));
            let mut height = content_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);
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

        UiNode::Input { value, .. } => {
            let content_width = value.value().len().max(1) as u16;
            let content_height: u16 = 1;
            let mut width = content_width.saturating_add(inset.saturating_mul(2));
            let mut height = content_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);
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

        UiNode::Checkbox { .. } => {
            // Render as [x] or [ ]
            let content_width: u16 = 3;
            let content_height: u16 = 1;
            let mut width = content_width.saturating_add(inset.saturating_mul(2));
            let mut height = content_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);
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

        UiNode::Conditional { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(
                    registry,
                    child_id,
                    origin_row.saturating_add(inset),
                    origin_col.saturating_add(inset),
                    layout,
                );
                let mut width = w.saturating_add(inset.saturating_mul(2));
                let mut height = h.saturating_add(inset.saturating_mul(2));
                apply_size_overrides(&style, &mut width, &mut height);
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
            } else {
                let mut width = inset.saturating_mul(2);
                let mut height = inset.saturating_mul(2);
                apply_size_overrides(&style, &mut width, &mut height);
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
        }

        UiNode::KeyArea { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(
                    registry,
                    child_id,
                    origin_row.saturating_add(inset),
                    origin_col.saturating_add(inset),
                    layout,
                );
                let mut width = w.saturating_add(inset.saturating_mul(2));
                let mut height = h.saturating_add(inset.saturating_mul(2));
                apply_size_overrides(&style, &mut width, &mut height);
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
            } else {
                let mut width = inset.saturating_mul(2);
                let mut height = inset.saturating_mul(2);
                apply_size_overrides(&style, &mut width, &mut height);
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
        }

        UiNode::List { children, .. } => {
            // Like Column, but absolute-positioned children are out-of-flow
            let children = children.clone();
            let mut flow_height: u16 = 0;
            let mut flow_max_width: u16 = 0;
            let mut absolute_max_width: u16 = 0;
            let mut absolute_max_height: u16 = 0;
            let inner_row = origin_row.saturating_add(inset);
            let inner_col = origin_col.saturating_add(inset);

            for child_id in &children {
                let child_style = registry
                    .get_node_style(*child_id)
                    .cloned()
                    .unwrap_or_default();

                if child_style.position == PositionType::Absolute {
                    let child_row =
                        inner_row.saturating_add(child_style.top.unwrap_or(0).max(0) as u16);
                    let child_col =
                        inner_col.saturating_add(child_style.left.unwrap_or(0).max(0) as u16);
                    let (w, h) = compute_layout(registry, *child_id, child_row, child_col, layout);
                    let rel_w = child_col.saturating_sub(inner_col).saturating_add(w);
                    let rel_h = child_row.saturating_sub(inner_row).saturating_add(h);
                    absolute_max_width = absolute_max_width.max(rel_w);
                    absolute_max_height = absolute_max_height.max(rel_h);
                } else if child_style.position == PositionType::Fixed {
                    let _ = compute_layout(registry, *child_id, inner_row, inner_col, layout);
                } else {
                    let (w, h) = compute_layout(
                        registry,
                        *child_id,
                        inner_row.saturating_add(flow_height),
                        inner_col,
                        layout,
                    );
                    flow_max_width = flow_max_width.max(w);
                    flow_height = flow_height.saturating_add(h);
                }
            }

            let content_width = flow_max_width.max(absolute_max_width);
            let content_height = flow_height.max(absolute_max_height);

            let mut width = content_width.saturating_add(inset.saturating_mul(2));
            let mut height = content_height.saturating_add(inset.saturating_mul(2));
            apply_size_overrides(&style, &mut width, &mut height);

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

        UiNode::ScrollBox { child, .. } => {
            if let Some(child_id) = child {
                let child_id = *child_id;
                let (w, h) = compute_layout(
                    registry,
                    child_id,
                    origin_row.saturating_add(inset),
                    origin_col.saturating_add(inset),
                    layout,
                );
                let mut width = w.saturating_add(inset.saturating_mul(2));
                let mut height = h.saturating_add(inset.saturating_mul(2));
                apply_size_overrides(&style, &mut width, &mut height);
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
            } else {
                let mut width = inset.saturating_mul(2);
                let mut height = inset.saturating_mul(2);
                apply_size_overrides(&style, &mut width, &mut height);
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
        }

        UiNode::Image { .. } => {
            // Placeholder — images not supported in terminal yet
            let mut width = inset.saturating_mul(2);
            let mut height = inset.saturating_mul(2);
            apply_size_overrides(&style, &mut width, &mut height);
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
    }
}

pub fn resolve_full_sizes(registry: &UiRegistry, root: NodeId, layout: &mut LayoutMap) {
    resolve_full_sizes_inner(registry, root, None, layout);
}

pub fn resolve_alignment(registry: &UiRegistry, root: NodeId, layout: &mut LayoutMap) {
    resolve_alignment_inner(registry, root, layout);
}

pub fn resolve_fixed_positions(registry: &UiRegistry, root: NodeId, layout: &mut LayoutMap) {
    resolve_fixed_positions_inner(registry, root, layout);
}

fn resolve_full_sizes_inner(
    registry: &UiRegistry,
    node_id: NodeId,
    parent_id: Option<NodeId>,
    layout: &mut LayoutMap,
) {
    let Some(mut rect) = layout.get(node_id).copied() else {
        return;
    };

    if let Some(pid) = parent_id {
        let parent_rect = match layout.get(pid).copied() {
            Some(r) => r,
            None => return,
        };
        let parent_style = registry.get_node_style(pid).cloned().unwrap_or_default();
        let parent_inset = style_inset(&parent_style);

        let parent_inner_row = parent_rect.row.saturating_add(parent_inset);
        let parent_inner_col = parent_rect.col.saturating_add(parent_inset);
        let parent_inner_w = parent_rect
            .width
            .saturating_sub(parent_inset.saturating_mul(2));
        let parent_inner_h = parent_rect
            .height
            .saturating_sub(parent_inset.saturating_mul(2));

        let style = registry
            .get_node_style(node_id)
            .cloned()
            .unwrap_or_default();
        if matches!(style.width, Some(StyleSize::Full)) {
            let rel_col = rect.col.saturating_sub(parent_inner_col);
            rect.width = parent_inner_w.saturating_sub(rel_col);
        }
        if matches!(style.height, Some(StyleSize::Full)) {
            let rel_row = rect.row.saturating_sub(parent_inner_row);
            rect.height = parent_inner_h.saturating_sub(rel_row);
        }

        layout.set(node_id, rect);
    }

    for child in child_nodes(registry, node_id) {
        resolve_full_sizes_inner(registry, child, Some(node_id), layout);
    }
}

fn resolve_alignment_inner(registry: &UiRegistry, node_id: NodeId, layout: &mut LayoutMap) {
    let Some(node_rect) = layout.get(node_id).copied() else {
        return;
    };
    let style = registry
        .get_node_style(node_id)
        .cloned()
        .unwrap_or_default();
    let inset = style_inset(&style);

    let children_all = child_nodes(registry, node_id);
    let children = children_all
        .iter()
        .copied()
        .filter(|child_id| {
            let child_style = registry
                .get_node_style(*child_id)
                .cloned()
                .unwrap_or_default();
            if child_style.position == PositionType::Fixed {
                return false;
            }
            if matches!(registry.get_node(node_id), Some(UiNode::Stack { .. })) {
                return child_style.position != PositionType::Absolute;
            }
            true
        })
        .collect::<Vec<_>>();

    if !children.is_empty() {
        let mut min_row = u16::MAX;
        let mut min_col = u16::MAX;
        let mut max_row = 0u16;
        let mut max_col = 0u16;

        for child_id in &children {
            if let Some(rect) = layout.get(*child_id).copied() {
                min_row = min_row.min(rect.row);
                min_col = min_col.min(rect.col);
                max_row = max_row.max(rect.row.saturating_add(rect.height));
                max_col = max_col.max(rect.col.saturating_add(rect.width));
            }
        }

        if min_row != u16::MAX && min_col != u16::MAX {
            let inner_row = node_rect.row.saturating_add(inset);
            let inner_col = node_rect.col.saturating_add(inset);
            let inner_h = node_rect.height.saturating_sub(inset.saturating_mul(2));
            let inner_w = node_rect.width.saturating_sub(inset.saturating_mul(2));
            let content_h = max_row.saturating_sub(min_row);
            let content_w = max_col.saturating_sub(min_col);

            let target_col = match style.horizontal.as_deref() {
                Some("center") => {
                    if inner_w > content_w {
                        inner_col.saturating_add((inner_w - content_w) / 2)
                    } else {
                        inner_col
                    }
                }
                Some("right") => {
                    if inner_w > content_w {
                        inner_col.saturating_add(inner_w - content_w)
                    } else {
                        inner_col
                    }
                }
                _ => inner_col,
            };

            let target_row = match style.vertical.as_deref() {
                Some("center") => {
                    if inner_h > content_h {
                        inner_row.saturating_add((inner_h - content_h) / 2)
                    } else {
                        inner_row
                    }
                }
                Some("bottom") => {
                    if inner_h > content_h {
                        inner_row.saturating_add(inner_h - content_h)
                    } else {
                        inner_row
                    }
                }
                _ => inner_row,
            };

            let delta_row = target_row as i32 - min_row as i32;
            let delta_col = target_col as i32 - min_col as i32;

            if delta_row != 0 || delta_col != 0 {
                for child_id in &children {
                    offset_subtree(layout, registry, *child_id, delta_row, delta_col);
                }
            }
        }
    }

    for child in children_all {
        resolve_alignment_inner(registry, child, layout);
    }
}

fn resolve_fixed_positions_inner(registry: &UiRegistry, node_id: NodeId, layout: &mut LayoutMap) {
    let Some(parent_rect) = layout.get(node_id).copied() else {
        return;
    };
    let parent_style = registry
        .get_node_style(node_id)
        .cloned()
        .unwrap_or_default();
    let inset = style_inset(&parent_style);
    let inner_row = parent_rect.row.saturating_add(inset);
    let inner_col = parent_rect.col.saturating_add(inset);
    let inner_h = parent_rect.height.saturating_sub(inset.saturating_mul(2));
    let inner_w = parent_rect.width.saturating_sub(inset.saturating_mul(2));

    for child_id in child_nodes(registry, node_id) {
        let child_style = registry
            .get_node_style(child_id)
            .cloned()
            .unwrap_or_default();
        if child_style.position == PositionType::Fixed
            && let Some(child_rect) = layout.get(child_id).copied()
        {
            let target_row = if let Some(top) = child_style.top {
                inner_row.saturating_add(top.max(0) as u16)
            } else if let Some(bottom) = child_style.bottom {
                inner_row
                    .saturating_add(inner_h.saturating_sub(child_rect.height))
                    .saturating_sub(bottom.max(0) as u16)
            } else {
                child_rect.row
            };

            let target_col = if let Some(left) = child_style.left {
                inner_col.saturating_add(left.max(0) as u16)
            } else if let Some(right) = child_style.right {
                inner_col
                    .saturating_add(inner_w.saturating_sub(child_rect.width))
                    .saturating_sub(right.max(0) as u16)
            } else {
                child_rect.col
            };

            let delta_row = target_row as i32 - child_rect.row as i32;
            let delta_col = target_col as i32 - child_rect.col as i32;
            if delta_row != 0 || delta_col != 0 {
                offset_subtree(layout, registry, child_id, delta_row, delta_col);
            }
        }

        resolve_fixed_positions_inner(registry, child_id, layout);
    }
}

fn offset_subtree(
    layout: &mut LayoutMap,
    registry: &UiRegistry,
    node_id: NodeId,
    delta_row: i32,
    delta_col: i32,
) {
    if let Some(mut rect) = layout.get(node_id).copied() {
        rect.row = add_signed(rect.row, delta_row);
        rect.col = add_signed(rect.col, delta_col);
        layout.set(node_id, rect);
    }

    for child in child_nodes(registry, node_id) {
        offset_subtree(layout, registry, child, delta_row, delta_col);
    }
}

fn add_signed(value: u16, delta: i32) -> u16 {
    if delta >= 0 {
        value.saturating_add(delta as u16)
    } else {
        value.saturating_sub((-delta) as u16)
    }
}

fn child_nodes(registry: &UiRegistry, node_id: NodeId) -> Vec<NodeId> {
    let Some(node) = registry.get_node(node_id) else {
        return Vec::new();
    };

    let raw_children = match node {
        UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::View { children }
        | UiNode::Stack { children }
        | UiNode::List { children, .. } => children.clone(),
        UiNode::ScrollBox {
            child: Some(child),
            stick_bottom: _,
        } => vec![*child],
        UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. }
        | UiNode::FullScreen { child, .. } => child.iter().copied().collect(),
        _ => Vec::new(),
    };

    flatten_list_nodes(registry, &raw_children)
}

fn flatten_list_nodes(registry: &UiRegistry, children: &[NodeId]) -> Vec<NodeId> {
    let mut flattened = Vec::new();
    flatten_list_nodes_into(registry, children, &mut flattened);
    flattened
}

fn flatten_list_nodes_into(registry: &UiRegistry, children: &[NodeId], out: &mut Vec<NodeId>) {
    for child_id in children {
        match registry.get_node(*child_id) {
            Some(UiNode::List { children, .. }) => flatten_list_nodes_into(registry, children, out),
            _ => out.push(*child_id),
        }
    }
}

pub fn style_inset(style: &NodeStyle) -> u16 {
    let mut inset: u16 = 0;
    for op in &style.ops {
        match op {
            StyleOp::Padding(v) => inset = inset.saturating_add(*v),
            StyleOp::BorderWidth(v) => inset = inset.saturating_add(*v),
            StyleOp::BgColor(_) | StyleOp::BorderColor(_) => {}
        }
    }
    inset
}

fn apply_size_overrides(style: &NodeStyle, width: &mut u16, height: &mut u16) {
    if let Some(style_width) = style.width {
        match style_width {
            StyleSize::Px(v) => *width = (*width).max(v),
            StyleSize::Full => {}
        }
    }
    if let Some(style_height) = style.height {
        match style_height {
            StyleSize::Px(v) => *height = (*height).max(v),
            StyleSize::Full => {}
        }
    }
}

fn text_dimensions(text: &str) -> (u16, u16) {
    if text.is_empty() {
        return (0, 0);
    }

    let mut max_width: usize = 0;
    let mut height: usize = 0;
    for line in text.split('\n') {
        max_width = max_width.max(UnicodeWidthStr::width(line));
        height += 1;
    }

    (
        max_width.min(u16::MAX as usize) as u16,
        height.min(u16::MAX as usize) as u16,
    )
}

pub fn normalize_text_content(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let mut start = 0usize;
    let mut end = lines.len();

    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    if start >= end {
        String::new()
    } else if start == 0 && end == lines.len() {
        text.to_string()
    } else {
        lines[start..end].join("\n")
    }
}

/// Get the renderable text for a node (leaf content only).
/// Returns `None` for container nodes.
pub fn node_content(node: &UiNode) -> Option<String> {
    match node {
        UiNode::Text { content } => Some(normalize_text_content(content.value())),
        UiNode::Button { label, .. } => Some(format!("[{}]", label)),
        UiNode::Input { value, .. } => Some(value.value().to_string()),
        UiNode::Checkbox { checked, .. } => Some(if *checked { "[x]" } else { "[ ]" }.to_string()),
        // Container nodes have no direct content
        UiNode::Column { .. }
        | UiNode::Row { .. }
        | UiNode::View { .. }
        | UiNode::ScrollBox { .. }
        | UiNode::Stack { .. }
        | UiNode::FullScreen { .. }
        | UiNode::Conditional { .. }
        | UiNode::KeyArea { .. }
        | UiNode::List { .. }
        | UiNode::Image { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::{NodeStyle, PositionType, StyleSize, TextContent, UiNode};

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
    fn test_multiline_text_node_trims_edges() {
        let (registry, root) = build_registry(UiNode::Text {
            content: TextContent::Static("\nline 1\nline 2\n\n".into()),
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 6);
        assert_eq!(h, 2);
    }

    #[test]
    fn test_unicode_text_uses_display_width() {
        let (registry, root) = build_registry(UiNode::Text {
            content: TextContent::Static("▐▛███▜▌".into()),
        });

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, root, 0, 0, &mut layout);

        assert_eq!(w, 7);
        assert_eq!(h, 1);
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
    fn test_node_content_text_trims_edge_blank_lines() {
        let node = UiNode::Text {
            content: TextContent::Static("\n  a\n b\n\n".into()),
        };
        assert_eq!(node_content(&node).unwrap(), "  a\n b");
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

    #[test]
    fn test_stack_absolute_child_offset() {
        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("x".into()),
        });
        let stack = registry.create_node(UiNode::Stack {
            children: vec![child],
        });
        registry.set_root(stack);

        let child_style = NodeStyle {
            position: PositionType::Absolute,
            top: Some(2),
            left: Some(3),
            ..NodeStyle::default()
        };
        registry.set_node_style(child, child_style);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, stack, 0, 0, &mut layout);

        let rect = layout.get(child).unwrap();
        assert_eq!(rect.row, 2);
        assert_eq!(rect.col, 3);
    }

    #[test]
    fn test_list_absolute_child_offset() {
        use rover_ui::signal::graph::EffectId;

        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("x".into()),
        });
        let list = registry.create_node(UiNode::List {
            items_effect: EffectId(0),
            children: vec![child],
        });
        registry.set_root(list);

        let child_style = NodeStyle {
            position: PositionType::Absolute,
            top: Some(2),
            left: Some(3),
            ..NodeStyle::default()
        };
        registry.set_node_style(child, child_style);

        let mut layout = LayoutMap::new();
        let (w, h) = compute_layout(&registry, list, 0, 0, &mut layout);

        let rect = layout.get(child).unwrap();
        assert_eq!(rect.row, 2);
        assert_eq!(rect.col, 3);
        assert_eq!(w, 4);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_stack_flattens_list_children() {
        use rover_ui::signal::graph::EffectId;

        let mut registry = UiRegistry::new();
        let piece = registry.create_node(UiNode::Text {
            content: TextContent::Static("x".into()),
        });
        let list = registry.create_node(UiNode::List {
            items_effect: EffectId(0),
            children: vec![piece],
        });
        let stack = registry.create_node(UiNode::Stack {
            children: vec![list],
        });
        registry.set_root(stack);

        let piece_style = NodeStyle {
            position: PositionType::Absolute,
            top: Some(5),
            left: Some(7),
            ..NodeStyle::default()
        };
        registry.set_node_style(piece, piece_style);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, stack, 0, 0, &mut layout);

        let rect = layout.get(piece).unwrap();
        assert_eq!(rect.row, 5);
        assert_eq!(rect.col, 7);
    }

    #[test]
    fn test_view_horizontal_vertical_center() {
        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("ok".into()),
        });
        let view = registry.create_node(UiNode::View {
            children: vec![child],
        });
        registry.set_root(view);

        let style = NodeStyle {
            width: Some(StyleSize::Px(20)),
            height: Some(StyleSize::Px(10)),
            horizontal: Some("center".to_string()),
            vertical: Some("center".to_string()),
            ..NodeStyle::default()
        };
        registry.set_node_style(view, style);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, view, 0, 0, &mut layout);
        resolve_full_sizes(&registry, view, &mut layout);
        resolve_alignment(&registry, view, &mut layout);

        let child_rect = layout.get(child).unwrap();
        assert_eq!(child_rect.col, 9);
        assert_eq!(child_rect.row, 4);
    }
}
