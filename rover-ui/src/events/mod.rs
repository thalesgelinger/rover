mod queue;

pub use queue::EventQueue;

use crate::ui::node::NodeId;

/// UI events that can be dispatched to nodes
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// Button click event
    Click { node_id: NodeId },
    /// Input value change event (fired on every keystroke)
    Change { node_id: NodeId, value: String },
    /// Input submit event (fired on Enter)
    Submit { node_id: NodeId, value: String },
    /// Checkbox toggle event
    Toggle { node_id: NodeId, checked: bool },
    /// Generic key event for focusable TUI nodes
    Key { node_id: NodeId, key: String },
}

impl UiEvent {
    /// Get the target node ID for this event
    pub fn node_id(&self) -> NodeId {
        match self {
            UiEvent::Click { node_id } => *node_id,
            UiEvent::Change { node_id, .. } => *node_id,
            UiEvent::Submit { node_id, .. } => *node_id,
            UiEvent::Toggle { node_id, .. } => *node_id,
            UiEvent::Key { node_id, .. } => *node_id,
        }
    }
}
