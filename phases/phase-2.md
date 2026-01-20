# Fase 2: UI Core + TUI Renderer

**Status:** 🔲 Não Iniciado  
**Duration:** 2-3 semanas  
**Dependencies:** Fase 1

## Objetivo

Implementar componentes básicos com renderer TUI para validar arquitetura signal → comando → mutação.

## Entregas

### 2.1 Node System

```rust
pub struct NodeId(u32);

pub struct NodeArena {
    nodes: Vec<Node>,
    parent: Vec<Option<NodeId>>,
    children: Vec<SmallVec<[NodeId; 8]>>,
}

pub enum Node {
    Text(TextNode),
    Column(ContainerNode),
    Row(ContainerNode),
    Conditional(ConditionalNode),
    Each(EachNode),
}

pub struct TextNode {
    content: TextContent,
}

pub enum TextContent {
    Static(StringId),
    Signal(SignalId),
    Concat(SmallVec<[TextPart; 4]>),
}
```

### 2.2 Render Commands

```rust
pub enum RenderCommand {
    // Texto
    UpdateText { node: NodeId, value: String },
    
    // Visibilidade
    Show { node: NodeId },
    Hide { node: NodeId },
    
    // Hierarquia
    InsertChild { parent: NodeId, index: usize, child: NodeId },
    RemoveChild { parent: NodeId, index: usize },
    MoveChild { parent: NodeId, from: usize, to: usize },
    
    // Layout (pra futuro)
    UpdateLayout { node: NodeId, layout: LayoutParams },
}
```

### 2.3 Signal → Node Binding

```rust
impl SignalArena {
    fn subscribe_node(&mut self, signal: SignalId, node: NodeId, binding: NodeBinding) {
        self.graph.subscribers[signal.0].push(SubscriberId::UiNode(node));
        self.node_bindings.insert((signal, node), binding);
    }
}

pub enum NodeBinding {
    TextContent,
    Visibility,
    // futuro: Style properties
}
```

### 2.4 Componentes Lua Básicos

```lua
-- ui.text
ui.text { "static" }
ui.text { count }  -- signal
ui.text { "Count: " .. count }  -- concat com signal

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

-- ui.when
ui.when(condition, ui.text { "Visible!" })
ui.when(condition, 
    ui.text { "True" },
    ui.text { "False" }
)

-- ui.each
ui.each(items, function(item, index)
    return ui.text { key = item.id, item.name }
end)
```

### 2.5 TUI Renderer

```rust
pub struct TuiRenderer {
    node_positions: HashMap<NodeId, Position>,
    terminal: Terminal,
}

impl Renderer for TuiRenderer {
    fn create_node(&mut self, node_type: NodeType) -> TuiHandle {
        // TUI não cria "objetos", só registra posição
        TuiHandle { id: self.next_id() }
    }
    
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let pos = self.node_positions[&node];
                self.terminal.move_cursor(pos.row, pos.col);
                self.terminal.clear_line();
                self.terminal.write(&value);
            }
            RenderCommand::Show { node } => {
                self.redraw_node(node);
            }
            RenderCommand::Hide { node } => {
                let pos = self.node_positions[&node];
                self.terminal.move_cursor(pos.row, pos.col);
                self.terminal.clear_line();
            }
            // ...
        }
    }
}
```

### 2.6 Layout Básico (Column/Row)

```rust
pub struct LayoutEngine {
    constraints: HashMap<NodeId, Constraints>,
    computed: HashMap<NodeId, ComputedLayout>,
}

impl LayoutEngine {
    fn compute(&mut self, root: NodeId, available: Size) {
        // Flexbox simplificado pra TUI
        // Column: empilha vertical
        // Row: empilha horizontal
    }
}
```

## Testes da Fase 2

```lua
-- test_ui_tui.lua

-- App de teste
function App()
    local count = signal(0)
    local show_double = signal(true)
    
    return ui.column {
        ui.text { "Counter App" },
        ui.text { "Count: " .. count },
        
        ui.when(show_double,
            ui.text { "Double: " .. (count * 2) }
        ),
        
        ui.row {
            ui.text { "[+]" },  -- vai virar button depois
            ui.text { "[-]" },
        }
    }
end
```

*Validação manual:*

1. Rodar no terminal
2. Incrementar count via input
3. Verificar que SÓ as linhas afetadas atualizam (não pisca tela toda)
4. Toggle show_double, verificar que linha aparece/desaparece

*Teste automatizado de granularidade:*

```rust
#[test]
fn test_granular_update() {
    let mut rover = Rover::new_tui();
    
    let count = rover.create_signal(0);
    let static_text = rover.create_text_node("Static");
    let dynamic_text = rover.create_text_node_signal(count);
    
    rover.flush();
    
    // Muda signal
    rover.set_signal(count, 1);
    
    // Pega comandos gerados
    let commands = rover.take_render_commands();
    
    // DEVE ter só 1 comando, pro dynamic_text
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        RenderCommand::UpdateText { node, value } => {
            assert_eq!(*node, dynamic_text);
            assert_eq!(value, "1");
        }
        _ => panic!("Wrong command type"),
    }
}
```

## Estrutura de Arquivos Fase 2

```
rover-core/
├── src/
│   ├── signal/          # Fase 1
│   ├── node/
│   │   ├── mod.rs
│   │   ├── arena.rs     # NodeArena
│   │   ├── types.rs     # Node enum, TextNode, etc
│   │   ├── binding.rs   # Signal → Node bindings
│   │   └── commands.rs  # RenderCommand
│   ├── layout/
│   │   ├── mod.rs
│   │   └── engine.rs    # Layout computation
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── trait.rs     # Renderer trait
│   │   └── tui.rs       # TUI implementation
│   └── lua/
│       ├── signal.rs    # Fase 1
│       └── ui.rs        # ui.text, ui.column, etc
```
