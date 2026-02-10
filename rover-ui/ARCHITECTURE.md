# Rover UI Reactivity System

## Namespace Contract

- `rover.ui`: portable primitives intended to work across TUI/web/mobile.
- `rover.tui`: terminal-only components (`select`, `tab_select`, `scroll_box`, `textarea`).
- `rover.target`: runtime target string (`tui`, `web`, `mobile`, `unknown`).

Guard behavior:

- Calling `rover.tui.*` on non-`tui` target emits warning and throws runtime error.
- Warning sink is stderr by default.
- Optional warning hook: `rover.on_warning(function(msg) ... end)`.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        LUA CODE                                  │
│  local name = rover.signal("Alice")                              │
│  local greeting = rover.derive(function()                        │
│    return "Hello, " .. name:get()                                │
│  end)                                                            │
│  local textNode = rover.ui.text(greeting)  ← Creates UI node     │
└───────────────┬─────────────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────────────┐
│                    UI REGISTRY                                   │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  effect_to_node: EffectId → NodeId                     │    │
│  │  node_to_effects: NodeId → Vec<EffectId>               │    │
│  │  dirty_nodes: HashSet<NodeId>                          │    │
│  │  nodes: NodeArena (stores UiNode)                      │    │
│  └────────────────────────────────────────────────────────┘    │
└───────────────┬─────────────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────────────────────────────┐
│                  SIGNAL RUNTIME                                  │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  arena: SignalArena (stores signal values)             │    │
│  │  graph: SubscriberGraph (tracks dependencies)          │    │
│  │  effects: Vec<Effect> (effect callbacks)                │    │
│  │  derived: Vec<DerivedSignal> (computed values)         │    │
│  │  batch: pending_effects (batched updates)              │    │
│  └────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Key Design Principle: Stable IDs + In-Place Mutation

**NodeId = Stable Pointer to UI Node**

The system uses arena-based storage where each NodeId is a stable u32 index into the arena. This means:

- A NodeId always points to the same memory location
- Content changes update the node **in-place** without changing the ID
- Renderers can maintain a mapping: `NodeId → PlatformView` (e.g., SDL window handle, SwiftUI view)
- When content changes, the renderer updates the existing platform view instead of recreating it

```rust
// NodeArena structure
pub struct NodeArena {
    nodes: Vec<Option<UiNode>>,  // Stable indices
    free_list: Vec<u32>,          // For ID reuse
}

// NodeId is just an index
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);

// Content updates happen in-place
pub fn update(&mut self, new_value: String) {
    *current_value = new_value;  // Mutates in place, ID stays same
}
```

## Reactive Text Node Creation (rover.ui.text(signal))

**Step-by-step flow:**

```
rover.ui.text(signal)
    │
    ├─► 1. RESERVE NODE ID (in UiRegistry)
    │       node_id = registry.reserve_node_id()
    │       - Allocates a stable NodeId
    │       - NodeId doesn't change during lifetime
    │
    ├─► 2. CREATE EFFECT CALLBACK
    │       callback = |lua| {
    │         value = runtime.get_signal(signal_id)
    │         registry.update_text_content(node_id, value)
    │       }
    │       - Callback closure captures node_id
    │       - Will be called whenever signal changes
    │
    ├─► 3. CREATE EFFECT (in SignalRuntime)
    │       effect_id = runtime.create_effect(callback)
    │       │
    │       ├─► Runs callback immediately
    │       │     - Reads signal value (tracks dependency!)
    │       │     - Updates node with initial value (IN-PLACE)
    │       │     - Marks node as dirty
    │       │
    │       └─► Registers subscription in graph
    │             graph.subscribe(signal_id, EffectId(effect_id))
    │             - Effect now subscribed to signal changes
    │
    ├─► 4. FINALIZE NODE
    │       node = UiNode::Text {
    │         content: Reactive {
    │           current_value: "initial",
    │           effect_id: effect_id
    │         }
    │       }
    │       registry.finalize_node(node_id, node)
    │       - Places node at the reserved index
    │
    └─► 5. ATTACH EFFECT TO NODE
        registry.attach_effect(node_id, effect_id)
        └─► Stores bidirectional mapping:
            effect_to_node[effect_id] = node_id
            node_to_effects[node_id].push(effect_id)
```

