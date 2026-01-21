# Fase 3: Web Renderer (DOM)

**Status:** Not Started
**Duration:** 2 semanas
**Dependencies:** Fase 2

## Agent Context

### Prerequisites
- Phase 2 must be complete (Node system, RenderCommands working)
- Same Lua code should work on both TUI and Web platforms
- The Renderer trait abstraction enables platform-agnostic code

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Lua App Code                             │
│   local count = rover.signal(0)                                 │
│   return ui.text { count }                                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     SignalRuntime (Rust)                        │
│   Signal change → Notify subscribers → Generate RenderCommands  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      RenderCommand                              │
│   UpdateText { node: NodeId, value: "42" }                      │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│      TuiRenderer        │     │      WebRenderer        │
│   (ratatui terminal)    │     │   (web-sys DOM)         │
│   node_text.insert()    │     │   element.set_text()    │
└─────────────────────────┘     └─────────────────────────┘
```

### What Needs to Be Built

1. **rover-web crate** - New workspace member with wasm-bindgen
2. **WebRenderer** - Implements Renderer trait using web-sys
3. **WebPlatform** - Implements PlatformHandler for browser events
4. **WASM tests** - Headless browser testing with wasm-pack

## Objetivo

Segundo renderer usando DOM para validar que a mesma logica de signal funciona em multiplas plataformas.

## Entregas

### 3.1 WASM Build Setup

#### Workspace Configuration

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "rover-ui",
    "rover-web",  # NEW
    "rover-cli",
]
```

```toml
# rover-web/Cargo.toml
[package]
name = "rover-web"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
rover_ui = { path = "../rover-ui" }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = { version = "0.3", features = [
    "Document",
    "Element",
    "HtmlElement",
    "Node",
    "Text",
    "Window",
    "console",
    "KeyboardEvent",
    "MouseEvent",
    "EventTarget",
    "AddEventListenerOptions",
] }
console_error_panic_hook = "0.1"

[dev-dependencies]
wasm-bindgen-test = "0.3"

[profile.release]
opt-level = "s"
lto = true
```

#### Module Structure

```rust
// rover-web/src/lib.rs
use wasm_bindgen::prelude::*;

pub mod platform;
pub mod renderer;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn run_app(lua_code: &str, container_id: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let container = document
        .get_element_by_id(container_id)
        .ok_or("container not found")?;

    // Initialize runtime
    let runtime = rover_ui::SignalRuntime::new_shared();

    // Run Lua code to build UI tree
    let root_node = rover_ui::lua::run_lua_app(&runtime, lua_code)?;

    // Create web renderer and platform
    let renderer = renderer::WebRenderer::new(container, runtime.clone());
    let platform = platform::WebPlatform::new(renderer, runtime);

    // Start event loop (requestAnimationFrame based)
    platform.start()?;

    Ok(())
}
```

### 3.2 Web Renderer Implementation

