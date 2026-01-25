mod queue;

pub use queue::EventQueue;

use crate::ui::node::NodeId;

/// UI events that can be dispatched to nodes
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// Button click event
    Click { node_id: NodeId },
    /// Input value change event
    Change { node_id: NodeId, value: String },
    /// Checkbox toggle event
    Toggle { node_id: NodeId, checked: bool },
}

impl UiEvent {
    /// Get the target node ID for this event
    pub fn node_id(&self) -> NodeId {
        match self {
            UiEvent::Click { node_id } => *node_id,
            UiEvent::Change { node_id, .. } => *node_id,
            UiEvent::Toggle { node_id, .. } => *node_id,
        }
    }
}
