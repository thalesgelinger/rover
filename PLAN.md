# Rover UI — Plano de Implementação

	⁠Framework Lua-native, signal-driven, cross-platform com views nativas

-----

## Visão Geral da Arquitetura


┌─────────────────────────────────────────────────────────────────┐
│                         RUST CORE                               │
├─────────────────────────────────────────────────────────────────┤
│  Signal Arena    →    Subscriber Graph    →    Node Registry    │
│  (valores)            (dependências)           (handles)        │
│                              │                                  │
│                              ▼                                  │
│                    Render Command Queue                         │
│                    (UpdateText, InsertChild, etc)               │
└─────────────────────────────────────────────────────────────────┘
                               │
         ┌─────────────────────┼─────────────────────┐
         ▼                     ▼                     ▼
   ┌──────────┐          ┌──────────┐          ┌──────────┐
   │   TUI    │          │   Web    │          │  Native  │
   │  (ANSI)  │          │  (DOM)   │          │(UIKit/AV)│
   └──────────┘          └──────────┘          └──────────┘


### Princípio Core


Signal mudou → Notifica subscribers → Gera RenderCommand → Mutação imperativa
     │                                        │
     └── O(k) onde k = subscribers ───────────┘
     
NÃO:
Diff tree → O(n) onde n = tamanho da árvore


-----

## Fases de Implementação

-----

## Fase 1: Signal System (Rust Core)

### Objetivo

Implementar o sistema de signals completamente isolado, testável sem UI.

### Duração Estimada

2-3 semanas

### Entregas

#### 1.1 Signal Arena (Storage)

⁠ rust
pub struct SignalArena {
    values: Vec<SignalValue>,
    versions: Vec<u64>,
    current_epoch: u64,
}

pub struct SignalId(u32);

pub enum SignalValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(StringId),
    Table(TableId),
}
 ⁠

*Critério de validação:*

•⁠  ⁠[ ] Criar signal com valor inicial
•⁠  ⁠[ ] Ler valor via ⁠ .val ⁠
•⁠  ⁠[ ] Escrever valor via ⁠ .val = ⁠
•⁠  ⁠[ ] Zero allocation em read/write

#### 1.2 Subscriber Graph

⁠ rust
pub struct SubscriberGraph {
    // Quem eu dependo
    dependencies: Vec<SmallVec<[SignalId; 4]>>,
    // Quem depende de mim
    subscribers: Vec<SmallVec<[SubscriberId; 8]>>,
}

pub enum SubscriberId {
    DerivedSignal(SignalId),
    Effect(EffectId),
    UiNode(NodeId),  // pra fase 2
}
 ⁠

*Critério de validação:*

•⁠  ⁠[ ] Registrar dependência signal → subscriber
•⁠  ⁠[ ] Notificar subscribers quando signal muda
•⁠  ⁠[ ] Propagação em cascata (A → B → C)
•⁠  ⁠[ ] Sem notificação se valor não mudou realmente

#### 1.3 Derived Signals

⁠ rust
pub struct DerivedSignal {
    id: SignalId,
    compute: ComputeFn,
    cached_value: SignalValue,
    dirty: bool,
}
 ⁠

*Critério de validação:*

•⁠  ⁠[ ] ⁠ derive(fn) ⁠ cria signal computado
•⁠  ⁠[ ] Recomputa quando dependências mudam
•⁠  ⁠[ ] Lazy evaluation (só computa quando lido)
•⁠  ⁠[ ] Cache de valor (não recomputa se limpo)

#### 1.4 Magic Metamethods (Lua Bindings)

⁠ lua
-- Operadores retornam derived signals
local double = count * 2
local is_big = count > 10
local label = "Count: " .. count
 ⁠

*Implementar metamethods:*

•⁠  ⁠[ ] ⁠ __add ⁠, ⁠ __sub ⁠, ⁠ __mul ⁠, ⁠ __div ⁠, ⁠ __mod ⁠, ⁠ __unm ⁠
•⁠  ⁠[ ] ⁠ __concat ⁠
•⁠  ⁠[ ] ⁠ __eq ⁠, ⁠ __lt ⁠, ⁠ __le ⁠
•⁠  ⁠[ ] ⁠ __tostring ⁠

*Critério de validação:*

⁠ lua
local count = signal(5)
local double = count * 2
assert(double.val == 10)

count.val = 20
assert(double.val == 40)  -- reatividade funciona

local is_big = count > 10
assert(is_big.val == true)
 ⁠

#### 1.5 Effects

⁠ rust
pub struct Effect {
    id: EffectId,
    callback: LuaFunction,
    dependencies: SmallVec<[SignalId; 4]>,
}
 ⁠

*Critério de validação:*

•⁠  ⁠[ ] Effect roda uma vez no registro
•⁠  ⁠[ ] Effect re-roda quando dependências mudam
•⁠  ⁠[ ] Tracking automático de dependências
•⁠  ⁠[ ] Cleanup function opcional

#### 1.6 Utilities

