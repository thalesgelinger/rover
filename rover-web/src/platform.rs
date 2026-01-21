use rover_ui::platform::tui::{KeyModifier, PlatformEvent, PlatformHandler};
use rover_ui::renderer::Renderer;
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
            if event.shift_key() {
                modifiers.push(KeyModifier::Shift);
            }
            if event.ctrl_key() {
                modifiers.push(KeyModifier::Control);
            }
            if event.alt_key() {
                modifiers.push(KeyModifier::Alt);
            }
            if event.meta_key() {
                modifiers.push(KeyModifier::Meta);
            }

            event_queue.borrow_mut().push(PlatformEvent::KeyDown {
                key: event.key(),
                modifiers,
            });
        })
            as Box<dyn FnMut(_)>);

        document
            .add_event_listener_with_callback("keydown", keydown_closure.as_ref().unchecked_ref())?;
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
            // Check if we need to process updates
            if runtime.tick() {
                // Process pending node updates
                runtime.process_node_updates();

                // Apply render commands
                let commands = runtime.take_render_commands();
                let arena = runtime.node_arena.borrow();
                let mut renderer = renderer.borrow_mut();

                for cmd in &commands {
                    renderer.apply(cmd, &arena, &rover_ui::layout::LayoutEngine::new());
                }
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
        // Process updates
        self.runtime.process_node_updates();

        // Apply render commands
        let commands = self.runtime.take_render_commands();
        let arena = self.runtime.node_arena.borrow();
        let mut renderer = self.renderer.borrow_mut();
        let layout = rover_ui::layout::LayoutEngine::new();

        for cmd in &commands {
            renderer.apply(cmd, &arena, &layout);
        }

        Ok(())
    }

    fn cleanup(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
