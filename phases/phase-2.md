# Fase 2: UI Core + TUI Renderer

**Status:** Parcialmente Implementado
**Duration:** 2-3 semanas
**Dependencies:** Fase 1, Phase 0

## Agent Context

### Current Implementation State

The following components are **already implemented** in `rover-ui/src/`:

| Component | Location | Status |
|-----------|----------|--------|
| NodeArena | `node/arena.rs` | Complete - Text, Column, Row, Conditional, Each nodes |
| Node Types | `node/types.rs` | Complete - NodeId, TextContent, ContainerNode, ConditionalNode, EachNode |
| RenderCommands | `node/commands.rs` | Complete - UpdateText, Show, Hide, InsertChild, RemoveChild, MountTree, ReplaceEach |
| TuiRenderer | `renderer/tui.rs` | Complete - ratatui-based, handles commands, renders tree |
| LayoutEngine | `layout/engine.rs` | Basic - divide-equally for Column/Row |
| Lua UI | `lua/ui.rs` | Complete - ui.text, ui.column, ui.row, ui.when, ui.each |
| Signal→Node | `signal/runtime.rs` | Complete - subscribe_node(), schedule_node_update() |

### What Needs to Be Completed

1. **Tests & Verification (2.7)** - Sandbox tests, granular update verification
2. **Input Handling** - Keyboard/mouse event routing to nodes
3. **ui.button component** - Pressable element with on_press callback

## Objetivo

Implementar componentes basicos com renderer TUI para validar arquitetura signal → comando → mutacao.

## Entregas

### 2.1 Node System (COMPLETE)

```rust
// rover-ui/src/node/types.rs
pub struct NodeId(pub(crate) u32);

pub enum Node {
    Text(TextNode),
    Column(ContainerNode),
    Row(ContainerNode),
    Conditional(ConditionalNode),
    Each(EachNode),
}

pub enum TextContent {
    Static(SmartString<LazyCompact>),
    Signal(SignalId),
    Derived(DerivedId),
}
```

```rust
// rover-ui/src/node/arena.rs
pub struct NodeArena {
    nodes: Vec<Option<Node>>,
    parents: Vec<Option<NodeId>>,
    keys: Vec<Option<SmartString<LazyCompact>>>,
    free_list: Vec<u32>,
}

impl NodeArena {
    pub fn create(&mut self, node: Node) -> NodeId;
    pub fn get(&self, id: NodeId) -> Option<&Node>;
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node>;
    pub fn set_parent(&mut self, node: NodeId, parent: Option<NodeId>);
    pub fn children(&self, id: NodeId) -> Vec<NodeId>;
    pub fn dispose(&mut self, id: NodeId);
}
```

### 2.2 Render Commands (COMPLETE)

```rust
// rover-ui/src/node/commands.rs
pub enum RenderCommand {
    UpdateText { node: NodeId, value: String },
    Show { node: NodeId },
    Hide { node: NodeId },
    InsertChild { parent: NodeId, index: usize, child: NodeId },
    RemoveChild { parent: NodeId, index: usize },
    MountTree { root: NodeId },
    ReplaceEach { node: NodeId, children: Vec<NodeId> },
}
```

### 2.3 Signal → Node Binding (COMPLETE)

```rust
// rover-ui/src/signal/runtime.rs
impl SignalRuntime {
    pub fn subscribe_node(&self, source: SignalOrDerived, node: NodeId) {
        let subscriber = SubscriberId::Node(node);
        match source {
            SignalOrDerived::Signal(signal_id) => {
                self.graph.borrow_mut().subscribe(signal_id, subscriber);
            }
            SignalOrDerived::Derived(_) => {}
        }
        self.node_bindings.borrow_mut().push((source, node));
    }

    pub fn schedule_node_update(&self, node: NodeId) {
        let mut batch = self.batch.borrow_mut();
        if !batch.pending_node_updates.contains(&node) {
            batch.pending_node_updates.push(node);
        }
    }

    pub fn push_render_command(&self, command: RenderCommand) {
        self.batch.borrow_mut().render_commands.push(command);
    }

    pub fn take_render_commands(&self) -> Vec<RenderCommand> {
        std::mem::take(&mut self.batch.borrow_mut().render_commands)
    }
}
```

### 2.4 Componentes Lua Basicos (COMPLETE)

