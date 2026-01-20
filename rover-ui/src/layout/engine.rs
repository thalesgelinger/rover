use crate::node::{Node, NodeArena, NodeId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComputedLayout {
    pub rect: Rect,
}

pub struct LayoutEngine {
    computed: HashMap<NodeId, ComputedLayout>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            computed: HashMap::new(),
        }
    }

    pub fn compute(&mut self, root: NodeId, arena: &NodeArena, available: Size) {
        self.computed.clear();
        self.compute_node(
            root,
            arena,
            Rect::new(0, 0, available.width, available.height),
        );
    }

    fn compute_node(&mut self, node: NodeId, arena: &NodeArena, rect: Rect) {
        self.computed
            .insert(node, ComputedLayout { rect: rect.clone() });

        if let Some(n) = arena.get(node) {
            match n {
                Node::Column(_) => self.compute_column(node, arena, &rect),
                Node::Row(_) => self.compute_row(node, arena, &rect),
                Node::Text(_) | Node::Conditional(_) | Node::Each(_) => {}
            }
        }
    }

    fn compute_column(&mut self, node: NodeId, arena: &NodeArena, rect: &Rect) {
        let children = arena.children(node);
        let child_count = children.len();
        if child_count == 0 {
            return;
        }

        let child_height = rect.height / child_count as u16;
        let mut current_y = rect.y;

        for child in children {
            let child_rect = Rect::new(rect.x, current_y, rect.width, child_height);
            self.compute_node(child, arena, child_rect);
            current_y += child_height;
        }
    }

    fn compute_row(&mut self, node: NodeId, arena: &NodeArena, rect: &Rect) {
        let children = arena.children(node);
        let child_count = children.len();
        if child_count == 0 {
            return;
        }

        let child_width = rect.width / child_count as u16;
        let mut current_x = rect.x;

        for child in children {
            let child_rect = Rect::new(current_x, rect.y, child_width, rect.height);
            self.compute_node(child, arena, child_rect);
            current_x += child_width;
        }
    }

    pub fn get_layout(&self, node: NodeId) -> Option<&ComputedLayout> {
        self.computed.get(&node)
    }

    pub fn clear(&mut self) {
        self.computed.clear();
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}