```rust
// rover-web/src/renderer.rs
use rover_ui::node::{NodeArena, NodeId, Node, RenderCommand};
use rover_ui::layout::LayoutEngine;
use rover_ui::renderer::Renderer;
use rover_ui::SharedSignalRuntime;
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Text};

pub struct WebRenderer {
    container: Element,
    runtime: SharedSignalRuntime,
    document: Document,
    // Map NodeId -> DOM element
    elements: HashMap<NodeId, Element>,
    // Map NodeId -> Text node (for text content)
    text_nodes: HashMap<NodeId, Text>,
}

impl WebRenderer {
    pub fn new(container: Element, runtime: SharedSignalRuntime) -> Self {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        Self {
            container,
            runtime,
            document,
            elements: HashMap::new(),
            text_nodes: HashMap::new(),
        }
    }

    fn get_or_create_element(&mut self, node_id: NodeId, arena: &NodeArena) -> Element {
        if let Some(el) = self.elements.get(&node_id) {
            return el.clone();
        }

        let node = arena.get(node_id).expect("node not found");
        let element = match node {
            Node::Text(_) => {
                let span = self.document.create_element("span").unwrap();
                let text = self.document.create_text_node("");
                span.append_child(&text).unwrap();
                self.text_nodes.insert(node_id, text);
                span
            }
            Node::Column(_) => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: column;").unwrap();
                div
            }
            Node::Row(_) => {
                let div = self.document.create_element("div").unwrap();
                div.set_attribute("style", "display: flex; flex-direction: row;").unwrap();
                div
            }
            Node::Conditional(_) | Node::Each(_) => {
                // Wrapper div for conditional/list content
                self.document.create_element("div").unwrap()
            }
        };

        self.elements.insert(node_id, element.clone());
        element
    }

    pub fn mount_tree(&mut self, root: NodeId, arena: &NodeArena) {
        // Clear container
        self.container.set_inner_html("");

        // Recursively mount nodes
        let root_element = self.mount_node(root, arena);
        self.container.append_child(&root_element).unwrap();
    }

    fn mount_node(&mut self, node_id: NodeId, arena: &NodeArena) -> Element {
        let element = self.get_or_create_element(node_id, arena);

        // Mount children
        let children = arena.children(node_id);
        for child_id in children {
            let child_element = self.mount_node(child_id, arena);
            element.append_child(&child_element).unwrap();
        }

        element
    }
}

impl Renderer for WebRenderer {
    fn apply(&mut self, cmd: &RenderCommand, arena: &NodeArena, _layout: &LayoutEngine) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                if let Some(text_node) = self.text_nodes.get(node) {
                    text_node.set_data(value);
                }
            }
            RenderCommand::Show { node } => {
                if let Some(element) = self.elements.get(node) {
                    element.set_attribute("style",
                        &format!("{} display: block;",
                            element.get_attribute("style").unwrap_or_default()
                                .replace("display: none;", "")
                        )
                    ).unwrap();
                }
            }
            RenderCommand::Hide { node } => {
                if let Some(element) = self.elements.get(node) {
                    element.set_attribute("style",
                        &format!("{} display: none;",
                            element.get_attribute("style").unwrap_or_default()
                        )
                    ).unwrap();
                }
            }
            RenderCommand::InsertChild { parent, index, child } => {
                let parent_el = self.get_or_create_element(*parent, arena);
                let child_el = self.get_or_create_element(*child, arena);

                let children = parent_el.children();
                if *index >= children.length() as usize {
                    parent_el.append_child(&child_el).unwrap();
                } else {
                    let ref_node = children.item(*index as u32);
                    parent_el.insert_before(&child_el, ref_node.as_ref()).unwrap();
                }
            }
            RenderCommand::RemoveChild { parent, index } => {
                if let Some(parent_el) = self.elements.get(parent) {
                    let children = parent_el.children();
                    if let Some(child) = children.item(*index as u32) {
                        parent_el.remove_child(&child).unwrap();
                    }
                }
            }
            RenderCommand::MountTree { root } => {
                self.mount_tree(*root, arena);
            }
            RenderCommand::ReplaceEach { node, children } => {
                if let Some(element) = self.elements.get(node) {
                    // Clear existing children
                    element.set_inner_html("");
                    // Add new children
                    for child_id in children {
                        let child_el = self.mount_node(*child_id, arena);
                        element.append_child(&child_el).unwrap();
                    }
                }
            }
        }
    }

    fn render_frame(
        &mut self,
        _root: NodeId,
        _arena: &NodeArena,
        _layout: &LayoutEngine,
        _runtime: &SharedSignalRuntime,
    ) -> std::io::Result<()> {
        // DOM updates are immediate, no frame rendering needed
        Ok(())
    }
}
```

### 3.3 Web Platform (Event Handling)