## When Signal Value Changes

```
signal:set("new_value")
    │
    ├─► 1. UPDATE SIGNAL VALUE
    │       arena.set(signal_id, "new_value")
    │       - Mutates signal in-place
    │       - Bumps version number (for change detection)
    │       - Returns true if value actually changed
    │
    ├─► 2. NOTIFY SUBSCRIBERS
    │       notify_subscribers(signal_id)
    │       │
    │       └─► Look up subscribers in graph:
    │             subscribers = graph.get_subscribers(signal_id)
    │             - Returns Vec<SubscriberId>
    │             - Could be DerivedSignal or Effect
    │
    └─► 3. FOR EACH SUBSCRIBER:
            │
            ├─► IF DerivedSignal:
            │     mark_derived_dirty(derived_id)
            │     - Mark derived as dirty
            │     - Propagate to its subscribers recursively
            │
            └─► IF Effect:
                  schedule_effect(effect_id)
                  - Add to batch.pending_effects
                  - Effect will run when batch ends
```

## How Dirty Marking Works

**Two Levels of Dirty Tracking:**

### 1. Signal Level (for derived signals)
```rust
pub struct DerivedSignal {
    compute_fn: RegistryKey,
    cached_value: SignalValue,
    dependencies: SmallVec<[SignalId; 4]>,
    dirty: bool,  // ← Dirty flag for derived signals
}

fn mark_derived_dirty(&self, id: DerivedId) {
    if !derived[id].is_dirty() {
        derived[id].mark_dirty();  // Sets dirty = true
        // Propagate to subscribers
        for subscriber in graph.get_subscribers(id) {
            match subscriber {
                DerivedId => mark_derived_dirty(child_id),
                EffectId => schedule_effect(effect_id),
            }
        }
    }
}
```

### 2. UI Node Level (for rendering)
```rust
pub struct UiRegistry {
    dirty_nodes: HashSet<NodeId>,  // ← Dirty flag set for UI nodes
    // ...
}

fn mark_dirty(&mut self, node_id: NodeId) {
    self.dirty_nodes.insert(node_id);
}

fn update_text_content(&mut self, node_id: NodeId, new_value: String) {
    if let Some(UiNode::Text { content }) = self.nodes.get_mut(node_id) {
        content.update(new_value);        // In-place update
        self.mark_dirty(node_id);         // Mark for rendering
    }
}
```

**Flow: Signal Change → Effect Callback → Mark Node Dirty**

```
signal:set("new")
    ↓
notify_subscribers(signal_id)
    ↓
schedule_effect(effect_id)
    ↓
[batch ends]
    ↓
run_effect(effect_id)
    ↓
callback() {
    value = get_signal(signal_id)      // Read new value
    registry.update_text_content(node_id, value)
}
    ↓
update_text_content() {
    node.content.update(new_value)     // In-place mutation
    mark_dirty(node_id)                // ← DIRTY MARKING HERE
    dirty_nodes.insert(node_id)
}
    ↓
[renderer takes dirty nodes]
    ↓
renderer.update(registry, [node_id])  // Render the dirty nodes
```

## How Rendering Happens

### Renderer Trait Interface

```rust
pub trait Renderer: 'static {
    /// Called once when UI tree is first mounted
    fn mount(&mut self, registry: &UiRegistry);

    /// Called when nodes updated - MUTATE EXISTING VIEWS
    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]);

    /// Called when new node added
    fn node_added(&mut self, registry: &UiRegistry, node_id: NodeId);

    /// Called when node removed
    fn node_removed(&mut self, node_id: NodeId);
}
```

### Rendering Flow

**1. Initial Mount (when UI first created)**

```
renderer.mount(registry)
    │
    └─► Traverse tree from root
        ├─► For each node:
        │     1. Create platform view (e.g., SDL widget, SwiftUI view)
        │     2. Store mapping: NodeId → PlatformView
        │     3. Set initial content
        │
        └─► Platform view is created once
```

