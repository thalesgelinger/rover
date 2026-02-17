use crate::coroutine::{CoroutineResult, run_coroutine_with_delay};
use crate::events::{EventQueue, UiEvent};
use crate::platform::{
    DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH, UiRuntimeConfig, ViewportSignals,
};
use crate::scheduler::{Scheduler, SharedScheduler};
use crate::signal::{SignalRuntime, SignalValue};
use crate::ui::node::UiNode;
use crate::ui::registry::UiRegistry;
use crate::ui::renderer::Renderer;
use mlua::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Main application struct combining all systems
pub struct App<R: Renderer> {
    /// Lua interpreter (owned)
    lua: Lua,
    /// Signal runtime (shared with Lua app_data)
    runtime: Rc<SignalRuntime>,
    /// UI registry (shared with Lua app_data)
    registry: Rc<RefCell<UiRegistry>>,
    /// Coroutine scheduler (shared with Lua app_data)
    scheduler: SharedScheduler,
    /// Event queue
    events: EventQueue,
    /// Renderer
    renderer: R,
    /// Running state
    running: bool,
}

impl<R: Renderer> App<R> {
    /// Create a new App with the given renderer
    pub fn new(renderer: R) -> mlua::Result<Self> {
        let lua = Lua::new();
        let target = renderer.target();
        let runtime = Rc::new(SignalRuntime::new());
        let registry = Rc::new(RefCell::new(UiRegistry::new()));
        let scheduler: SharedScheduler = Rc::new(RefCell::new(Scheduler::new()));
        let runtime_config = UiRuntimeConfig::new(target);
        // TODO: replace these defaults with per-platform viewport providers (web/mobile/etc).
        let viewport_signals = ViewportSignals {
            width: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_WIDTH as i64)),
            height: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_HEIGHT as i64)),
        };

        // Store runtime, registry, and scheduler in Lua app_data for access from Lua
        lua.set_app_data(runtime.clone());
        lua.set_app_data(registry.clone());
        lua.set_app_data(scheduler.clone());
        lua.set_app_data(runtime_config);
        lua.set_app_data(viewport_signals);

        // Register rover module
        let rover_table = lua.create_table()?;
        crate::register_ui_module(&lua, &rover_table)?;
        lua.globals().set("rover", rover_table)?;

        Ok(Self {
            lua,
            runtime,
            registry,
            scheduler,
            events: EventQueue::new(),
            renderer,
            running: false,
        })
    }

    /// Get a reference to the Lua interpreter
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Get the signal runtime
    pub fn runtime(&self) -> &Rc<SignalRuntime> {
        &self.runtime
    }

    /// Get the UI registry
    pub fn registry(&self) -> &Rc<RefCell<UiRegistry>> {
        &self.registry
    }

    /// Get the scheduler
    pub fn scheduler(&self) -> SharedScheduler {
        self.scheduler.clone()
    }

    /// Get the event queue
    pub fn events(&mut self) -> &mut EventQueue {
        &mut self.events
    }

    /// Get the renderer
    pub fn renderer(&mut self) -> &mut R {
        &mut self.renderer
    }

    /// Run a Lua script to set up the UI
    pub fn run_script(&mut self, script: &str) -> mlua::Result<()> {
        self.lua.load(script).exec()?;
        Ok(())
    }

    /// Mount the UI by calling the global rover.render() function
    /// This should be called after running the user's script
    pub fn mount(&mut self) -> mlua::Result<()> {
        // Check if rover.render function exists
        let rover_table: mlua::Table = self.lua.globals().get("rover")?;
        let render_fn: Option<mlua::Function> = rover_table.get("render")?;

        if let Some(render_fn) = render_fn {
            // Call the render function to get the root node
            let root_node: crate::ui::lua_node::LuaNode = render_fn.call(())?;
            self.registry.borrow_mut().set_root(root_node.id());
        }

        // Trigger initial render (even if no render function defined)
        self.renderer.mount(&self.registry.borrow());
        self.running = true;

        Ok(())
    }

    /// Push an event to the event queue
    pub fn push_event(&mut self, event: UiEvent) {
        self.events.push(event);
    }

    /// Process one tick of the application loop
    pub fn tick(&mut self) -> mlua::Result<bool> {
        // Auto-mount on first tick (for testing convenience)
        // In production, run() calls mount() explicitly
        if !self.running {
            self.mount()?;
        }

        let now = Instant::now();

        // 1. Resume ready timers
        let ready_ids = self.scheduler.borrow_mut().tick(now);
        for id in ready_ids {
            let Ok(pending) = self.scheduler.borrow_mut().take_pending(id) else {
                continue;
            };
            match run_coroutine_with_delay(
                &self.lua,
                &self.runtime,
                &pending.thread,
                LuaValue::Nil,
            )? {
                CoroutineResult::Completed => {
                    // Coroutine finished, nothing more to do
                }
                CoroutineResult::YieldedDelay { delay_ms } => {
                    // Re-schedule with delay
                    self.scheduler.borrow_mut().schedule_delay_with_id(
                        id,
                        pending.thread,
                        delay_ms,
                    );
                }
                CoroutineResult::YieldedOther => {
                    // Unknown yield - could be an error
                }
            }
        }

        // 2. Process events (in signal batch)
        self.process_events()?;

        // 3. Flush any pending effects from signal updates
        self.flush_effects()?;

        // 4. Render dirty nodes
        let dirty_set = self.registry.borrow_mut().take_dirty_nodes();
        if !dirty_set.is_empty() {
            let dirty: Vec<_> = dirty_set.into_iter().collect();
            self.renderer.update(&self.registry.borrow(), &dirty);
        }

        Ok(self.running)
    }

    /// Run ticks for a specified duration (for testing)
    /// This processes all timers that are ready within the time window
    pub fn tick_ms(&mut self, duration_ms: u64) -> mlua::Result<()> {
        // Auto-mount on first call (for testing convenience)
        if !self.running {
            self.mount()?;
        }

        let start = Instant::now();
        let target = start + Duration::from_millis(duration_ms);

        while Instant::now() < target {
            let now = Instant::now();

            // Resume ready timers
            let ready_ids = self.scheduler.borrow_mut().tick(now);
            for id in ready_ids {
                let Ok(pending) = self.scheduler.borrow_mut().take_pending(id) else {
                    continue;
                };
                match run_coroutine_with_delay(
                    &self.lua,
                    &self.runtime,
                    &pending.thread,
                    LuaValue::Nil,
                )? {
                    CoroutineResult::Completed => {}
                    CoroutineResult::YieldedDelay { delay_ms } => {
                        self.scheduler.borrow_mut().schedule_delay_with_id(
                            id,
                            pending.thread,
                            delay_ms,
                        );
                    }
                    CoroutineResult::YieldedOther => {}
                }
            }

            // Process events
            self.process_events()?;

            // Render dirty nodes
            let dirty_set = self.registry.borrow_mut().take_dirty_nodes();
            if !dirty_set.is_empty() {
                let dirty: Vec<_> = dirty_set.into_iter().collect();
                self.renderer.update(&self.registry.borrow(), &dirty);
            }

            // Sleep a bit if there's pending work
            if self.scheduler.borrow().has_pending() {
                let sleep_dur = self
                    .scheduler
                    .borrow()
                    .next_wake_time()
                    .map(|wake| wake.saturating_duration_since(now))
                    .unwrap_or_else(|| Duration::from_millis(1))
                    .min(Duration::from_millis(10));
                std::thread::sleep(sleep_dur);
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Flush any pending effects from signal updates
    /// This ensures effects run even when there are no events
    fn flush_effects(&mut self) -> mlua::Result<()> {
        // Run any pending effects that were scheduled outside of a batch
        self.runtime
            .run_pending_effects(&self.lua)
            .map_err(|e| LuaError::RuntimeError(format!("Effect error: {:?}", e)))?;

        Ok(())
    }

    /// Process all pending events
    fn process_events(&mut self) -> mlua::Result<()> {
        let events: Vec<_> = self.events.drain().collect();

        if events.is_empty() {
            return Ok(());
        }

        // Begin batch for all event processing
        self.runtime.begin_batch();

        for event in events {
            self.dispatch_event(event)?;
        }

        // End batch and run effects
        self.runtime
            .end_batch(&self.lua)
            .map_err(|e| LuaError::RuntimeError(format!("Effect error: {:?}", e)))?;

        Ok(())
    }

    /// Dispatch a single event to the specific handler on its target node.
    ///
    /// Each event type maps to a specific effect field on the node:
    /// - Click → Button.on_click
    /// - Change → Input.on_change (and updates the bound signal for two-way binding)
    /// - Submit → Input.on_submit
    /// - Toggle → Checkbox.on_toggle
    /// - Key → KeyArea.on_key / FullScreen.on_key
    fn dispatch_event(&mut self, event: UiEvent) -> mlua::Result<()> {
        let node_id = event.node_id();

        // For Change events, update the bound signal first (two-way binding)
        if let UiEvent::Change { value, .. } = &event {
            let registry = self.registry.borrow();
            if let Some(UiNode::Input {
                value: text_content,
                ..
            }) = registry.get_node(node_id)
            {
                if let Some(signal_id) = text_content.signal_id() {
                    // Update the signal with the new value (two-way binding)
                    drop(registry);
                    self.runtime.set_signal(
                        &self.lua,
                        signal_id,
                        SignalValue::String(value.clone().into()),
                    );
                }
            }
        }

        let registry = self.registry.borrow();
        let node = match registry.get_node(node_id) {
            Some(n) => n,
            None => return Ok(()),
        };

        let effect_id = match (&event, node) {
            (UiEvent::Click { .. }, UiNode::Button { on_click, .. }) => *on_click,
            (UiEvent::Change { .. }, UiNode::Input { on_change, .. }) => *on_change,
            (UiEvent::Submit { .. }, UiNode::Input { on_submit, .. }) => *on_submit,
            (UiEvent::Toggle { .. }, UiNode::Checkbox { on_toggle, .. }) => *on_toggle,
            (UiEvent::Key { .. }, UiNode::KeyArea { on_key, .. }) => *on_key,
            (UiEvent::Key { .. }, UiNode::FullScreen { on_key, .. }) => *on_key,
            _ => None,
        };
        drop(registry);

        if let Some(effect_id) = effect_id {
            let args = match &event {
                UiEvent::Click { .. } => LuaValue::Nil,
                UiEvent::Change { value, .. } | UiEvent::Submit { value, .. } => {
                    LuaValue::String(self.lua.create_string(value)?)
                }
                UiEvent::Toggle { checked, .. } => LuaValue::Boolean(*checked),
                UiEvent::Key { key, .. } => LuaValue::String(self.lua.create_string(key)?),
            };

            if let Err(e) = self
                .runtime
                .call_effect(&self.lua, effect_id, args)
                .map_err(|e| LuaError::RuntimeError(format!("Effect error: {:?}", e)))
            {
                eprintln!("Event handler error: {:?}", e);
            }
        }

        Ok(())
    }

    /// Get the duration until the next timer fires
    pub fn next_wake_time(&self) -> Option<Instant> {
        self.scheduler.borrow().next_wake_time()
    }

    /// Check if the app is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Stop the application
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Update reactive viewport size signals.
    pub fn set_viewport_size(&mut self, cols: u16, rows: u16) {
        let Some(viewport) = self.lua.app_data_ref::<ViewportSignals>().map(|s| *s) else {
            return;
        };

        self.runtime
            .set_signal(&self.lua, viewport.width, SignalValue::Int(cols as i64));
        self.runtime
            .set_signal(&self.lua, viewport.height, SignalValue::Int(rows as i64));
    }

    /// Run the application loop (blocking)
    ///
    /// This will keep ticking until `stop()` is called or there are no more pending coroutines.
    pub fn run(&mut self) -> mlua::Result<()> {
        // Mount UI by calling global rover.render()
        self.mount()?;

        while self.running {
            self.tick()?;

            // Sleep until next timer or poll at 60fps
            let sleep_duration = self
                .next_wake_time()
                .map(|wake| {
                    let now = Instant::now();
                    if wake > now {
                        wake.saturating_duration_since(now)
                    } else {
                        Duration::ZERO
                    }
                })
                .unwrap_or_else(|| Duration::from_millis(16)); // ~60 FPS

            std::thread::sleep(sleep_duration);

            // Exit if no more pending work
            if !self.scheduler.borrow().has_pending() && self.events.is_empty() {
                break;
            }
        }

        Ok(())
    }
}

impl<R: Renderer> Drop for App<R> {
    fn drop(&mut self) {
        // Run cleanup callbacks
        let callbacks = self.registry.borrow_mut().take_on_destroy_callbacks();
        for key in callbacks {
            // Call the cleanup function
            if let Ok(func) = self.lua.registry_value::<mlua::Function>(&key) {
                if let Err(e) = func.call::<()>(()) {
                    eprintln!("Error in on_destroy callback: {:?}", e);
                }
            }
            // Remove from registry
            let _ = self.lua.remove_registry_value(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::UiTarget;
    use crate::ui::registry::UiRegistry;
    use crate::ui::renderer::Renderer;
    use crate::ui::stub::StubRenderer;

    struct TestTuiRenderer;

    impl Renderer for TestTuiRenderer {
        fn mount(&mut self, _registry: &UiRegistry) {}
        fn update(&mut self, _registry: &UiRegistry, _dirty_nodes: &[crate::ui::node::NodeId]) {}
        fn node_added(&mut self, _registry: &UiRegistry, _node_id: crate::ui::node::NodeId) {}
        fn node_removed(&mut self, _node_id: crate::ui::node::NodeId) {}
        fn target(&self) -> UiTarget {
            UiTarget::Tui
        }
    }

    #[test]
    fn test_app_creation() {
        let renderer = StubRenderer::new();
        let app = App::new(renderer);
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_run_simple_script() {
        let renderer = StubRenderer::new();
        let mut app = App::new(renderer).unwrap();

        let script = r#"
            local count = rover.signal(0)
            return count.val
        "#;

        let result: LuaValue = app.lua.load(script).eval().unwrap();
        if let LuaValue::Integer(n) = result {
            assert_eq!(n, 0);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_app_tick_no_errors() {
        let renderer = StubRenderer::new();
        let mut app = App::new(renderer).unwrap();

        // Tick should not error even with nothing to do
        let result = app.tick();
        assert!(result.is_ok());
    }

    #[test]
    fn test_app_push_event() {
        let renderer = StubRenderer::new();
        let mut app = App::new(renderer).unwrap();

        // Create a simple UI with a button
        let script = r#"
            local button = rover.ui.button({ label = "Click me" })
            return button.id
        "#;

        let node_id: u32 = app.lua.load(script).eval().unwrap();

        // Push an event
        app.push_event(UiEvent::Click {
            node_id: crate::ui::node::NodeId::from_u32(node_id),
        });

        assert_eq!(app.events.len(), 1);

        // Tick should process the event
        let result = app.tick();
        assert!(result.is_ok());
        assert_eq!(app.events.len(), 0);
    }

    #[test]
    fn test_require_rover_tui_fails_on_non_tui() {
        let renderer = StubRenderer::new();
        let app = App::new(renderer).unwrap();

        let (ok, err): (bool, String) = app
            .lua
            .load(
                r#"
                local ok, err = pcall(function()
                    require("rover.tui")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
        assert!(err.contains("require(\"rover.tui\") requires target=tui"));

        let is_nil: bool = app.lua.load("return rover.tui == nil").eval().unwrap();
        assert!(is_nil);
    }

    #[test]
    fn test_tui_namespace_available_on_tui_target() {
        let renderer = TestTuiRenderer;
        let app = App::new(renderer).unwrap();

        let (ui_select, ui_full_screen, tui_select, tui_nav_list, tui_progress): (
            String,
            String,
            String,
            String,
            String,
        ) = app
            .lua
            .load(
                r#"
                local ui_select = type(rover.ui.select)
                local ui_full_screen = type(rover.ui.full_screen)
                local tui_select = type(rover.tui.select)
                local tui_nav_list = type(rover.tui.nav_list)
                local tui_progress = type(rover.tui.progress)
                return ui_select, ui_full_screen, tui_select, tui_nav_list, tui_progress
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(ui_select, "nil");
        assert_eq!(ui_full_screen, "nil");
        assert_eq!(tui_select, "function");
        assert_eq!(tui_nav_list, "function");
        assert_eq!(tui_progress, "function");
    }

    #[test]
    fn test_tui_components_render_nodes_from_namespace() {
        let renderer = TestTuiRenderer;
        let app = App::new(renderer).unwrap();

        let node_kind: String = app
            .lua
            .load(
                r#"
                local node = rover.tui.select({
                    title = "x",
                    items = { "a", "b" },
                })
                return type(node) == "userdata" and "userdata" or type(node)
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(node_kind, "userdata");
    }

    #[test]
    fn test_full_screen_on_key_dispatches() {
        let renderer = TestTuiRenderer;
        let mut app = App::new(renderer).unwrap();

        app.lua
            .load(
                r#"
                _G.hit = rover.signal(0)
                function rover.render()
                    return rover.tui.full_screen {
                        on_key = function(key)
                            if key == "left" then
                                _G.hit.val = _G.hit.val + 1
                            end
                        end,
                        rover.ui.text { "x" },
                    }
                end
            "#,
            )
            .exec()
            .unwrap();

        app.mount().unwrap();

        let full_screen_id = {
            let reg = app.registry.borrow();
            let root = reg.root().unwrap();
            match reg.get_node(root).unwrap() {
                UiNode::FullScreen { on_key, .. } if on_key.is_some() => root,
                _ => panic!("expected full_screen root with on_key"),
            }
        };

        app.push_event(UiEvent::Key {
            node_id: full_screen_id,
            key: "left".to_string(),
        });
        app.tick().unwrap();

        let hit: i64 = app.lua.load("return _G.hit.val").eval().unwrap();
        assert_eq!(hit, 1);
    }

    #[test]
    fn test_ui_screen_signals_exposed_and_mutable() {
        let renderer = StubRenderer::new();
        let mut app = App::new(renderer).unwrap();

        let (w0, h0): (i64, i64) = app
            .lua
            .load("return rover.ui.screen.width.val, rover.ui.screen.height.val")
            .eval()
            .unwrap();
        assert_eq!(w0, 80);
        assert_eq!(h0, 24);

        app.set_viewport_size(123, 45);
        let (w1, h1): (i64, i64) = app
            .lua
            .load("return rover.ui.screen.width.val, rover.ui.screen.height.val")
            .eval()
            .unwrap();
        assert_eq!(w1, 123);
        assert_eq!(h1, 45);
    }
}