```rust
// rover-web/src/platform.rs
use rover_ui::platform::{PlatformEvent, PlatformHandler, KeyModifier};
use rover_ui::SharedSignalRuntime;
use crate::renderer::WebRenderer;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

pub struct WebPlatform {
    renderer: Rc<RefCell<WebRenderer>>,
    runtime: SharedSignalRuntime,
    event_queue: Rc<RefCell<Vec<PlatformEvent>>>,
}

impl WebPlatform {
    pub fn new(renderer: WebRenderer, runtime: SharedSignalRuntime) -> Self {
        Self {
            renderer: Rc::new(RefCell::new(renderer)),
            runtime,
            event_queue: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn start(self) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;

        // Setup keyboard listener
        let event_queue = self.event_queue.clone();
        let keydown_closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let mut modifiers = Vec::new();
            if event.shift_key() { modifiers.push(KeyModifier::Shift); }
            if event.ctrl_key() { modifiers.push(KeyModifier::Control); }
            if event.alt_key() { modifiers.push(KeyModifier::Alt); }
            if event.meta_key() { modifiers.push(KeyModifier::Meta); }

            event_queue.borrow_mut().push(PlatformEvent::KeyDown {
                key: event.key(),
                modifiers,
            });
        }) as Box<dyn FnMut(_)>);

        document.add_event_listener_with_callback(
            "keydown",
            keydown_closure.as_ref().unchecked_ref(),
        )?;
        keydown_closure.forget();

        // Start animation frame loop
        self.request_animation_frame()?;

        Ok(())
    }

    fn request_animation_frame(&self) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;

        let renderer = self.renderer.clone();
        let runtime = self.runtime.clone();
        let event_queue = self.event_queue.clone();

        let closure = Closure::once(Box::new(move || {
            // Process tick
            runtime.tick();

            // Apply render commands
            let commands = runtime.take_render_commands();
            let arena = runtime.node_arena.borrow();
            let mut renderer = renderer.borrow_mut();

            for cmd in &commands {
                renderer.apply(cmd, &arena, &rover_ui::layout::LayoutEngine::new());
            }

            // Clear processed events
            event_queue.borrow_mut().clear();

            // Schedule next frame
            // Note: In real impl, store platform in Rc and call request_animation_frame again
        }) as Box<dyn FnOnce()>);

        window.request_animation_frame(closure.as_ref().unchecked_ref())?;
        closure.forget();

        Ok(())
    }
}

impl PlatformHandler for WebPlatform {
    fn init(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn wait_for_event(&mut self, _timeout: Duration) -> std::io::Result<Option<PlatformEvent>> {
        // Web uses event-driven model, not polling
        let mut queue = self.event_queue.borrow_mut();
        Ok(queue.pop())
    }

    fn render(&mut self) -> std::io::Result<()> {
        // DOM updates are immediate
        Ok(())
    }

    fn cleanup(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
```

### 3.4 WASM Tests