```lua
-- ui.text
ui.text { "static" }
ui.text { count }  -- signal
ui.text { "Count: " .. count }  -- concat com signal (via __concat)

-- ui.column
ui.column {
    ui.text { "First" },
    ui.text { "Second" },
}

-- ui.row
ui.row {
    ui.text { "Left" },
    ui.text { "Right" },
}

-- ui.when (conditional rendering)
ui.when(condition, ui.text { "Visible!" })
ui.when(condition,
    ui.text { "True" },
    ui.text { "False" }
)

-- ui.each (list rendering)
ui.each(items, function(item, index)
    return ui.text { key = item.id, item.name }
end)
```

Implementation in `rover-ui/src/lua/ui.rs`:

```rust
pub fn register_ui_functions(lua: &Lua, ui_table: &Table) -> Result<()> {
    ui_table.set("text", create_text_fn(lua)?)?;
    ui_table.set("column", create_column_fn(lua)?)?;
    ui_table.set("row", create_row_fn(lua)?)?;
    ui_table.set("when", create_when_fn(lua)?)?;
    ui_table.set("each", create_each_fn(lua)?)?;
    Ok(())
}
```

### 2.5 TUI Renderer (COMPLETE)

```rust
// rover-ui/src/renderer/tui.rs
pub struct TuiRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    root: NodeId,
    runtime: SharedSignalRuntime,
    layout: LayoutEngine,
    visible_nodes: HashMap<NodeId, bool>,
    node_text: HashMap<NodeId, String>,
}

impl Renderer for TuiRenderer {
    fn apply(&mut self, cmd: &RenderCommand, _arena: &NodeArena, _layout: &LayoutEngine) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                self.node_text.insert(*node, value.clone());
            }
            RenderCommand::Show { node } => {
                self.visible_nodes.insert(*node, true);
            }
            RenderCommand::Hide { node } => {
                self.visible_nodes.insert(*node, false);
            }
            // ...
        }
    }

    fn render_frame(&mut self, root: NodeId, arena: &NodeArena, layout: &LayoutEngine, runtime: &SharedSignalRuntime) -> io::Result<()>;
}
```

### 2.6 Layout Basico (Column/Row) (COMPLETE)

```rust
// rover-ui/src/layout/engine.rs
pub struct LayoutEngine {
    computed: HashMap<NodeId, ComputedLayout>,
}

impl LayoutEngine {
    pub fn compute(&mut self, root: NodeId, arena: &NodeArena, available: Size) {
        self.computed.clear();
        self.compute_node(root, arena, Rect::new(0, 0, available.width, available.height));
    }

    fn compute_column(&mut self, node: NodeId, arena: &NodeArena, rect: &Rect) {
        let children = arena.children(node);
        let child_height = rect.height / children.len() as u16;
        // ... divide vertical space equally
    }

    fn compute_row(&mut self, node: NodeId, arena: &NodeArena, rect: &Rect) {
        let children = arena.children(node);
        let child_width = rect.width / children.len() as u16;
        // ... divide horizontal space equally
    }
}
```

### 2.7 Verify & Test (PENDING)

#### Sandbox Test Infrastructure

Create a mock terminal backend for testing without a real terminal:

```rust
// rover-ui/src/renderer/test_utils.rs
pub struct MockTerminal {
    buffer: Vec<Vec<char>>,
    width: u16,
    height: u16,
}

impl MockTerminal {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            buffer: vec![vec![' '; width as usize]; height as usize],
            width,
            height,
        }
    }

    pub fn get_text_at(&self, x: u16, y: u16, len: usize) -> String {
        self.buffer[y as usize][x as usize..x as usize + len].iter().collect()
    }
}

pub struct TestRenderer {
    commands_received: Vec<RenderCommand>,
    mock_terminal: MockTerminal,
}

impl TestRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            commands_received: Vec::new(),
            mock_terminal: MockTerminal::new(width, height),
        }
    }

    pub fn take_commands(&mut self) -> Vec<RenderCommand> {
        std::mem::take(&mut self.commands_received)
    }
}

impl Renderer for TestRenderer {
    fn apply(&mut self, cmd: &RenderCommand, _arena: &NodeArena, _layout: &LayoutEngine) {
        self.commands_received.push(cmd.clone());
    }
}
```

