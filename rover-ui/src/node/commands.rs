use super::types::NodeId;

#[derive(Debug, Clone)]
pub enum RenderCommand {
    UpdateText {
        node: NodeId,
        value: String,
    },
    Show {
        node: NodeId,
    },
    Hide {
        node: NodeId,
    },
    InsertChild {
        parent: NodeId,
        index: usize,
        child: NodeId,
    },
    RemoveChild {
        parent: NodeId,
        index: usize,
    },
    MountTree {
        root: NodeId,
    },
    ReplaceEach {
        node: NodeId,
        children: Vec<NodeId>,
    },
}