**2. Incremental Updates (when signals change)**

```
// When batch ends and effects have updated nodes
let dirty_nodes = registry.take_dirty_nodes();  // Consumes dirty set

renderer.update(registry, dirty_nodes)
    │
    ├─► For each dirty node_id:
    │     │
    │     ├─► Get node content: registry.get_node(node_id)
    │     │     - Returns &UiNode
    │     │     - Contains updated content (was mutated in-place)
    │     │
    │     ├─► Look up platform view: platform_views[node_id]
    │     │     - Renderer maintains this mapping
    │     │     - Same view as created during mount
    │     │
    │     └─► Update platform view IN-PLACE
    │           if node is Text:
    │               platform_view.set_text(node.content.value())
    │           if node is Column:
    │               platform_view.update_layout(children)
    │           // etc.
    │
    └─► Platform views are MUTATED, not recreated
```

### In-Place Rendering Example (SDL)

```rust
struct SdlRenderer {
    windows: HashMap<NodeId, SdlWindow>,  // NodeId → Window
}

impl Renderer for SdlRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        // Traverse tree and create windows
        for node_id in traverse(registry.root()) {
            let window = SdlWindow::new(/* params */);
            self.windows.insert(node_id, window);  // Store mapping
        }
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        for &node_id in dirty_nodes {
            if let Some(node) = registry.get_node(node_id) {
                if let UiNode::Text { content } = node {
                    // Get existing window and UPDATE IT
                    if let Some(window) = self.windows.get_mut(&node_id) {
                        window.set_text(content.value());  // In-place update!
                    }
                }
            }
        }
    }
}
```

## Critical Flow: signal.set() → effect → mark dirty → render (in-place)

```
1. LUA CALL
   signal:set("new_value")                     signal/runtime.rs:85-89
   │
   ├─► arena.set(signal_id, new_value)
   │     - Mutates signal value in-place
   │     - Bumps version
   │
   └─► notify_subscribers(signal_id)         signal/runtime.rs:309
         │
         └─► Look up subscribers in graph:
               subscribers = graph.get_subscribers(signal_id)
               └─► [Effect(effect_id), ...]

2. SCHEDULE EFFECT
   schedule_effect(effect_id)                signal/runtime.rs:371
   │
   └─► batch.pending_effects.push(effect_id)
         - Adds to pending list
         - Will run when batch ends

3. BATCH ENDS (e.g., after all Lua code finishes)
   end_batch()                               signal/runtime.rs:286
   │
   ├─► Take all pending effects
   │     pending = batch.pending_effects
   │
   └─► Run each effect:
         for effect_id in pending:
             run_effect(effect_id)           signal/runtime.rs:218
             │
             ├─► Get callback
             │     callback = effects[effect_id].callback
             │
             ├─► CALL CALLBACK (with tracking)
             │     callback(lua)
             │     │
             │     ├─► READ SIGNAL
             │     │     value = runtime.get_signal(signal_id)
             │     │     - Records read in tracking.reads
             │     │
             │     └─► UPDATE NODE (IN-PLACE!)
             │           registry.update_text_content(node_id, value)
             │           │
             │           ├─► MUTATE CONTENT
             │           │     node.content.update(new_value)
             │           │     // node.content.current_value = "new_value"
             │           │
             │           └─► MARK DIRTY     ui/registry.rs:63-64
             │                 registry.mark_dirty(node_id)
             │                 └─► dirty_nodes.insert(node_id)
             │
             └─► UPDATE DEPENDENCIES
                   graph.clear_for(EffectId(effect_id))
                   for signal in tracking.reads:
                     graph.subscribe(signal, EffectId(effect_id))

4. RENDER DIRTY NODES (IN-PLACE)
   renderer.update(registry, dirty_nodes)   renderer.rs:14
   │
   ├─► Take dirty nodes
   │     dirty_nodes = registry.take_dirty_nodes()
   │
   └─► For each dirty node:
         if let Some(window) = platform_views.get(node_id) {
             // MUTATE EXISTING WINDOW
             window.set_text(node.content.value())
             // Window stays at same memory location
             // Only content changes
         }
```

