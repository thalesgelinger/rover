use super::arena::SignalId;
use smallvec::SmallVec;

/// Unique identifier for a derived signal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DerivedId(pub(crate) u32);

/// Unique identifier for an effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectId(pub u32);

/// Identifies a subscriber to signal changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriberId {
    Derived(DerivedId),
    Effect(EffectId),
    // Node(NodeId), // Phase 2
}

/// Tracks dependencies between signals and their subscribers
pub struct SubscriberGraph {
    /// For each signal: who depends on it
    /// Index by SignalId.0
    subscribers: Vec<SmallVec<[SubscriberId; 8]>>,
}

impl SubscriberGraph {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Ensure capacity for a given number of signals
    pub fn ensure_capacity(&mut self, signal_count: usize) {
        if self.subscribers.len() < signal_count {
            self.subscribers.resize_with(signal_count, SmallVec::new);
        }
    }

    /// Subscribe a subscriber to a signal
    pub fn subscribe(&mut self, signal: SignalId, subscriber: SubscriberId) {
        let idx = signal.0 as usize;
        self.ensure_capacity(idx + 1);

        // Only add if not already subscribed
        if !self.subscribers[idx].contains(&subscriber) {
            self.subscribers[idx].push(subscriber);
        }
    }

    /// Unsubscribe a subscriber from a signal
    pub fn unsubscribe(&mut self, signal: SignalId, subscriber: SubscriberId) {
        let idx = signal.0 as usize;
        if idx < self.subscribers.len() {
            self.subscribers[idx].retain(|s| *s != subscriber);
        }
    }

    /// Get all subscribers for a signal
    pub fn get_subscribers(&self, signal: SignalId) -> &[SubscriberId] {
        let idx = signal.0 as usize;
        if idx < self.subscribers.len() {
            &self.subscribers[idx]
        } else {
            &[]
        }
    }

    /// Clear all subscriptions for a given subscriber
    pub fn clear_for(&mut self, subscriber: SubscriberId) {
        for subs in &mut self.subscribers {
            subs.retain(|s| *s != subscriber);
        }
    }
}

impl Default for SubscriberGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe() {
        let mut graph = SubscriberGraph::new();
        let signal = SignalId(0);
        let derived = SubscriberId::Derived(DerivedId(0));

        graph.subscribe(signal, derived);

        let subs = graph.get_subscribers(signal);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], derived);
    }

    #[test]
    fn test_subscribe_multiple() {
        let mut graph = SubscriberGraph::new();
        let signal = SignalId(0);
        let derived1 = SubscriberId::Derived(DerivedId(0));
        let derived2 = SubscriberId::Derived(DerivedId(1));
        let effect = SubscriberId::Effect(EffectId(0));

        graph.subscribe(signal, derived1);
        graph.subscribe(signal, derived2);
        graph.subscribe(signal, effect);

        let subs = graph.get_subscribers(signal);
        assert_eq!(subs.len(), 3);
    }

    #[test]
    fn test_subscribe_idempotent() {
        let mut graph = SubscriberGraph::new();
        let signal = SignalId(0);
        let derived = SubscriberId::Derived(DerivedId(0));

        graph.subscribe(signal, derived);
        graph.subscribe(signal, derived);
        graph.subscribe(signal, derived);

        let subs = graph.get_subscribers(signal);
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn test_unsubscribe() {
        let mut graph = SubscriberGraph::new();
        let signal = SignalId(0);
        let derived1 = SubscriberId::Derived(DerivedId(0));
        let derived2 = SubscriberId::Derived(DerivedId(1));

        graph.subscribe(signal, derived1);
        graph.subscribe(signal, derived2);

        graph.unsubscribe(signal, derived1);

        let subs = graph.get_subscribers(signal);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], derived2);
    }

    #[test]
    fn test_clear_for() {
        let mut graph = SubscriberGraph::new();
        let signal1 = SignalId(0);
        let signal2 = SignalId(1);
        let derived = SubscriberId::Derived(DerivedId(0));
        let effect = SubscriberId::Effect(EffectId(0));

        graph.subscribe(signal1, derived);
        graph.subscribe(signal1, effect);
        graph.subscribe(signal2, derived);
        graph.subscribe(signal2, effect);

        graph.clear_for(derived);

        // derived should be removed from all signals
        assert_eq!(graph.get_subscribers(signal1).len(), 1);
        assert_eq!(graph.get_subscribers(signal1)[0], effect);
        assert_eq!(graph.get_subscribers(signal2).len(), 1);
        assert_eq!(graph.get_subscribers(signal2)[0], effect);
    }

    #[test]
    fn test_get_nonexistent_signal() {
        let graph = SubscriberGraph::new();
        let signal = SignalId(999);

        let subs = graph.get_subscribers(signal);
        assert_eq!(subs.len(), 0);
    }
}
