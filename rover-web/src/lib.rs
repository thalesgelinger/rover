use wasm_bindgen::prelude::*;

pub mod platform;
pub mod renderer;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Run a Lua app in the browser
///
/// # Arguments
/// * `lua_code` - Lua source code that creates the UI
/// * `container_id` - DOM element ID to mount the app into
#[wasm_bindgen]
pub fn run_app(_lua_code: &str, container_id: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let container = document
        .get_element_by_id(container_id)
        .ok_or("container not found")?;

    // Initialize runtime
    let runtime = std::rc::Rc::new(rover_ui::signal::SignalRuntime::new());

    // TODO: Run Lua code to build UI tree
    // For now, create a simple test node
    let root_node = {
        let mut arena = runtime.node_arena.borrow_mut();
        use rover_ui::node::{Node, TextContent};
        arena.create(Node::text(TextContent::Static("Hello from Rover!".into())))
    };

    // Create web renderer
    let mut renderer = renderer::WebRenderer::new(container, runtime.clone());

    // Mount the UI tree
    {
        let arena = runtime.node_arena.borrow();
        renderer.mount_tree(root_node, &arena);
    }

    // Create and start platform
    let platform = platform::WebPlatform::new(renderer, runtime);
    platform.start()?;

    Ok(())
}
