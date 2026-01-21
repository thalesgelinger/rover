use rover_ui::layout::LayoutEngine;
use rover_ui::node::{Node, NodeArena, NodeId, RenderCommand};
use rover_ui::renderer::Renderer;
use rover_ui::SharedSignalRuntime;
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Text};

pub struct WebRenderer {
    container: Element,
    runtime: SharedSignalRuntime,
    document: Document,
    /// Map NodeId -> DOM element
    elements: HashMap<NodeId, Element>,
    /// Map NodeId -> Text node (for text content)
    text_nodes: HashMap<NodeId, Text>,
}

impl WebRenderer {
    pub fn new(container: Element, runtime: SharedSignalRuntime) -> Self {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        Self {
            container,
            runtime,
            document,
            elements: HashMap::new(),
            text_nodes: HashMap::new(),
        }
    }

    fn get_or_create_element(&mut self, node_id: NodeId, arena: &NodeArena) -> Element {
        if let Some(el) = self.elements.get(&node_id) {
            return el.clone();
        }

        let node = arena.get(node_id).expect("node not found");
        let element = match node {
            Node::Text(_) => {
                let span = self.document.create_element("span").unwrap();
                let text = self.document.create_text_node("");
                span.append_child(&text).unwrap();
                self.text_nodes.insert(node_id, text);
                span
            }
            Node::Column(_) => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: column;")
                    .unwrap();
                div
            }
            Node::Row(_) => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: row;")
                    .unwrap();
                div
            }
            Node::Conditional(_) | Node::Each(_) => {
                // Wrapper div for conditional/list content
                self.document.create_element("div").unwrap()
            }
        };

        self.elements.insert(node_id, element.clone());
        element
    }

    pub fn mount_tree(&mut self, root: NodeId, arena: &NodeArena) {
        // Clear container
        self.container.set_inner_html("");

        // Recursively mount nodes
        let root_element = self.mount_node(root, arena);
        self.container.append_child(&root_element).unwrap();
    }

    fn mount_node(&mut self, node_id: NodeId, arena: &NodeArena) -> Element {
        let element = self.get_or_create_element(node_id, arena);

        // Mount children
        let children = arena.children(node_id);
        for child_id in children {
            let child_element = self.mount_node(child_id, arena);
            element.append_child(&child_element).unwrap();
        }

        element
    }
}

impl Renderer for WebRenderer {
    fn apply(&mut self, cmd: &RenderCommand, arena: &NodeArena, _layout: &LayoutEngine) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                if let Some(text_node) = self.text_nodes.get(node) {
                    text_node.set_data(value);
                }
            }
            RenderCommand::Show { node } => {
                if let Some(element) = self.elements.get(node) {
                    let style = element
                        .get_attribute("style")
                        .unwrap_or_default()
                        .replace("display: none;", "");
                    element
                        .set_attribute("style", &format!("{} display: block;", style))
                        .unwrap();
                }
            }
            RenderCommand::Hide { node } => {
                if let Some(element) = self.elements.get(node) {
                    let style = element.get_attribute("style").unwrap_or_default();
                    element
                        .set_attribute("style", &format!("{} display: none;", style))
                        .unwrap();
                }
            }
            RenderCommand::InsertChild {
                parent,
                index,
                child,
            } => {
                let parent_el = self.get_or_create_element(*parent, arena);
                let child_el = self.get_or_create_element(*child, arena);

                let children = parent_el.children();
                if *index >= children.length() as usize {
                    parent_el.append_child(&child_el).unwrap();
                } else {
                    if let Some(ref_node) = children.item(*index as u32) {
                        parent_el
                            .insert_before(&child_el, Some(ref_node.as_ref()))
                            .unwrap();
                    } else {
                        parent_el.append_child(&child_el).unwrap();
                    }
                }
            }
            RenderCommand::RemoveChild { parent, index } => {
                if let Some(parent_el) = self.elements.get(parent) {
                    let children = parent_el.children();
                    if let Some(child) = children.item(*index as u32) {
                        parent_el.remove_child(&child).unwrap();
                    }
                }
            }
            RenderCommand::MountTree { root } => {
                self.mount_tree(*root, arena);
            }
            RenderCommand::ReplaceEach { node, children } => {
                if let Some(element) = self.elements.get(node).cloned() {
                    // Clear existing children
                    element.set_inner_html("");
                    // Add new children
                    for child_id in children {
                        let child_el = self.mount_node(*child_id, arena);
                        element.append_child(&child_el).unwrap();
                    }
                }
            }
        }
    }

    fn render_frame(
        &mut self,
        _root: NodeId,
        _arena: &NodeArena,
        _layout: &LayoutEngine,
        _runtime: &SharedSignalRuntime,
    ) -> std::io::Result<()> {
        // DOM updates are immediate, no frame rendering needed
        Ok(())
    }
}
