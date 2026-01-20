# Fase 3: Web Renderer (DOM)

**Status:** 🔲 Não Iniciado  
**Duration:** 2 semanas  
**Dependencies:** Fase 2

## Objetivo

Validar que a abstração funciona em plataforma real. Mesmo código Lua, renderer diferente.

## Entregas

### 3.1 WASM Build Setup

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Document", "Element", "HtmlElement", "Node", "Text", "Window", "console"] }
```

### 3.2 Web Renderer

```rust
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
```

### 3.3 Event Handling (Web)

```rust
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
```

## Validação Fase 3

```lua
-- Mesmo código da Fase 2 DEVE funcionar
function App()
    local count = signal(0)
    
    return ui.column {
        ui.text { "Count: " .. count },
        ui.button {
            text = "+",
            on_press = function() count.val = count.val + 1 end
        }
    end
end

-- Rodar com: TUI renderer → funciona
-- Rodar com: Web renderer → funciona
-- Código Lua: IDÊNTICO
```

*Teste de performance (DevTools):*

1. Abrir Performance tab
2. Incrementar contador 100x
3. Verificar que NÃO há "Recalculate Style" em cascata
4. Verificar que só o span do texto muda

## Estrutura de Arquivos Fase 3

```
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
```