## Pointer/View Reuse Mechanism

**Yes, the system is designed for pointer/view reuse!**

### How It Works:

1. **Stable NodeIds**
   - NodeId is a u32 index into NodeArena
   - Never changes for a given node's lifetime
   - Arena reuses freed slots, but only after node is removed

2. **In-Place Content Updates**
   ```rust
   // Signal updates value at same index
   arena.set(signal_id, new_value)  // Mutates, ID stays same

   // Node updates content at same index
   node.content.update(new_value)   // Mutates, ID stays same
   ```

3. **Renderer Mapping**
   ```rust
   struct MyRenderer {
       views: HashMap<NodeId, PlatformView>,
   }

   // Create view once during mount
   views.insert(node_id, create_view())

   // Update existing view during updates
   views.get_mut(node_id)?.update_content(new_value)
   ```

4. **No Recreation Needed**
   - Same NodeId → Same PlatformView
   - Content updates are in-place mutations
   - Renderer just mutates the existing view

### Benefits:

- **Performance**: No view recreation, just content updates
- **Preserved State**: Platform views maintain their state (scroll position, focus, etc.)
- **Predictable Mapping**: NodeId always maps to same view
- **Batched Updates**: Multiple node updates are rendered in one pass

### Example Lifecycle:

```
Create: rover.ui.text(signal)
  → NodeId(5) created
  → Renderer creates SDL window at &window_ptr
  → views[NodeId(5)] = window_ptr

Update: signal:set("new")
  → Node(5) content mutated in-place: "old" → "new"
  → NodeId(5) still same (not recreated)
  → Renderer: views[NodeId(5)].set_text("new")
  → Same window_ptr, just text changed

Update again: signal:set("newer")
  → Node(5) content mutated: "new" → "newer"
  → NodeId(5) still same
  → Renderer: views[NodeId(5)].set_text("newer")
  → Same window_ptr, same memory

Remove: node removed
  → Node(5) removed from arena
  → views.remove(NodeId(5))
  → window_ptr destroyed
  → NodeId(5) can be reused later for new node
```

## Key Data Structures

### UiRegistry (ui/registry.rs:6-15)
```rust
pub struct UiRegistry {
    nodes: NodeArena,                                    // Stable node storage
    root: Option<NodeId>,
    effect_to_node: HashMap<EffectId, NodeId>,          // Effect → Node
    node_to_effects: HashMap<NodeId, Vec<EffectId>>,    // Node → Effects
    dirty_nodes: HashSet<NodeId>,                        // Dirty tracking
}
```

### SubscriberGraph (signal/graph.rs:21-25)
```rust
pub struct SubscriberGraph {
    subscribers: Vec<SmallVec<[SubscriberId; 8]>>,
    // Index by SignalId.0
    // Each signal has list of subscribers (Derived or Effect)
}
```

### NodeArena (ui/node.rs:8-11)
```rust
pub struct NodeArena {
    nodes: Vec<Option<UiNode>>,  // Stable indices
    free_list: Vec<u32>,          // For ID reuse
}

// NodeId is just an index - always stable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);
```

## Summary: The Magic Formula

1. **Stable IDs**: NodeId is a stable u32 index, never changes
2. **In-Place Updates**: Signal and node content are mutated in-place
3. **Effect Bridge**: Effects read signals and update nodes, marking dirty
4. **Dirty Tracking**: Registry tracks which nodes need re-rendering
5. **Renderer Mapping**: Platform views are stored by NodeId, updated in-place
6. **No Recreation**: Same NodeId → Same PlatformView, only content changes

This enables:
- **Efficient updates**: Only dirty nodes are re-rendered
- **View reuse**: Platform views are created once, mutated thereafter
- **State preservation**: Scroll positions, focus, etc. are maintained
- **Predictable mapping**: NodeId always points to same memory location