```rust
// rover-web/tests/web.rs
use wasm_bindgen_test::*;
use rover_web::renderer::WebRenderer;
use rover_ui::node::{Node, NodeArena, RenderCommand, TextContent};
use rover_ui::SignalRuntime;

wasm_bindgen_test_configure!(run_in_browser);

fn setup_test_container() -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let container = document.create_element("div").unwrap();
    container.set_id("test-container");
    document.body().unwrap().append_child(&container).unwrap();
    container
}

#[wasm_bindgen_test]
fn test_text_node_renders() {
    let container = setup_test_container();
    let runtime = SignalRuntime::new_shared();
    let mut renderer = WebRenderer::new(container.clone(), runtime.clone());

    // Create text node
    let node_id = {
        let mut arena = runtime.node_arena.borrow_mut();
        arena.create(Node::text(TextContent::Static("Hello World".into())))
    };

    // Mount and verify
    let arena = runtime.node_arena.borrow();
    renderer.mount_tree(node_id, &arena);

    assert_eq!(container.inner_html(), "<span>Hello World</span>");
}

#[wasm_bindgen_test]
fn test_update_text_command() {
    let container = setup_test_container();
    let runtime = SignalRuntime::new_shared();
    let mut renderer = WebRenderer::new(container.clone(), runtime.clone());

    // Create and mount text node
    let node_id = {
        let mut arena = runtime.node_arena.borrow_mut();
        arena.create(Node::text(TextContent::Static("Initial".into())))
    };

    {
        let arena = runtime.node_arena.borrow();
        renderer.mount_tree(node_id, &arena);
    }

    // Apply update command
    let cmd = RenderCommand::UpdateText {
        node: node_id,
        value: "Updated".to_string(),
    };

    {
        let arena = runtime.node_arena.borrow();
        let layout = rover_ui::layout::LayoutEngine::new();
        renderer.apply(&cmd, &arena, &layout);
    }

    assert!(container.inner_html().contains("Updated"));
}

#[wasm_bindgen_test]
fn test_column_layout() {
    let container = setup_test_container();
    let runtime = SignalRuntime::new_shared();
    let mut renderer = WebRenderer::new(container.clone(), runtime.clone());

    // Create column with children
    let (col_id, child1, child2) = {
        let mut arena = runtime.node_arena.borrow_mut();
        let child1 = arena.create(Node::text(TextContent::Static("First".into())));
        let child2 = arena.create(Node::text(TextContent::Static("Second".into())));
        let col = arena.create(Node::column());

        arena.set_parent(child1, Some(col));
        arena.set_parent(child2, Some(col));

        if let Some(Node::Column(c)) = arena.get_mut(col) {
            c.children.push(child1);
            c.children.push(child2);
        }

        (col, child1, child2)
    };

    {
        let arena = runtime.node_arena.borrow();
        renderer.mount_tree(col_id, &arena);
    }

    let html = container.inner_html();
    assert!(html.contains("flex-direction: column"));
    assert!(html.contains("First"));
    assert!(html.contains("Second"));
}

#[wasm_bindgen_test]
fn test_show_hide_commands() {
    let container = setup_test_container();
    let runtime = SignalRuntime::new_shared();
    let mut renderer = WebRenderer::new(container.clone(), runtime.clone());

    let node_id = {
        let mut arena = runtime.node_arena.borrow_mut();
        arena.create(Node::text(TextContent::Static("Toggle me".into())))
    };

    {
        let arena = runtime.node_arena.borrow();
        renderer.mount_tree(node_id, &arena);
    }

    // Hide
    {
        let arena = runtime.node_arena.borrow();
        let layout = rover_ui::layout::LayoutEngine::new();
        renderer.apply(&RenderCommand::Hide { node: node_id }, &arena, &layout);
    }
    assert!(container.inner_html().contains("display: none"));

    // Show
    {
        let arena = runtime.node_arena.borrow();
        let layout = rover_ui::layout::LayoutEngine::new();
        renderer.apply(&RenderCommand::Show { node: node_id }, &arena, &layout);
    }
    assert!(!container.inner_html().contains("display: none"));
}
```

## Build & Test Commands

```bash
# Install wasm-pack if not present
cargo install wasm-pack

# Build WASM package
cd rover-web
wasm-pack build --target web

# Run headless browser tests
wasm-pack test --headless --chrome

# Or with Firefox
wasm-pack test --headless --firefox

# Build for npm (if distributing)
wasm-pack build --target bundler
```

## Example HTML Usage

```html
<!DOCTYPE html>
<html>
<head>
    <title>Rover Web Demo</title>
</head>
<body>
    <div id="app"></div>

    <script type="module">
        import init, { run_app } from './pkg/rover_web.js';

        async function main() {
            await init();

            const luaCode = `
                local count = rover.signal(0)

                return ui.column {
                    ui.text { "Count: " .. count },
                    ui.button {
                        text = "+",
                        on_press = function() count.val = count.val + 1 end
                    }
                }
            `;

            run_app(luaCode, "app");
        }

        main();
    </script>
</body>
</html>
```

## Validation Checklist

- [ ] `wasm-pack build` completes without errors
- [ ] `wasm-pack test --headless --chrome` all tests pass
- [ ] Text node renders correctly in DOM
- [ ] UpdateText command updates DOM text
- [ ] Column renders with `flex-direction: column`
- [ ] Row renders with `flex-direction: row`
- [ ] Show/Hide commands toggle visibility
- [ ] Same Lua code works on both TUI and Web