⁠ lua
rover.any(a, b, c)   -- true se qualquer um true
rover.all(a, b, c)   -- true se todos true
rover.none(a, b, c)  -- true se nenhum true
 ⁠

### Testes da Fase 1

⁠ lua
-- test_signals.lua

-- Básico
local count = signal(0)
assert(count.val == 0)
count.val = 5
assert(count.val == 5)

-- Derived implícito
local double = count * 2
assert(double.val == 10)
count.val = 10
assert(double.val == 20)

-- Derived explícito
local info = derive(function()
    return "Count is " .. count.val .. ", double is " .. (count.val * 2)
end)
assert(info.val == "Count is 10, double is 20")

-- Comparisons
local is_big = count > 5
assert(is_big.val == true)
count.val = 3
assert(is_big.val == false)

-- Effect
local effect_count = 0
effect(function()
    local _ = count.val  -- subscribe
    effect_count = effect_count + 1
end)
assert(effect_count == 1)  -- roda no mount
count.val = 100
assert(effect_count == 2)  -- roda na mudança

-- Cascata
local a = signal(1)
local b = a * 2
local c = b * 2
assert(c.val == 4)
a.val = 5
assert(c.val == 20)  -- propaga através de b

-- Não notifica se valor igual
effect_count = 0
local stable = signal(10)
effect(function()
    local _ = stable.val
    effect_count = effect_count + 1
end)
assert(effect_count == 1)
stable.val = 10  -- mesmo valor
assert(effect_count == 1)  -- não re-rodou
 ⁠

### Estrutura de Arquivos Fase 1


rover-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── signal/
│   │   ├── mod.rs
│   │   ├── arena.rs        # SignalArena
│   │   ├── value.rs        # SignalValue enum
│   │   ├── graph.rs        # SubscriberGraph
│   │   ├── derived.rs      # DerivedSignal
│   │   └── effect.rs       # Effect system
│   └── lua/
│       ├── mod.rs
│       ├── signal.rs       # Lua bindings for signal
│       └── metamethods.rs  # __add, __mul, etc
└── tests/
    └── signal_tests.rs


-----

## Fase 2: UI Core + TUI Renderer

### Objetivo

Implementar componentes básicos com renderer TUI para validar arquitetura signal → comando → mutação.

### Duração Estimada

2-3 semanas

### Entregas

#### 2.1 Node System

⁠ rust
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
 ⁠

#### 2.2 Render Commands

⁠ rust
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
 ⁠

#### 2.3 Signal → Node Binding

⁠ rust
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
 ⁠

#### 2.4 Componentes Lua Básicos

⁠ lua
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
 ⁠

#### 2.5 TUI Renderer

⁠ rust
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
 ⁠

#### 2.6 Layout Básico (Column/Row)

⁠ rust
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
 ⁠

### Testes da Fase 2

⁠ lua
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
 ⁠

*Validação manual:*

1.⁠ ⁠Rodar no terminal
1.⁠ ⁠Incrementar count via input
1.⁠ ⁠Verificar que SÓ as linhas afetadas atualizam (não pisca tela toda)
1.⁠ ⁠Toggle show_double, verificar que linha aparece/desaparece

*Teste automatizado de granularidade:*

⁠ rust
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
 ⁠

### Estrutura de Arquivos Fase 2


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


-----

## Fase 3: Web Renderer (DOM)

### Objetivo

Validar que a abstração funciona em plataforma real. Mesmo código Lua, renderer diferente.

### Duração Estimada

2 semanas

### Entregas

#### 3.1 WASM Build Setup

⁠ toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Document", "Element", "HtmlElement", "Node", "Text", "Window", "console"] }
 ⁠

#### 3.2 Web Renderer

⁠ rust
pub struct WebRenderer {
    document: web_sys::Document,
    node_refs: HashMap<NodeId, web_sys::Element>,
}

impl Renderer for WebRenderer {
    fn create_node(&mut self, node_type: NodeType) -> WebHandle {
        let el = match node_type {
            NodeType::Text => self.document.create_element("span").unwrap(),
            NodeType::Column => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: column;").unwrap();
                div
            }
            NodeType::Row => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: row;").unwrap();
                div
            }
        };
        WebHandle(el)
    }
    
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let el = &self.node_refs[&node];
                el.set_text_content(Some(&value));
            }
            RenderCommand::InsertChild { parent, index, child } => {
                let parent_el = &self.node_refs[&parent];
                let child_el = &self.node_refs[&child];
                let reference = parent_el.children().item(index as u32);
                parent_el.insert_before(child_el, reference.as_ref()).unwrap();
            }
            RenderCommand::RemoveChild { parent, index } => {
                let parent_el = &self.node_refs[&parent];
                if let Some(child) = parent_el.children().item(index as u32) {
                    parent_el.remove_child(&child).unwrap();
                }
            }
            // ...
        }
    }
}
 ⁠

#### 3.3 Event Handling (Web)

⁠ rust
impl WebRenderer {
    fn attach_event(&mut self, node: NodeId, event: &str, callback: LuaFunction) {
        let el = &self.node_refs[&node];
        let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
            // Chama callback Lua
            callback.call::<_, ()>(()).unwrap();
        }) as Box<dyn FnMut(_)>);
        
        el.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
}
 ⁠