#### Granular Update Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_granular_update_only_affected_nodes() {
        let lua = Lua::new();
        let runtime = SignalRuntime::new();

        // Create signals
        let count = runtime.create_signal(SignalValue::Int(0));
        let static_text = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Static("Static".into())))
        };
        let dynamic_text = {
            let mut arena = runtime.node_arena.borrow_mut();
            let node = arena.create(Node::text(TextContent::Signal(count)));
            node
        };

        runtime.subscribe_node(SignalOrDerived::Signal(count), dynamic_text);

        // Clear any initial commands
        runtime.take_render_commands();

        // Change signal
        runtime.set_signal(count, SignalValue::Int(42));

        // Should only have update for dynamic_text, not static_text
        let commands = runtime.take_render_commands();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            RenderCommand::UpdateText { node, value } => {
                assert_eq!(*node, dynamic_text);
                assert_eq!(value, "42");
            }
            _ => panic!("Expected UpdateText command"),
        }
    }

    #[test]
    fn test_conditional_visibility() {
        let lua = Lua::new();
        let runtime = SignalRuntime::new();

        let show = runtime.create_signal(SignalValue::Bool(true));

        let child_node = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::text(TextContent::Static("Hello".into())))
        };

        let cond_node = {
            let mut arena = runtime.node_arena.borrow_mut();
            let node = arena.create(Node::conditional(show));
            if let Some(Node::Conditional(cond)) = arena.get_mut(node) {
                cond.true_branch = Some(child_node);
            }
            node
        };

        runtime.subscribe_node(SignalOrDerived::Signal(show), cond_node);
        runtime.take_render_commands();

        // Toggle visibility
        runtime.set_signal(show, SignalValue::Bool(false));

        let commands = runtime.take_render_commands();
        assert!(commands.iter().any(|c| matches!(c, RenderCommand::Hide { .. })));
    }
}
```

#### Manual Validation Steps

1. Run TUI app: `cargo run -p rover_cli -- examples/counter.lua -p tui`
2. Increment counter, verify ONLY counter text updates (no screen flicker)
3. Toggle visibility with ui.when, verify element appears/disappears smoothly
4. Verify q/Esc exits cleanly

## Estrutura de Arquivos

```
rover-ui/
├── src/
│   ├── signal/
│   │   ├── mod.rs
│   │   ├── arena.rs       # SignalArena
│   │   ├── graph.rs       # SubscriberGraph
│   │   ├── value.rs       # SignalValue enum
│   │   ├── derived.rs     # DerivedSignal
│   │   ├── effect.rs      # Effect
│   │   └── runtime.rs     # SignalRuntime (coordinator)
│   ├── node/
│   │   ├── mod.rs
│   │   ├── arena.rs       # NodeArena
│   │   ├── types.rs       # Node enum, TextNode, etc
│   │   ├── binding.rs     # SignalOrDerived
│   │   └── commands.rs    # RenderCommand
│   ├── layout/
│   │   ├── mod.rs
│   │   └── engine.rs      # LayoutEngine
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── traits.rs      # Renderer trait
│   │   ├── tui.rs         # TUI implementation
│   │   └── test_utils.rs  # MockTerminal, TestRenderer (NEW)
│   ├── platform/
│   │   ├── mod.rs
│   │   └── tui.rs         # TuiPlatform, PlatformHandler trait
│   └── lua/
│       ├── mod.rs
│       ├── signal.rs      # LuaSignal userdata
│       ├── derived.rs     # LuaDerived userdata
│       ├── effect.rs      # effect() function
│       ├── ui.rs          # ui.text, ui.column, etc
│       ├── node.rs        # LuaNode userdata
│       ├── helpers.rs     # get_runtime helper
│       ├── utils.rs
│       └── metamethods.rs # __add, __concat, etc
```

## Test Commands

```bash
# Build and check for errors
cargo build -p rover_ui

# Run tests
cargo test -p rover_ui

# Run example (if rover_cli exists)
cargo run -p rover_cli -- examples/counter.lua -p tui
```

## Validation Checklist

- [ ] `cargo build -p rover_ui` compiles without errors
- [ ] `cargo test -p rover_ui` all tests pass
- [ ] Granular update test: only affected nodes receive RenderCommands
- [ ] Conditional test: ui.when correctly shows/hides based on signal
- [ ] Manual TUI test: counter app works, no screen flicker
- [ ] Layout test: Column/Row divide space correctly
