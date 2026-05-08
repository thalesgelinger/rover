use crate::abi::{HostCallbacks, NativeViewHandle, NativeViewKind};
use crate::layout::{LayoutMap, Rect, compute_layout};
use rover_ui::platform::UiTarget;
use rover_ui::ui::{NodeId, Renderer, StyleOp, UiNode, UiRegistry};
use std::collections::HashSet;
use std::ffi::c_void;

const DEFAULT_WIDTH: f32 = 900.0;
const DEFAULT_HEIGHT: f32 = 640.0;
const SYNTHETIC_WINDOW_ID: u32 = u32::MAX;

pub struct MacosRenderer {
    callbacks: HostCallbacks,
    handles: Vec<Option<NativeViewHandle>>,
    layout: LayoutMap,
    viewport_width: f32,
    viewport_height: f32,
    default_window: Option<NativeViewHandle>,
    attached_edges: HashSet<(u32, u32)>,
}

impl MacosRenderer {
    pub fn new(callbacks: HostCallbacks) -> Self {
        Self {
            callbacks,
            handles: Vec::new(),
            layout: LayoutMap::new(),
            viewport_width: DEFAULT_WIDTH,
            viewport_height: DEFAULT_HEIGHT,
            default_window: None,
            attached_edges: HashSet::new(),
        }
    }

    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.viewport_width = width.max(1.0);
        self.viewport_height = height.max(1.0);
    }

    fn set_handle(&mut self, node_id: NodeId, handle: NativeViewHandle) {
        let idx = node_id.index();
        if idx >= self.handles.len() {
            self.handles.resize(idx + 1, None);
        }
        self.handles[idx] = Some(handle);
    }

    fn handle(&self, node_id: NodeId) -> Option<NativeViewHandle> {
        self.handles.get(node_id.index()).and_then(|entry| *entry)
    }

    fn create_subtree(
        &mut self,
        registry: &UiRegistry,
        node_id: NodeId,
    ) -> Option<NativeViewHandle> {
        let node = registry.get_node(node_id)?;
        if let Some(handle) = self.handle(node_id) {
            return Some(handle);
        }

        let handle = self.create_view(node_id, kind_for_node(node));
        self.set_handle(node_id, handle);
        self.apply_node_props(node_id, node, handle);

        for child in children_for_node(node) {
            if let Some(child_handle) = self.create_subtree(registry, child) {
                if let Some(append_child) = self.callbacks.append_child {
                    append_child(handle, child_handle);
                }
            }
        }

        Some(handle)
    }

    fn mount_root(&mut self, registry: &UiRegistry, root: NodeId) {
        if matches!(registry.get_node(root), Some(UiNode::MacosWindow { .. })) {
            self.create_subtree(registry, root);
            return;
        }

        let window = self.default_window.unwrap_or_else(|| {
            let handle = self.create_view_raw(SYNTHETIC_WINDOW_ID, NativeViewKind::Window);
            self.set_window(handle, "Rover", DEFAULT_WIDTH, DEFAULT_HEIGHT);
            self.default_window = Some(handle);
            handle
        });

        if let Some(root_handle) = self.create_subtree(registry, root) {
            self.append_child_once(
                SYNTHETIC_WINDOW_ID,
                root.index() as u32,
                window,
                root_handle,
            );
        }
    }

    fn sync_subtree(&mut self, registry: &UiRegistry, node_id: NodeId) {
        let Some(node) = registry.get_node(node_id) else {
            return;
        };
        let Some(parent_handle) = self.handle(node_id) else {
            return;
        };

        for child in children_for_node(node) {
            if self.handle(child).is_none() {
                self.create_subtree(registry, child);
            }
            if let Some(child_handle) = self.handle(child) {
                self.append_child_once(
                    node_id.index() as u32,
                    child.index() as u32,
                    parent_handle,
                    child_handle,
                );
            }
            self.sync_subtree(registry, child);
        }
    }

    fn append_child_once(
        &mut self,
        parent_id: u32,
        child_id: u32,
        parent: NativeViewHandle,
        child: NativeViewHandle,
    ) {
        if !self.attached_edges.insert((parent_id, child_id)) {
            return;
        }
        if let Some(append_child) = self.callbacks.append_child {
            append_child(parent, child);
        }
    }

    fn create_view(&self, node_id: NodeId, kind: NativeViewKind) -> NativeViewHandle {
        self.create_view_raw(node_id.index() as u32, kind)
    }

    fn create_view_raw(&self, node_id: u32, kind: NativeViewKind) -> NativeViewHandle {
        if let Some(create_view) = self.callbacks.create_view {
            create_view(node_id, kind)
        } else {
            std::ptr::null_mut::<c_void>()
        }
    }

    fn apply_node_props(&self, node_id: NodeId, node: &UiNode, handle: NativeViewHandle) {
        match node {
            UiNode::Text { content } => self.set_text(handle, content.value()),
            UiNode::Button { label, .. } => self.set_text(handle, label),
            UiNode::Input { value, .. } => self.set_text(handle, value.value()),
            UiNode::Checkbox { checked, .. } => {
                if let Some(set_bool) = self.callbacks.set_bool {
                    set_bool(handle, *checked);
                }
            }
            UiNode::Image { src } => self.set_text(handle, src),
            UiNode::MacosWindow {
                title,
                width,
                height,
                ..
            } => {
                if let Some(set_window) = self.callbacks.set_window {
                    set_window(
                        handle,
                        title.as_ptr().cast(),
                        title.len(),
                        *width as f32,
                        *height as f32,
                    );
                }
            }
            _ => {
                let _ = node_id;
            }
        }
    }

    fn set_text(&self, handle: NativeViewHandle, text: &str) {
        if let Some(set_text) = self.callbacks.set_text {
            set_text(handle, text.as_ptr().cast(), text.len());
        }
    }

    fn set_window(&self, handle: NativeViewHandle, title: &str, width: f32, height: f32) {
        if let Some(set_window) = self.callbacks.set_window {
            set_window(handle, title.as_ptr().cast(), title.len(), width, height);
        }
    }

    fn apply_layout(&self, registry: &UiRegistry, node_id: NodeId) {
        self.apply_layout_relative_to(registry, node_id, None);
    }

    fn apply_layout_relative_to(
        &self,
        registry: &UiRegistry,
        node_id: NodeId,
        parent_rect: Option<Rect>,
    ) {
        let Some(node) = registry.get_node(node_id) else {
            return;
        };
        let Some(rect) = self.layout.get(node_id) else {
            return;
        };

        if let Some(handle) = self.handle(node_id) {
            self.apply_style(registry, node_id, handle);
        }

        if !matches!(node, UiNode::MacosWindow { .. }) {
            if let (Some(handle), Some(set_frame)) =
                (self.handle(node_id), self.callbacks.set_frame)
            {
                let frame = rect.relative_to(parent_rect.unwrap_or_default());
                set_frame(handle, frame.x, frame.y, frame.width, frame.height);
            }
        }

        for child in children_for_node(node) {
            self.apply_layout_relative_to(registry, child, Some(rect));
        }
    }

    fn apply_style(&self, registry: &UiRegistry, node_id: NodeId, handle: NativeViewHandle) {
        let Some(set_style) = self.callbacks.set_style else {
            return;
        };
        let Some(style) = registry.get_node_style(node_id) else {
            return;
        };

        let mut bg = "";
        let mut border = "";
        let mut border_width = 0.0;

        for op in &style.ops {
            match op {
                StyleOp::BgColor(value) => bg = value,
                StyleOp::BorderColor(value) => border = value,
                StyleOp::BorderWidth(value) => border_width = *value as f32,
                StyleOp::Padding(_) => {}
            }
        }

        let text = style.color.as_deref().unwrap_or("");
        set_style(
            handle,
            bg.as_ptr().cast(),
            bg.len(),
            border.as_ptr().cast(),
            border.len(),
            border_width,
            text.as_ptr().cast(),
            text.len(),
        );
    }
}