### Validação Fase 3

⁠ lua
-- Mesmo código da Fase 2 DEVE funcionar
function App()
    local count = signal(0)
    
    return ui.column {
        ui.text { "Count: " .. count },
        ui.button {
            text = "+",
            on_press = function() count.val = count.val + 1 end
        }
    }
end

-- Rodar com: TUI renderer → funciona
-- Rodar com: Web renderer → funciona
-- Código Lua: IDÊNTICO
 ⁠

*Teste de performance (DevTools):*

1.⁠ ⁠Abrir Performance tab
1.⁠ ⁠Incrementar contador 100x
1.⁠ ⁠Verificar que NÃO há “Recalculate Style” em cascata
1.⁠ ⁠Verificar que só o span do texto muda

### Estrutura de Arquivos Fase 3


rover-core/
├── src/
│   └── renderer/
│       ├── tui.rs
│       └── web.rs       # NOVO

rover-web/                # Package separado pra WASM
├── Cargo.toml
├── src/
│   └── lib.rs           # wasm-bindgen exports
└── www/
    ├── index.html
    └── index.js


-----

## Fase 4: Styling Modifiers

### Objetivo

Implementar sistema de modifiers semânticos que funcionam em TUI e Web.

### Duração Estimada

2-3 semanas

### Entregas

#### 4.1 Modifier Chain System

⁠ rust
pub struct ModifierChain {
    modifiers: SmallVec<[Modifier; 8]>,
}

pub enum Modifier {
    // Layout
    Fill,
    Wrap,
    Gap(SpacingToken),
    Pad(SpacingToken),
    Center,
    CrossAlign(Alignment),
    MainAlign(Distribution),
    
    // Visual
    Tint(ColorToken),
    Surface(SurfaceToken),
    Radius(RadiusToken),
    
    // Conditional
    When(SignalId, Box<ModifierChain>),
    On(EventType, Box<ModifierChain>),
}

pub enum SpacingToken { None, Xs, Sm, Md, Lg, Xl }
pub enum ColorToken { Default, Muted, Primary, Danger, Success, Warning }
pub enum SurfaceToken { Flat, Raised, Filled, Ghost }
 ⁠

#### 4.2 Lua Chainable API

⁠ lua
local mod = rover.ui.mod

-- Chainable
mod:fill():center():gap("md"):surface("raised")

-- Condicional
mod:when(loading, mod:opacity("muted"))
mod:on("hover", mod:elevate("sm"))
 ⁠

⁠ rust
// Implementação via metatable
impl LuaMod {
    fn fill(this: &LuaMod) -> LuaMod {
        let mut chain = this.chain.clone();
        chain.push(Modifier::Fill);
        LuaMod { chain }
    }
    
    fn gap(this: &LuaMod, token: String) -> LuaMod {
        let mut chain = this.chain.clone();
        chain.push(Modifier::Gap(SpacingToken::from_str(&token)));
        LuaMod { chain }
    }
}
 ⁠

#### 4.3 Token → Platform Value Mapping

⁠ rust
pub trait TokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32;
    fn resolve_color(&self, token: ColorToken) -> Color;
    fn resolve_radius(&self, token: RadiusToken) -> f32;
}

pub struct TuiTokenResolver;
impl TokenResolver for TuiTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::None => 0.0,
            SpacingToken::Xs => 1.0,   // 1 char
            SpacingToken::Sm => 1.0,
            SpacingToken::Md => 2.0,   // 2 chars
            SpacingToken::Lg => 3.0,
            SpacingToken::Xl => 4.0,
        }
    }
    // TUI não tem cor real, pode usar ANSI codes
}

pub struct WebTokenResolver;
impl TokenResolver for WebTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::None => 0.0,
            SpacingToken::Xs => 4.0,   // 4px
            SpacingToken::Sm => 8.0,
            SpacingToken::Md => 16.0,
            SpacingToken::Lg => 24.0,
            SpacingToken::Xl => 32.0,
        }
    }
}
 ⁠

#### 4.4 Modifier → RenderCommand

⁠ rust
impl ModifierChain {
    fn to_style_commands(&self, node: NodeId, resolver: &dyn TokenResolver) -> Vec<RenderCommand> {
        let mut commands = vec![];
        
        for modifier in &self.modifiers {
            match modifier {
                Modifier::Gap(token) => {
                    let value = resolver.resolve_spacing(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Gap,
                        value: StyleValue::Float(value),
                    });
                }
                Modifier::Surface(token) => {
                    let (bg, shadow) = resolver.resolve_surface(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Background,
                        value: StyleValue::Color(bg),
                    });
                }
                Modifier::When(signal, inner_chain) => {
                    // Registra subscription
                    // Quando signal muda, aplica/remove inner_chain
                }
                // ...
            }
        }
        
        commands
    }
}
 ⁠

#### 4.5 Conditional Modifiers (⁠ :when ⁠)

