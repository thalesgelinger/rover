use crate::abi::{HostCallbacks, NativeViewHandle, NativeViewKind};
use rover_apple::{AppleStyle, LayoutMap, Rect, compute_layout};
use rover_ui::platform::UiTarget;
use rover_ui::ui::{NodeId, Renderer, UiNode, UiRegistry};
use std::collections::HashSet;
use std::ffi::c_void;

const DEFAULT_WIDTH: f32 = 390.0;
const DEFAULT_HEIGHT: f32 = 844.0;
const SYNTHETIC_ROOT_ID: u32 = u32::MAX;

pub struct IosRenderer {
    callbacks: HostCallbacks,
    handles: Vec<Option<NativeViewHandle>>,
    layout: LayoutMap,
    viewport_width: f32,
    viewport_height: f32,
    root_view: Option<NativeViewHandle>,
    attached_edges: HashSet<(u32, u32)>,
}

impl IosRenderer {
    pub fn new(callbacks: HostCallbacks) -> Self {
        Self {
            callbacks,
            handles: Vec::new(),
            layout: LayoutMap::new(),
            viewport_width: DEFAULT_WIDTH,
            viewport_height: DEFAULT_HEIGHT,
            root_view: None,
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
        self.apply_node_props(node, handle);

        for child in children_for_node(node) {
            if let Some(child_handle) = self.create_subtree(registry, child) {
                self.append_child_once(
                    node_id.index() as u32,
                    child.index() as u32,
                    handle,
                    child_handle,
                );
            }
        }

        Some(handle)
    }

    fn mount_root(&mut self, registry: &UiRegistry, root: NodeId) {
        let root_view = self.root_view.unwrap_or_else(|| {
            let handle = self.create_view_raw(SYNTHETIC_ROOT_ID, NativeViewKind::Window);
            self.root_view = Some(handle);
            handle
        });

        if let Some(root_handle) = self.create_subtree(registry, root) {
            self.append_child_once(
                SYNTHETIC_ROOT_ID,
                root.index() as u32,
                root_view,
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

    fn apply_node_props(&self, node: &UiNode, handle: NativeViewHandle) {
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
            } => self.set_window(handle, title, *width as f32, *height as f32),
            _ => {}
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

        if let (Some(handle), Some(set_frame)) = (self.handle(node_id), self.callbacks.set_frame) {
            let frame = rect.relative_to(parent_rect.unwrap_or_default());
            set_frame(handle, frame.x, frame.y, frame.width, frame.height);
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

        set_style(handle, AppleStyle::from_node_style(style));
    }
}

impl Renderer for IosRenderer {
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
                self.apply_node_props(node, handle);
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
        UiTarget::Ios
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
        UiNode::ScrollView { .. } | UiNode::ScrollBox { .. } => NativeViewKind::ScrollView,
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
        | UiNode::ScrollView { children }
        | UiNode::MacosWindow { children, .. } => children.clone(),
        UiNode::ScrollBox { child, .. }
        | UiNode::FullScreen { child, .. }
        | UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. } => child.iter().copied().collect(),
        _ => Vec::new(),
    }
}

unsafe impl Send for IosRenderer {}
