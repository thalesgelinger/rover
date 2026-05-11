use jni::objects::{GlobalRef, JValue};
use jni::{AttachGuard, JavaVM};
use rover_native::{LayoutMap, NativeStyle, Rect, compute_layout};
use rover_ui::platform::UiTarget;
use rover_ui::ui::{NodeId, Renderer, UiNode, UiRegistry};
use std::collections::HashSet;

const DEFAULT_WIDTH: f32 = 390.0;
const DEFAULT_HEIGHT: f32 = 844.0;
const SYNTHETIC_ROOT_ID: u32 = u32::MAX;

pub type AndroidViewHandle = i64;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidViewKind {
    Root = 0,
    View = 1,
    Column = 2,
    Row = 3,
    Text = 4,
    Button = 5,
    Input = 6,
    Checkbox = 7,
    Image = 8,
    ScrollView = 9,
}

impl AndroidViewKind {
    fn as_i32(self) -> i32 {
        self as i32
    }
}

pub struct AndroidRenderer {
    vm: JavaVM,
    host: GlobalRef,
    handles: Vec<Option<AndroidViewHandle>>,
    layout: LayoutMap,
    viewport_width: f32,
    viewport_height: f32,
    root_view: Option<AndroidViewHandle>,
    attached_edges: HashSet<(u32, u32)>,
}

impl AndroidRenderer {
    pub fn new(vm: JavaVM, host: GlobalRef) -> Self {
        Self {
            vm,
            host,
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

    fn env(&self) -> Option<AttachGuard<'_>> {
        self.vm.attach_current_thread().ok()
    }

    fn set_handle(&mut self, node_id: NodeId, handle: AndroidViewHandle) {
        let idx = node_id.index();
        if idx >= self.handles.len() {
            self.handles.resize(idx + 1, None);
        }
        self.handles[idx] = Some(handle);
    }

    fn handle(&self, node_id: NodeId) -> Option<AndroidViewHandle> {
        self.handles.get(node_id.index()).and_then(|entry| *entry)
    }

    fn create_subtree(
        &mut self,
        registry: &UiRegistry,
        node_id: NodeId,
    ) -> Option<AndroidViewHandle> {
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
            let handle = self.create_view_raw(SYNTHETIC_ROOT_ID, AndroidViewKind::Root);
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
        parent: AndroidViewHandle,
        child: AndroidViewHandle,
    ) {
        if !self.attached_edges.insert((parent_id, child_id)) {
            return;
        }
        let Some(mut env) = self.env() else {
            return;
        };
        let _ = env.call_method(
            self.host.as_obj(),
            "appendChild",
            "(JJ)V",
            &[JValue::Long(parent), JValue::Long(child)],
        );
    }

    fn create_view(&self, node_id: NodeId, kind: AndroidViewKind) -> AndroidViewHandle {
        self.create_view_raw(node_id.index() as u32, kind)
    }

    fn create_view_raw(&self, node_id: u32, kind: AndroidViewKind) -> AndroidViewHandle {
        let Some(mut env) = self.env() else {
            return node_id as AndroidViewHandle;
        };
        env.call_method(
            self.host.as_obj(),
            "createView",
            "(JI)J",
            &[JValue::Long(node_id as i64), JValue::Int(kind.as_i32())],
        )
        .and_then(|value| value.j())
        .unwrap_or(node_id as AndroidViewHandle)
    }

    fn apply_node_props(&self, node: &UiNode, handle: AndroidViewHandle) {
        match node {
            UiNode::Text { content } => self.set_text(handle, content.value()),
            UiNode::Button { label, .. } => self.set_text(handle, label),
            UiNode::Input { value, .. } => self.set_text(handle, value.value()),
            UiNode::Checkbox { checked, .. } => self.set_bool(handle, *checked),
            UiNode::Image { src } => self.set_text(handle, src),
            _ => {}
        }
    }

    fn set_text(&self, handle: AndroidViewHandle, text: &str) {
        let Some(mut env) = self.env() else {
            return;
        };
        let Ok(text) = env.new_string(text) else {
            return;
        };
        let _ = env.call_method(
            self.host.as_obj(),
            "setText",
            "(JLjava/lang/String;)V",
            &[JValue::Long(handle), JValue::Object(&text)],
        );
    }

    fn set_bool(&self, handle: AndroidViewHandle, value: bool) {
        let Some(mut env) = self.env() else {
            return;
        };
        let _ = env.call_method(
            self.host.as_obj(),
            "setBool",
            "(JZ)V",
            &[JValue::Long(handle), JValue::Bool(value as u8)],
        );
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
            let frame = rect.relative_to(parent_rect.unwrap_or_default());
            self.set_frame(handle, frame);
        }

        for child in children_for_node(node) {
            self.apply_layout_relative_to(registry, child, Some(rect));
        }
    }