⁠ rust
pub struct ConditionalModifier {
    condition: SignalId,
    modifiers: ModifierChain,
    active: bool,
}

impl ConditionalModifier {
    fn on_signal_change(&mut self, new_value: bool, node: NodeId, renderer: &mut dyn Renderer) {
        if new_value && !self.active {
            // Aplica modifiers
            for cmd in self.modifiers.to_style_commands(node) {
                renderer.apply(cmd);
            }
            self.active = true;
        } else if !new_value && self.active {
            // Remove/reverte modifiers
            for cmd in self.modifiers.to_reset_commands(node) {
                renderer.apply(cmd);
            }
            self.active = false;
        }
    }
}
 ⁠

### Validação Fase 4

⁠ lua
function StyledCard()
    local hovered = signal(false)
    
    return ui.column {
        mod = mod
            :pad("md")
            :gap("sm")
            :surface("raised")
            :radius("md")
            :when(hovered, mod:elevate("md"):shadow("soft")),
        
        on_hover_start = function() hovered.val = true end,
        on_hover_end = function() hovered.val = false end,
        
        ui.text { "Card Title", mod = mod:size("lg"):weight("bold") },
        ui.text { "Card content", mod = mod:tint("muted") },
    }
end
 ⁠

*TUI:* Card aparece com padding de espaços, sem cor (ou ANSI colors)
*Web:* Card aparece com padding em px, cores reais, shadow no hover

-----

## Fase 5: Events e Interatividade

### Objetivo

Implementar sistema de eventos consistente entre plataformas.

### Duração Estimada

1-2 semanas

### Entregas

#### 5.1 Event Types

⁠ rust
pub enum EventType {
    // Universal
    Press,
    LongPress,
    
    // Pointer (desktop/web)
    Hover,
    HoverEnd,
    
    // Focus (accessibility)
    Focus,
    Blur,
    
    // Lifecycle
    Mount,
    Unmount,
    Visible,  // entrou no viewport
}
 ⁠

#### 5.2 Event → Modifier Trigger

⁠ rust
pub struct EventModifier {
    event: EventType,
    modifiers: ModifierChain,
}

// on_press, on_hover, etc viram bindings internos
// que setam um signal interno e disparam :when
 ⁠

#### 5.3 Input Components

⁠ lua
-- Button
ui.button {
    text = "Click me",
    icon = "plus",  -- opcional
    on_press = function() end,
    mod = mod:intent("primary"),
}

-- Input
ui.input {
    value = email,  -- signal, two-way binding
    placeholder = "Email",
    secure = false,  -- password mode
    on_submit = function() end,
    on_focus = function() end,
    on_blur = function() end,
}

-- Checkbox
ui.checkbox {
    checked = remember_me,  -- signal
    on_change = function(new_value) end,
}

-- Switch
ui.switch {
    value = dark_mode,
    on_change = function(new_value) end,
}
 ⁠

#### 5.4 Two-Way Binding

⁠ rust
// ui.input { value = signal }
// Internamente:
// - Lê signal pra valor inicial
// - Em onChange nativo, seta signal.val = novo_valor
// - Signal change propaga de volta pro input (caso setado externamente)
 ⁠

### Validação Fase 5

⁠ lua
function LoginForm()
    local email = signal("")
    local password = signal("")
    local loading = signal(false)
    local error = signal(nil)
    
    local is_valid = derive(function()
        return #email.val > 0 and #password.val > 0
    end)
    
    local submit = function()
        loading.val = true
        -- HTTP call aqui (fase futura)
    end
    
    return ui.column {
        mod = mod:gap("md"):pad("lg"):width("sm"):center(),
        
        ui.text { "Login", mod = mod:size("xl"):weight("bold") },
        
        ui.input {
            value = email,
            placeholder = "Email",
            mod = mod:fill(),
        },
        
        ui.input {
            value = password,
            placeholder = "Password",
            secure = true,
            mod = mod:fill(),
        },
        
        ui.when(error, function()
            return ui.text { error, mod = mod:tint("danger") }
        end),
        
        ui.button {
            text = "Login",
            on_press = submit,
            mod = mod
                :intent("primary")
                :fill()
                :when(not is_valid, mod:opacity("muted"):disabled())
                :when(loading, mod:opacity("muted"):disabled()),
        },
    }
end
 ⁠

-----

## Fase 6: iOS Renderer (UIKit)

### Objetivo

Primeira plataforma mobile nativa. Validar que arquitetura escala pra mobile.

### Duração Estimada

3-4 semanas

### Entregas

#### 6.1 Rust → Swift/ObjC Bridge

⁠ rust
// Via swift-bridge ou manual FFI
#[repr(C)]
pub struct RoverBridge {
    // Callbacks pro Swift
    create_view: extern "C" fn(NodeType) -> *mut c_void,
    update_text: extern "C" fn(*mut c_void, *const c_char),
    insert_subview: extern "C" fn(*mut c_void, *mut c_void, usize),
    // ...
}
 ⁠

