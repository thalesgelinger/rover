use rover_ui::ui::{NodeId, NodeStyle, StyleOp, StyleSize, UiNode, UiRegistry};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn relative_to(self, parent: Rect) -> Self {
        Self {
            x: self.x - parent.x,
            y: self.y - parent.y,
            width: self.width,
            height: self.height,
        }
    }
}

pub struct LayoutMap {
    entries: Vec<Option<Rect>>,
}

impl LayoutMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn set(&mut self, id: NodeId, rect: Rect) {
        let idx = id.index();
        if idx >= self.entries.len() {
            self.entries.resize(idx + 1, None);
        }
        self.entries[idx] = Some(rect);
    }

    pub fn get(&self, id: NodeId) -> Option<Rect> {
        self.entries.get(id.index()).and_then(|entry| *entry)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for LayoutMap {
    fn default() -> Self {
        Self::new()
    }
}

pub fn compute_layout(
    registry: &UiRegistry,
    root: NodeId,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutMap {
    let mut layout = LayoutMap::new();
    compute_node(
        registry,
        root,
        Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_width,
            height: viewport_height,
        },
        &mut layout,
    );
    layout
}

fn compute_node(
    registry: &UiRegistry,
    node_id: NodeId,
    available: Rect,
    layout: &mut LayoutMap,
) -> Rect {
    let Some(node) = registry.get_node(node_id) else {
        return Rect::default();
    };
    let style = registry
        .get_node_style(node_id)
        .cloned()
        .unwrap_or_default();
    let padding = padding(&style);
    let gap = style.gap.unwrap_or(0) as f32;

    let mut rect = match node {
        UiNode::Text { content } => leaf_rect(&style, available, text_width(content.value()), 20.0),
        UiNode::Button { label, .. } => {
            leaf_rect(&style, available, text_width(label) + 28.0, 30.0)
        }
        UiNode::Input { value, .. } => leaf_rect(
            &style,
            available,
            text_width(value.value()).max(160.0),
            28.0,
        ),
        UiNode::Checkbox { .. } => leaf_rect(&style, available, 22.0, 22.0),
        UiNode::Image { .. } => leaf_rect(&style, available, 0.0, 0.0),
        UiNode::Row { children } => {
            layout_row(registry, node_id, children, available, padding, gap, layout)
        }
        UiNode::Column { children }
        | UiNode::View { children }
        | UiNode::Stack { children }
        | UiNode::List { children, .. }
        | UiNode::ScrollView { children }
        | UiNode::MacosWindow { children, .. } => {
            layout_column(registry, node_id, children, available, padding, gap, layout)
        }
        UiNode::ScrollBox { child, .. }
        | UiNode::FullScreen { child, .. }
        | UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. } => {
            let children: Vec<NodeId> = child.iter().copied().collect();
            layout_column(
                registry, node_id, &children, available, padding, gap, layout,
            )
        }
    };

    apply_size(&style, available, &mut rect);
    layout.set(node_id, rect);
    rect
}

fn layout_column(
    registry: &UiRegistry,
    node_id: NodeId,
    children: &[NodeId],
    available: Rect,
    padding: f32,
    gap: f32,
    layout: &mut LayoutMap,
) -> Rect {
    let mut y = available.y + padding;
    let inner_width = (available.width - padding * 2.0).max(0.0);
    let mut used_height: f32 = padding * 2.0;
    let mut max_width: f32 = 0.0;
    let mut child_rects = Vec::with_capacity(children.len());

    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            y += gap;
            used_height += gap;
        }
        let child_rect = compute_node(
            registry,
            *child,
            Rect {
                x: available.x + padding,
                y,
                width: inner_width,
                height: available.height,
            },
            layout,
        );
        y += child_rect.height;
        used_height += child_rect.height;
        max_width = max_width.max(child_rect.width);
        child_rects.push((*child, child_rect));
    }

    let style = registry
        .get_node_style(node_id)
        .cloned()
        .unwrap_or_default();
    let mut rect = Rect {
        x: available.x,
        y: available.y,
        width: (max_width + padding * 2.0).max(available.width),
        height: used_height.max(0.0),
    };
    apply_size(&style, available, &mut rect);
    align_column_children(
        registry,
        &style,
        &child_rects,
        rect,
        padding,
        used_height,
        layout,
    );
    rect
}

fn layout_row(
    registry: &UiRegistry,
    node_id: NodeId,
    children: &[NodeId],
    available: Rect,
    padding: f32,
    gap: f32,
    layout: &mut LayoutMap,
) -> Rect {
    let mut x = available.x + padding;
    let inner_height = (available.height - padding * 2.0).max(0.0);
    let mut used_width: f32 = padding * 2.0;
    let mut max_height: f32 = 0.0;
    let mut child_rects = Vec::with_capacity(children.len());

    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            x += gap;
            used_width += gap;
        }
        let child_rect = compute_node(
            registry,
            *child,
            Rect {
                x,
                y: available.y + padding,
                width: available.width,
                height: inner_height,
            },
            layout,
        );
        x += child_rect.width;
        used_width += child_rect.width;
        max_height = max_height.max(child_rect.height);
        child_rects.push((*child, child_rect));
    }

    let style = registry
        .get_node_style(node_id)
        .cloned()
        .unwrap_or_default();
    let mut rect = Rect {
        x: available.x,
        y: available.y,
        width: used_width,
        height: (max_height + padding * 2.0).max(0.0),
    };
    apply_size(&style, available, &mut rect);
    align_row_children(
        registry,
        &style,
        &child_rects,
        rect,
        padding,
        used_width,
        layout,
    );
    rect
}