## Performance Validation (DevTools)

1. Open browser DevTools Performance tab
2. Increment counter 100x rapidly
3. Verify NO "Recalculate Style" cascades
4. Verify only the affected text span changes

## Phase 3.4: CLI Dev Server Integration (TODO)

### Goal: Seamless Developer Experience

Running `rover examples/counter.lua --platform web` should "just work" like the TUI version:

```bash
# User runs this:
rover examples/counter.lua --platform web

# System automatically:
# 1. Compiles WASM (cached in .rover/)
# 2. Starts dev server on localhost:4242
# 3. Opens browser automatically
# 4. Hot-reloads on file changes
# 5. NO generated files in project directory
```

### Implementation Requirements

1. **Auto WASM Compilation**
   - Detect `--platform web` flag in rover-cli
   - Run `wasm-pack build` automatically
   - Cache output in `.rover/wasm-cache/`
   - Rebuild only when rover-web or lua file changes

2. **Embedded Dev Server**
   - **Reuse rover-server infrastructure** (already battle-tested!)
   - Serve on `localhost:4242` by default
   - Add static file serving for WASM assets
   - Auto-generate HTML wrapper that:
     - Loads WASM module from `.rover/wasm-cache/`
     - Calls `run_app(lua_code, "root")`
     - Injects the Lua file content

3. **Zero Config Philosophy**
   - No `index.html` needed
   - No `Cargo.toml` in user project
   - No webpack/bundler setup
   - Just `rover file.lua --platform web` and it works

4. **File Watching (Optional)**
   - Watch lua file for changes
   - Trigger WASM rebuild if needed
   - WebSocket for browser refresh

### Example User Flow

```bash
# User writes counter.lua
local count = rover.signal(0)
return ui.column {
    ui.text { "Count: " .. count },
    ui.button { text = "+", on_press = function() count.val = count.val + 1 end }
}

# User runs:
$ rover counter.lua --platform web

# Output:
🚀 Compiling WASM...
✅ Compiled successfully
🌐 Server running at http://localhost:4242
   Press Ctrl+C to stop

# Browser auto-opens with the counter app running
# No files generated in user's directory
```

### Hidden .rover/ Structure

```
project/
├── counter.lua          # User's file (clean!)
└── .rover/              # Hidden cache (gitignored)
    ├── wasm-cache/
    │   ├── rover_web_bg.wasm
    │   ├── rover_web.js
    │   └── build-hash.txt
    └── server/
        └── generated-index.html
```

### Implementation Strategy

**Leverage existing rover-server infrastructure:**

1. **rover-cli detects `--platform web`**
   ```rust
   // In rover-cli/src/main.rs
   match args.platform {
       Platform::Web => {
           // 1. Compile WASM to .rover/wasm-cache/
           compile_wasm(&lua_file)?;

           // 2. Start rover-server in dev mode
           let config = ServerConfig {
               port: 4242,
               static_files: ".rover/wasm-cache/",
               lua_file: lua_file.clone(),
               dev_mode: true,
           };
           start_dev_server(config)?;
       }
       Platform::Tui => { /* existing TUI logic */ }
   }
   ```

2. **Extend rover-server for static file serving**
   - Add route for `/_rover/wasm/*` → serves from `.rover/wasm-cache/`
   - Add route for `/` → serves auto-generated HTML
   - Reuse existing event loop, routing, and logging infrastructure

3. **Benefits of reusing rover-server**
   - ✅ No new HTTP server dependencies
   - ✅ Consistent architecture across TUI and Web
   - ✅ Easy to add hot-reload (already has event loop)
   - ✅ Reuse existing request/response handling
   - ✅ Battle-tested production code

### Priority

This is **Phase 3.4** and should be implemented after Phase 3.1-3.3 are stable. For now, users can manually use wasm-pack, but the goal is to hide all complexity.

### Related Work

- Multi-page apps (routing, navigation) → Later phase
- Production builds → `rover build --platform web --release`
- Static site generation → Future consideration