⁠ swift
// Swift side
class RoverRenderer {
    var viewRefs: [UInt32: UIView] = [:]
    
    func createView(_ nodeType: NodeType) -> UnsafeMutableRawPointer {
        let view: UIView
        switch nodeType {
        case .text:
            view = UILabel()
        case .column:
            view = UIStackView()
            (view as! UIStackView).axis = .vertical
        case .row:
            view = UIStackView()
            (view as! UIStackView).axis = .horizontal
        case .button:
            view = UIButton(type: .system)
        }
        // Store and return pointer
    }
    
    func updateText(_ viewPtr: UnsafeMutableRawPointer, _ text: UnsafePointer<CChar>) {
        let label = Unmanaged<UILabel>.fromOpaque(viewPtr).takeUnretainedValue()
        label.text = String(cString: text)
    }
}
 ⁠

#### 6.2 iOS Renderer Implementation

⁠ rust
pub struct IosRenderer {
    bridge: RoverBridge,
    node_refs: HashMap<NodeId, *mut c_void>,
}

impl Renderer for IosRenderer {
    fn create_node(&mut self, node_type: NodeType) -> IosHandle {
        let ptr = (self.bridge.create_view)(node_type);
        IosHandle(ptr)
    }
    
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let ptr = self.node_refs[&node];
                let c_str = CString::new(value).unwrap();
                (self.bridge.update_text)(ptr, c_str.as_ptr());
            }
            RenderCommand::InsertChild { parent, index, child } => {
                let parent_ptr = self.node_refs[&parent];
                let child_ptr = self.node_refs[&child];
                (self.bridge.insert_subview)(parent_ptr, child_ptr, index);
            }
        }
    }
}
 ⁠

#### 6.3 Layout com Auto Layout ou Manual

*Opção A: Auto Layout*

⁠ swift
func applyLayout(_ viewPtr: UnsafeMutableRawPointer, _ constraints: LayoutConstraints) {
    let view = Unmanaged<UIView>.fromOpaque(viewPtr).takeUnretainedValue()
    view.translatesAutoresizingMaskIntoConstraints = false
    
    NSLayoutConstraint.activate([
        view.widthAnchor.constraint(equalToConstant: constraints.width),
        // ...
    ])
}
 ⁠

*Opção B: Manual frames (mais controle)*

⁠ swift
func applyLayout(_ viewPtr: UnsafeMutableRawPointer, _ frame: CGRect) {
    let view = Unmanaged<UIView>.fromOpaque(viewPtr).takeUnretainedValue()
    view.frame = frame
}
 ⁠

#### 6.4 Token Resolver iOS

⁠ rust
pub struct IosTokenResolver {
    scale: f32,  // UIScreen.main.scale
}

impl TokenResolver for IosTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::Xs => 4.0 * self.scale,
            SpacingToken::Sm => 8.0 * self.scale,
            SpacingToken::Md => 16.0 * self.scale,
            SpacingToken::Lg => 24.0 * self.scale,
            SpacingToken::Xl => 32.0 * self.scale,
        }
    }
}
 ⁠

### Validação Fase 6

•⁠  ⁠Mesmo código Lua do LoginForm roda no iOS
•⁠  ⁠Performance: 60fps em scroll de lista
•⁠  ⁠Memory: baseline < 20MB

-----

## Fase 7: Android Renderer

### Objetivo

Segunda plataforma mobile. Mesma arquitetura, renderer diferente.

### Duração Estimada

2-3 semanas

### Entregas

#### 7.1 Rust → JNI Bridge

⁠ rust
#[no_mangle]
pub extern "system" fn Java_com_rover_RoverBridge_createView(
    env: JNIEnv,
    _: JClass,
    node_type: jint,
) -> jobject {
    // Cria View via JNI
}

#[no_mangle]
pub extern "system" fn Java_com_rover_RoverBridge_updateText(
    env: JNIEnv,
    _: JClass,
    view: jobject,
    text: JString,
) {
    let text: String = env.get_string(text).unwrap().into();
    env.call_method(view, "setText", "(Ljava/lang/CharSequence;)V", &[JValue::Object(text.into())]).unwrap();
}
 ⁠

#### 7.2 Android Renderer

⁠ rust
pub struct AndroidRenderer {
    env: JNIEnv,
    node_refs: HashMap<NodeId, jobject>,
}

impl Renderer for AndroidRenderer {
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let view = self.node_refs[&node];
                let jstr = self.env.new_string(&value).unwrap();
                self.env.call_method(view, "setText", "(Ljava/lang/CharSequence;)V", &[jstr.into()]).unwrap();
            }
        }
    }
}
 ⁠

### Validação Fase 7

•⁠  ⁠Mesmo código Lua roda no Android
•⁠  ⁠Paridade visual com iOS (tokens resolvem pra valores apropriados)

-----

## Fase 8: Animation Modifiers

### Objetivo

Implementar sistema de animações baseado em modifiers + timing.

### Duração Estimada

3-4 semanas

### Entregas

#### 8.1 Timing Modifiers