fn align_column_children(
    registry: &UiRegistry,
    style: &NodeStyle,
    child_rects: &[(NodeId, Rect)],
    rect: Rect,
    padding: f32,
    used_height: f32,
    layout: &mut LayoutMap,
) {
    let main_offset = main_axis_offset(style.justify.as_deref(), rect.height - used_height);
    for (child, child_rect) in child_rects {
        let cross_offset = cross_axis_offset(
            style.align.as_deref(),
            rect.width,
            child_rect.width,
            padding,
        );
        translate_subtree(registry, layout, *child, cross_offset, main_offset);
    }
}

fn align_row_children(
    registry: &UiRegistry,
    style: &NodeStyle,
    child_rects: &[(NodeId, Rect)],
    rect: Rect,
    padding: f32,
    used_width: f32,
    layout: &mut LayoutMap,
) {
    let main_offset = main_axis_offset(style.justify.as_deref(), rect.width - used_width);
    for (child, child_rect) in child_rects {
        let cross_offset = cross_axis_offset(
            style.align.as_deref(),
            rect.height,
            child_rect.height,
            padding,
        );
        translate_subtree(registry, layout, *child, main_offset, cross_offset);
    }
}

fn main_axis_offset(value: Option<&str>, free_space: f32) -> f32 {
    let free_space = free_space.max(0.0);
    match value {
        Some("center") => free_space / 2.0,
        Some("end") | Some("flex-end") => free_space,
        _ => 0.0,
    }
}

fn cross_axis_offset(value: Option<&str>, parent_size: f32, child_size: f32, padding: f32) -> f32 {
    let free_space = (parent_size - padding * 2.0 - child_size).max(0.0);
    match value {
        Some("center") => free_space / 2.0,
        Some("end") | Some("flex-end") => free_space,
        _ => 0.0,
    }
}

fn translate_subtree(
    registry: &UiRegistry,
    layout: &mut LayoutMap,
    node_id: NodeId,
    dx: f32,
    dy: f32,
) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    if let Some(mut rect) = layout.get(node_id) {
        rect.x += dx;
        rect.y += dy;
        layout.set(node_id, rect);
    }
    if let Some(node) = registry.get_node(node_id) {
        for child in children_for_node(node) {
            translate_subtree(registry, layout, child, dx, dy);
        }
    }
}

fn children_for_node(node: &UiNode) -> Vec<NodeId> {
    match node {
        UiNode::View { children }
        | UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::Stack { children }
        | UiNode::List { children, .. }
        | UiNode::ScrollView { children }
        | UiNode::MacosWindow { children, .. } => children.clone(),
        UiNode::ScrollBox { child, .. }
        | UiNode::FullScreen { child, .. }
        | UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. } => child.iter().copied().collect(),
        _ => Vec::new(),
    }
}

fn leaf_rect(
    style: &NodeStyle,
    available: Rect,
    intrinsic_width: f32,
    intrinsic_height: f32,
) -> Rect {
    let padding = padding(style);
    let mut rect = Rect {
        x: available.x,
        y: available.y,
        width: intrinsic_width + padding * 2.0,
        height: intrinsic_height + padding * 2.0,
    };
    apply_size(style, available, &mut rect);
    rect
}

fn apply_size(style: &NodeStyle, available: Rect, rect: &mut Rect) {
    if let Some(width) = style.width {
        rect.width = match width {
            StyleSize::Full => available.width,
            StyleSize::Px(value) => value as f32,
        };
    }
    if let Some(height) = style.height {
        rect.height = match height {
            StyleSize::Full => available.height,
            StyleSize::Px(value) => value as f32,
        };
    }
}

fn padding(style: &NodeStyle) -> f32 {
    style
        .ops
        .iter()
        .rev()
        .find_map(|op| match op {
            StyleOp::Padding(value) => Some(*value as f32),
            _ => None,
        })
        .unwrap_or(0.0)
}

fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * 7.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::{TextContent, UiRegistry};

    #[test]
    fn lays_out_column_children_in_px() {
        let mut registry = UiRegistry::new();
        let first = registry.create_node(UiNode::Text {
            content: TextContent::Static("a".to_string()),
        });
        let second = registry.create_node(UiNode::Text {
            content: TextContent::Static("b".to_string()),
        });
        let root = registry.create_node(UiNode::Column {
            children: vec![first, second],
        });

        let layout = compute_layout(&registry, root, 900.0, 640.0);

        assert_eq!(layout.get(first).unwrap().y, 0.0);
        assert_eq!(layout.get(second).unwrap().y, 20.0);
    }

    #[test]
    fn centers_column_child_on_both_axes() {
        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("x".to_string()),
        });
        let root = registry.create_node(UiNode::Column {
            children: vec![child],
        });
        registry.set_node_style(
            root,
            NodeStyle {
                width: Some(StyleSize::Px(100)),
                height: Some(StyleSize::Px(100)),
                justify: Some("center".to_string()),
                align: Some("center".to_string()),
                ..Default::default()
            },
        );

        let layout = compute_layout(&registry, root, 100.0, 100.0);
        let child_rect = layout.get(child).unwrap();

        assert_eq!(child_rect.x, 46.25);
        assert_eq!(child_rect.y, 40.0);
    }

    #[test]
    fn converts_absolute_rect_to_parent_relative_frame() {
        let parent = Rect {
            x: 24.0,
            y: 62.0,
            width: 252.0,
            height: 100.0,
        };
        let child = Rect {
            x: 24.0,
            y: 62.0,
            width: 252.0,
            height: 80.0,
        };

        assert_eq!(
            child.relative_to(parent),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 252.0,
                height: 80.0,
            }
        );
    }
}