    fn set_frame(&self, handle: AndroidViewHandle, frame: Rect) {
        let Some(mut env) = self.env() else {
            return;
        };
        let _ = env.call_method(
            self.host.as_obj(),
            "setFrame",
            "(JFFFF)V",
            &[
                JValue::Long(handle),
                JValue::Float(frame.x),
                JValue::Float(frame.y),
                JValue::Float(frame.width),
                JValue::Float(frame.height),
            ],
        );
    }

    fn apply_style(&self, registry: &UiRegistry, node_id: NodeId, handle: AndroidViewHandle) {
        let Some(style) = registry.get_node_style(node_id) else {
            return;
        };
        let style = NativeStyle::from_node_style(style);
        let Some(mut env) = self.env() else {
            return;
        };
        let _ = env.call_method(
            self.host.as_obj(),
            "setStyle",
            "(JIIIII)V",
            &[
                JValue::Long(handle),
                JValue::Int(style.flags as i32),
                JValue::Int(style.bg_rgba as i32),
                JValue::Int(style.border_rgba as i32),
                JValue::Int(style.text_rgba as i32),
                JValue::Int(style.border_width as i32),
            ],
        );
    }
}

impl Renderer for AndroidRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        if let Some(root) = registry.root() {
            self.mount_root(registry, root);
            self.sync_subtree(registry, root);
            self.layout = compute_layout(registry, root, self.viewport_width, self.viewport_height);
            self.apply_layout(registry, root);
        }
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
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

        if let Some(root) = registry.root() {
            self.mount_root(registry, root);
            self.sync_subtree(registry, root);
            self.layout = compute_layout(registry, root, self.viewport_width, self.viewport_height);
            self.apply_layout(registry, root);
        }
    }

    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId) {
        self.create_subtree(registry, node_id);
    }

    fn node_removed(&mut self, node_id: NodeId) {
        if let Some(handle) = self.handle(node_id) {
            let Some(mut env) = self.env() else {
                return;
            };
            let _ = env.call_method(
                self.host.as_obj(),
                "removeView",
                "(J)V",
                &[JValue::Long(handle)],
            );
        }
        if let Some(entry) = self.handles.get_mut(node_id.index()) {
            *entry = None;
        }
    }

    fn target(&self) -> UiTarget {
        UiTarget::Android
    }
}

fn kind_for_node(node: &UiNode) -> AndroidViewKind {
    match node {
        UiNode::Text { .. } => AndroidViewKind::Text,
        UiNode::Button { .. } => AndroidViewKind::Button,
        UiNode::Input { .. } => AndroidViewKind::Input,
        UiNode::Checkbox { .. } => AndroidViewKind::Checkbox,
        UiNode::Image { .. } => AndroidViewKind::Image,
        UiNode::Row { .. } => AndroidViewKind::Row,
        UiNode::ScrollView { .. } | UiNode::ScrollBox { .. } => AndroidViewKind::ScrollView,
        UiNode::View { .. } => AndroidViewKind::View,
        _ => AndroidViewKind::Column,
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

unsafe impl Send for AndroidRenderer {}