⁠ rust
pub struct TimingConfig {
    duration: Duration,
    curve: EasingCurve,
    delay: Duration,
    loop_mode: LoopMode,
}

pub enum EasingCurve {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    Spring { damping: f32, stiffness: f32 },
    Bounce,
}

pub enum LoopMode {
    None,
    Forever,
    Count(u32),
}
 ⁠

#### 8.2 Animated Properties

⁠ rust
pub struct AnimatedProperty {
    property: StyleProperty,
    from: StyleValue,
    to: StyleValue,
    timing: TimingConfig,
    progress: f32,
}

impl AnimatedProperty {
    fn tick(&mut self, dt: Duration) -> StyleValue {
        self.progress += dt.as_secs_f32() / self.timing.duration.as_secs_f32();
        let t = self.timing.curve.apply(self.progress.min(1.0));
        self.from.interpolate(&self.to, t)
    }
}
 ⁠

#### 8.3 Animation Loop

⁠ rust
impl Rover {
    fn animation_tick(&mut self, dt: Duration) {
        for anim in &mut self.active_animations {
            let new_value = anim.tick(dt);
            self.render_queue.push(RenderCommand::UpdateStyle {
                node: anim.node,
                property: anim.property,
                value: new_value,
            });
            
            if anim.is_complete() {
                self.completed_animations.push(anim.id);
            }
        }
    }
}
 ⁠

#### 8.4 Platform Animation Integration

⁠ rust
// iOS: usar CAAnimation quando possível
// Android: usar Animator
// Web: usar CSS transitions/Web Animations API
// TUI: tick-based manual

impl IosRenderer {
    fn apply_animated(&mut self, cmd: RenderCommand, timing: TimingConfig) {
        match cmd {
            RenderCommand::UpdateStyle { node, property: StyleProperty::Opacity, value } => {
                let view = self.node_refs[&node];
                UIView.animate(withDuration: timing.duration) {
                    view.alpha = value.as_f64()
                }
            }
        }
    }
}
 ⁠

#### 8.5 Lua API

⁠ lua
mod:on("mount",
    mod
        :opacity("full")
        :move("y", "none")
        :duration("normal")
        :curve("spring")
)

mod:on("hover",
    mod:elevate("md"):shadow("soft"):duration("fast")
)

mod:when(loading,
    mod:rotate(360):duration("long"):loop()
)
 ⁠

### Validação Fase 8

⁠ lua
function AnimatedCard()
    local expanded = signal(false)
    
    return ui.column {
        on_press = function() expanded.val = not expanded.val end,
        mod = mod
            :surface("raised")
            :radius("md")
            :pad("md")
            :on("mount", 
                mod:opacity("full"):move("y", "none"):duration("normal"):curve("spring"))
            :on("hover",
                mod:elevate("md"):duration("fast"))
            :when(expanded,
                mod:height("lg"):duration("normal"):curve("spring")),
        
        ui.text { "Click to expand" },
        
        ui.when(expanded, function()
            return ui.text { 
                "Expanded content!",
                mod = mod:on("mount", mod:opacity("full"):duration("fast"))
            }
        end)
    }
end
 ⁠

*Validar:*

•⁠  ⁠Animação de entrada suave
•⁠  ⁠Hover feedback
•⁠  ⁠Expand/collapse animado
•⁠  ⁠60fps constante durante animações

-----

## Fase 9: HTTP e Async

### Objetivo

Implementar I/O não-bloqueante com sintaxe síncrona.

### Duração Estimada

2 semanas

### Entregas

#### 9.1 Async Runtime Integration

⁠ rust
// Usar tokio ou async-std internamente
// Expor API síncrona pro Lua via coroutines

pub fn http_get(lua: &Lua, url: String) -> LuaResult<LuaValue> {
    // Suspende coroutine Lua
    // Faz request async
    // Resume coroutine com resultado
}
 ⁠

#### 9.2 Lua API

⁠ lua
-- Parece síncrono, mas não bloqueia UI
local data = rover.http.get("/users")

-- Com error handling
local ok, result = pcall(function()
    return rover.http.post("/login", { email = email.val, password = password.val })
end)

if ok then
    -- success
else
    error.val = result
end
 ⁠

#### 9.3 Effect + HTTP

⁠ lua
rover.effect(function()
    local filter = filter.val  -- subscribe
    loading.val = true
    
    local ok, data = pcall(function()
        return rover.http.get("/items?filter=" .. filter)
    end)
    
    if ok then
        items.val = data
    else
        error.val = data
    end
    
    loading.val = false
end)
 ⁠

-----

## Fase 10: Polish e Otimizações

### Objetivo

Otimizar performance, reduzir bundle size, melhorar DX.

### Duração Estimada

Ongoing

### Entregas

#### 10.1 List Virtualization

⁠ lua
-- ui.each com virtualization automática pra listas grandes
ui.each(items, function(item)
    return ui.row { key = item.id, ... }
end, { virtualize = true })
 ⁠

#### 10.2 Batch Updates

⁠ rust
// Agrupa múltiplos signal changes em um frame
rover.batch(function()
    a.val = 1
    b.val = 2
    c.val = 3
end)  -- Só uma render pass
 ⁠

