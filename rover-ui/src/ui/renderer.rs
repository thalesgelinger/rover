use super::node::NodeId;
use super::registry::UiRegistry;

/// Platform-specific renderer trait
///
/// Implementations handle rendering UI nodes to their specific platform
/// (terminal, web, iOS, Android, etc.)
pub trait Renderer: 'static {
    /// Called once when the UI tree is first mounted
    fn mount(&mut self, registry: &UiRegistry);

    /// Called when nodes have been updated - only dirty nodes need to be re-rendered
    /// This should mutate existing platform views, not recreate them
    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]);

    /// Called when a new node is added to the tree
    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId);

    /// Called when a node is removed from the tree
    fn node_removed(&mut self, node_id: NodeId);
}
