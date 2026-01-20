use crate::node::NodeId;
use crate::signal::{DerivedId, SignalId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeBinding {
    TextContent,
    Visibility,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalOrDerived {
    Signal(SignalId),
    Derived(DerivedId),
}

pub struct NodeBindings {
    bindings: HashMap<(SignalOrDerived, NodeId), NodeBinding>,
}

impl NodeBindings {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn bind(&mut self, source: SignalOrDerived, node: NodeId, binding: NodeBinding) {
        self.bindings.insert((source, node), binding);
    }

    pub fn unbind(&mut self, source: SignalOrDerived, node: NodeId) {
        self.bindings.remove(&(source, node));
    }

    pub fn get_binding(&self, source: SignalOrDerived, node: NodeId) -> Option<NodeBinding> {
        self.bindings.get(&(source, node)).copied()
    }

    pub fn unbind_node(&mut self, node: NodeId) {
        self.bindings.retain(|(_, n), _| *n != node);
    }

    pub fn unbind_source(&mut self, source: SignalOrDerived) {
        self.bindings.retain(|(s, _), _| *s != source);
    }
}

impl Default for NodeBindings {
    fn default() -> Self {
        Self::new()
    }
}
