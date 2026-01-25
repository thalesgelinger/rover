use super::UiEvent;

/// FIFO queue for UI events
#[derive(Debug)]
pub struct EventQueue {
    events: Vec<UiEvent>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Push an event to the back of the queue
    pub fn push(&mut self, event: UiEvent) {
        self.events.push(event);
    }

    /// Drain all events from the queue in FIFO order
    pub fn drain(&mut self) -> impl Iterator<Item = UiEvent> + '_ {
        self.events.drain(..)
    }

    /// Get the number of pending events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::node::NodeId;

    #[test]
    fn test_event_queue_fifo() {
        let mut queue = EventQueue::new();

        queue.push(UiEvent::Click {
            node_id: NodeId(1),
        });
        queue.push(UiEvent::Click {
            node_id: NodeId(2),
        });
        queue.push(UiEvent::Click {
            node_id: NodeId(3),
        });

        assert_eq!(queue.len(), 3);

        let events: Vec<_> = queue.drain().collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].node_id(), NodeId(1));
        assert_eq!(events[1].node_id(), NodeId(2));
        assert_eq!(events[2].node_id(), NodeId(3));

        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_queue_clear() {
        let mut queue = EventQueue::new();

        queue.push(UiEvent::Click {
            node_id: NodeId(1),
        });
        queue.push(UiEvent::Click {
            node_id: NodeId(2),
        });

        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_types() {
        let click = UiEvent::Click {
            node_id: NodeId(1),
        };
        assert_eq!(click.node_id(), NodeId(1));

        let change = UiEvent::Change {
            node_id: NodeId(2),
            value: "test".to_string(),
        };
        assert_eq!(change.node_id(), NodeId(2));

        let toggle = UiEvent::Toggle {
            node_id: NodeId(3),
            checked: true,
        };
        assert_eq!(toggle.node_id(), NodeId(3));
    }
}