#### 10.3 DevTools

•⁠  ⁠Inspector de signals
•⁠  ⁠Visualização de subscriber graph
•⁠  ⁠Performance profiler
•⁠  ⁠Hot reload

#### 10.4 Error Boundaries

⁠ lua
ui.catch(
    function()
        return RiskyComponent {}
    end,
    function(err)
        return ui.text { "Error: " .. err, mod = mod:tint("danger") }
    end
)
 ⁠

-----

## Cronograma Estimado

|Fase            |Duração    |Dependências|
|----------------|-----------|------------|
|1. Signals      |2-3 semanas|-           |
|2. UI Core + TUI|2-3 semanas|Fase 1      |
|3. Web Renderer |2 semanas  |Fase 2      |
|4. Styling      |2-3 semanas|Fase 3      |
|5. Events       |1-2 semanas|Fase 4      |
|6. iOS          |3-4 semanas|Fase 5      |
|7. Android      |2-3 semanas|Fase 5      |
|8. Animations   |3-4 semanas|Fase 6, 7   |
|9. HTTP/Async   |2 semanas  |Fase 5      |
|10. Polish      |Ongoing    |-           |

*Total estimado: 5-7 meses* para MVP funcional em todas plataformas.

-----

## Métricas de Sucesso

### Performance Targets

|Métrica        |Target |React Native|Flutter  |
|---------------|-------|------------|---------|
|Update simples |< 1ms  |3-10ms      |1-3ms    |
|Cold start     |< 300ms|800-2000ms  |300-600ms|
|Memory baseline|< 15MB |40-80MB     |30-50MB  |
|Bundle size    |< 3MB  |7-15MB      |5-10MB   |
|60fps scroll   |Sim    |Difícil     |Sim      |

### Validation Checklist

•⁠  ⁠[ ] Mesmo código Lua roda em TUI, Web, iOS, Android
•⁠  ⁠[ ] Update granular comprovado (não re-renderiza siblings)
•⁠  ⁠[ ] Zero allocation em steady-state updates
•⁠  ⁠[ ] 60fps em animações
•⁠  ⁠[ ] Memory não cresce em uso prolongado

-----

## Riscos e Mitigações

|Risco                         |Probabilidade|Impacto|Mitigação                         |
|------------------------------|-------------|-------|----------------------------------|
|Layout engine complexo        |Alta         |Alto   |Usar Taffy (flexbox Rust)         |
|JNI overhead Android          |Média        |Médio  |Batch commands, minimize crossings|
|Lua GC pauses                 |Média        |Médio  |Arena allocation, object pooling  |
|iOS App Store rejection       |Baixa        |Alto   |Seguir guidelines, não usar JIT   |
|Performance não atinge targets|Média        |Alto   |Profile early, optimize hot paths |

-----

## Próximos Passos Imediatos

1.⁠ ⁠*Setup repositório* com estrutura de Fase 1
1.⁠ ⁠*Implementar SignalArena* básico
1.⁠ ⁠*Testes de signal* sem UI
1.⁠ ⁠*Bindings Lua* com mlua
1.⁠ ⁠*Validar metamethods* funcionam como esperado

-----

## Notas: Future Multi-Threading (Lynx-Style)

### Objetivo

Implementar arquitetura multi-thread para mobile nativo (iOS/Android), separando UI thread de compute thread.

### Arquitetura Lynx-Style

```
┌─────────────────────────────────────────────────────────┐
│                 Main Thread (UI)                        │
│  - Event handling                                       │
│  - Renderer (native views)                              │
│  - Layout computation                                   │
└──────────────┬──────────────────────────────────────────┘
               │ Message Channel
               │ (Command Queue)
               ▼
┌─────────────────────────────────────────────────────────┐
│              Worker Thread (Compute)                    │
│  - Lua VM                                               │
│  - Signal Runtime (Arc<RwLock<SignalRuntime>>)          │
│  - Effect execution                                     │
│  - Derived computation                                  │
└─────────────────────────────────────────────────────────┘
```

### Signal Runtime Thread-Safe Design

```rust
// Phase 1: Single-threaded (Lua app_data)
pub struct SignalRuntime {
    arena: SignalArena,
    graph: SubscriberGraph,
    // ...
}

// Future: Multi-threaded (Arc + RwLock)
pub struct SharedSignalRuntime {
    inner: Arc<RwLock<SignalRuntime>>,
}

impl SharedSignalRuntime {
    pub fn read_signal(&self, id: SignalId) -> SignalValue {
        self.inner.read().unwrap().get_signal(id).clone()
    }
    
    pub fn write_signal(&self, id: SignalId, value: SignalValue) {
        let mut rt = self.inner.write().unwrap();
        rt.set_signal(id, value);
        // Flush render commands to main thread via channel
    }
}
```

### Message Passing

