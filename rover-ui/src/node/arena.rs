use super::types::{Node, NodeId};
use smartstring::{LazyCompact, SmartString};

pub struct NodeArena {
    nodes: Vec<Option<Node>>,
    parents: Vec<Option<NodeId>>,
    keys: Vec<Option<SmartString<LazyCompact>>>,
    free_list: Vec<u32>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            parents: Vec::new(),
            keys: Vec::new(),
            free_list: Vec::new(),
        }
    }

    pub fn create(&mut self, node: Node) -> NodeId {
        let index = if let Some(idx) = self.free_list.pop() {
            idx as usize
        } else {
            self.nodes.len()
        };

        if index >= self.nodes.len() {
            self.nodes.push(Some(node));
            self.parents.push(None);
            self.keys.push(None);
        } else {
            self.nodes[index] = Some(node);
            self.parents[index] = None;
            self.keys[index] = None;
        }

        NodeId(index as u32)
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0 as usize)?.as_mut()
    }

    pub fn set_parent(&mut self, node: NodeId, parent: Option<NodeId>) {
        if let Some(p) = self.parents.get_mut(node.0 as usize) {
            *p = parent;
        }
    }

    pub fn get_parent(&self, node: NodeId) -> Option<NodeId> {
        *self.parents.get(node.0 as usize)?
    }

    pub fn set_key(&mut self, node: NodeId, key: Option<SmartString<LazyCompact>>) {
        if let Some(slot) = self.keys.get_mut(node.0 as usize) {
            *slot = key;
        }
    }

    pub fn key(&self, node: NodeId) -> Option<&SmartString<LazyCompact>> {
        self.keys.get(node.0 as usize)?.as_ref()
    }

    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        match self.get(id) {
            Some(Node::Column(c)) | Some(Node::Row(c)) => c.children.clone(),
            Some(Node::Each(e)) => e.children.iter().map(|(_, child)| *child).collect(),
            _ => Vec::new(),
        }
    }

    pub fn dispose(&mut self, id: NodeId) {
        let idx = id.0 as usize;
        if idx < self.nodes.len() {
            self.nodes[idx] = None;
            self.parents[idx] = None;
            self.keys[idx] = None;
            self.free_list.push(id.0);
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for NodeArena {
    fn default() -> Self {
        Self::new()
    }
}