impl Renderer for MacosRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        if let Some(root) = registry.root() {
            self.mount_root(registry, root);
            self.sync_subtree(registry, root);
            self.layout = compute_layout(registry, root, self.viewport_width, self.viewport_height);
            self.apply_layout(registry, root);
        }
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        if let Some(root) = registry.root() {
            self.mount_root(registry, root);
            self.sync_subtree(registry, root);
            self.layout = compute_layout(registry, root, self.viewport_width, self.viewport_height);
            self.apply_layout(registry, root);
        }

        for node_id in dirty_nodes {
            let Some(node) = registry.get_node(*node_id) else {
                continue;
            };
            if self.handle(*node_id).is_none() {
                self.create_subtree(registry, *node_id);
            }
            if let Some(handle) = self.handle(*node_id) {
                self.apply_node_props(*node_id, node, handle);
            }
        }
    }

    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId) {
        self.create_subtree(registry, node_id);
    }

    fn node_removed(&mut self, node_id: NodeId) {
        if let Some(handle) = self.handle(node_id) {
            if let Some(remove_view) = self.callbacks.remove_view {
                remove_view(handle);
            }
        }
        if let Some(entry) = self.handles.get_mut(node_id.index()) {
            *entry = None;
        }
    }

    fn target(&self) -> UiTarget {
        UiTarget::Macos
    }
}

fn kind_for_node(node: &UiNode) -> NativeViewKind {
    match node {
        UiNode::Text { .. } => NativeViewKind::Text,
        UiNode::Button { .. } => NativeViewKind::Button,
        UiNode::Input { .. } => NativeViewKind::Input,
        UiNode::Checkbox { .. } => NativeViewKind::Checkbox,
        UiNode::Image { .. } => NativeViewKind::Image,
        UiNode::Row { .. } => NativeViewKind::Row,
        UiNode::MacosWindow { .. } => NativeViewKind::Window,
        UiNode::MacosScrollView { .. } | UiNode::ScrollBox { .. } => NativeViewKind::ScrollView,
        _ => NativeViewKind::Column,
    }
}

fn children_for_node(node: &UiNode) -> Vec<NodeId> {
    match node {
        UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::View { children }
        | UiNode::Stack { children }
        | UiNode::List { children, .. }
        | UiNode::MacosWindow { children, .. }
        | UiNode::MacosScrollView { children } => children.clone(),
        UiNode::ScrollBox { child, .. }
        | UiNode::FullScreen { child, .. }
        | UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. } => child.iter().copied().collect(),
        _ => Vec::new(),
    }
}

unsafe impl Send for MacosRenderer {}