```rust
pub enum UICommand {
    RenderCommand(RenderCommand),
    RunEffect(EffectId),
    Batch(Vec<UICommand>),
}

pub enum WorkerMessage {
    SignalChanged(SignalId, SignalValue),
    UserEvent(EventType, NodeId),
    Shutdown,
}

// Main thread → Worker
let (worker_tx, worker_rx) = channel::<WorkerMessage>();

// Worker → Main thread  
let (main_tx, main_rx) = channel::<UICommand>();
```

### Thread-Safe Primitives

**Primitives** (`Bool`, `Int`, `Float`, `String`):
- Already `Send + Sync`
- Can be cloned across thread boundary

**Tables**:
- Option A: Keep thread-local, serialize for cross-thread
- Option B: Use Arc<RwLock<LuaTable>> with careful lifetime management
- **Recommended**: Thread-local signals, message passing for updates

### Effect Execution

Effects always run on **worker thread**:
```rust
// Worker thread
fn run_effect(lua: &Lua, effect_id: EffectId) -> Result<()> {
    let mut rt = get_runtime(lua);
    let effect = &rt.effects[effect_id.0 as usize];
    
    // Run cleanup
    if let Some(cleanup_key) = &effect.cleanup {
        let cleanup: Function = lua.registry_value(cleanup_key)?;
        cleanup.call::<_, ()>(())?;
    }
    
    // Run callback
    let callback: Function = lua.registry_value(&effect.callback)?;
    let result = callback.call::<_, Value>(())?;
    
    // Check if returned cleanup fn
    if let Value::Function(cleanup) = result {
        rt.effects[effect_id.0 as usize].cleanup = Some(lua.create_registry_value(cleanup)?);
    }
    
    // Flush render commands to main thread
    let commands = rt.take_render_commands();
    main_tx.send(UICommand::Batch(commands))?;
    
    Ok(())
}
```

### Considerations

1. **Lock granularity**: Use fine-grained locks per subsystem (arena, graph, effects) instead of single big lock
2. **Lock-free structures**: Consider crossbeam or dashmap for lock-free collections where possible
3. **Batching**: Reduce cross-thread messages by batching render commands
4. **Deadlock prevention**: Never hold multiple locks, always acquire in same order
5. **GC coordination**: Lua GC runs on worker thread, doesn't block UI

### Migration Path (Phase 1 → Future)

1. **Phase 1**: Implement with passed runtime (`lua.app_data`)
2. **Phase 2-5**: Keep single-threaded
3. **Phase 6-7**: Add `SharedSignalRuntime` wrapper for mobile
4. **Validate**: Primitives work across threads, tables remain thread-local

**Phase 1 Registry Cleanup Limitation:**

In Phase 1, proper disposal of `RegistryKey` values is skipped for simplicity. This means:

- **Memory leak for long-running apps**: Registry values (functions, tables stored in signals/derived/effects) will accumulate in Lua's registry
- **Impact**: Each signal/derived/effect stores 1-3 RegistryKeys. For apps creating 10,000+ signals/effects over time, this could grow significantly
- **Workaround**: Restart Lua VM periodically to clear registry
- **Future fix**: Phase 6+ will add proper disposal with explicit lifecycle management

**Why skip for Phase 1:**
- `RegistryKey` cannot be cloned, making clean disposal in `__gc` difficult
- Need to replace keys with placeholder values, which requires careful memory management
- For Phase 1 scope (short-lived processes, TUI/Web targets), the impact is acceptable
- Proper disposal will be implemented when adding UI node lifecycle in Phase 2+

### Performance Targets (Multi-Thread)

- Signal read: < 100ns (no contention)
- Signal write: < 500ns (lock acquisition + notify)
- Effect execution: runs async, doesn't block UI
- 120fps rendering (ProMotion displays)

-----


---

## Phase 1 - Completion Notes

### Status: ✅ COMPLETE

Phase 1 (Signal System) was completed successfully. All deliverables working:
- Signal Arena (storage, versioning, recycling)
- Subscriber Graph (dependencies, propagation)
- Derived Signals (lazy evaluation, caching)
- Magic Metamethods (arithmetic, concat)
- Effects (lifecycle, cleanup, auto-tracking)
- Utilities (any, all)

**Test Coverage:** 20 unit tests + integration test (signal_test.lua)

### Important Limitation Discovered

**Comparison Operators Cannot Create Reactive Signals**

Due to Lua's semantics, comparison operators (`<`, `>`, `<=`, `>=`, `==`, `~=`) must return boolean values and cannot return derived signals.

❌ **Does NOT work:**
```lua
local count = rover.signal(10)
local is_big = count > 5  -- Returns plain boolean, NOT a signal!
```

✅ **Use rover.derive() instead:**
```lua
local count = rover.signal(10)
local is_big = rover.derive(function()
    return count.val > 5
end)
print(is_big.val)  -- true (reactive!)
```

**Reason:** Lua requires comparison metamethods (__lt, __le, __eq) to return booleans for use in conditionals. This is a language-level constraint.

**Documentation:** Complete documentation created in:
- `/docs/docs/guides/signals.md` - User guide with examples and patterns
- `/docs/docs/api-reference/signals.md` - Complete API reference

