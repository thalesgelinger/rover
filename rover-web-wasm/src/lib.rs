use mlua::{Lua, Value};
use rover_ui::coroutine::{run_coroutine_with_delay, CoroutineResult};
use rover_ui::events::{EventQueue, UiEvent};
use rover_ui::lua::register_ui_module;
use rover_ui::platform::{
    UiRuntimeConfig, UiTarget, ViewportSignals, DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH,
};
use rover_ui::scheduler::{Scheduler, SharedScheduler};
use rover_ui::signal::{SignalRuntime, SignalValue};
use rover_ui::ui::lua_node::LuaNode;
use rover_ui::ui::node::{NodeId, UiNode};
use rover_ui::ui::registry::UiRegistry;
use rover_ui::ui::renderer::Renderer;
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::rc::Rc;
use std::time::Instant;

struct WebRenderer {
    html: String,
}

impl WebRenderer {
    fn new() -> Self {
        Self {
            html: "<div></div>".to_string(),
        }
    }

    fn html(&self) -> &str {
        &self.html
    }

    fn rebuild(&mut self, registry: &UiRegistry) {
        self.html = if let Some(root) = registry.root() {
            self.render_node(registry, root)
        } else {
            "<div></div>".to_string()
        };
    }

    fn render_node(&self, registry: &UiRegistry, node_id: NodeId) -> String {
        let Some(node) = registry.get_node(node_id) else {
            return String::new();
        };

        match node {
            UiNode::Text { content } => format!("<div>{}</div>", html_escape(content.value())),
            UiNode::Button { label, .. } => {
                let rid = node_id.index() as u32;
                format!(
                    "<button data-rid=\"{}\">{}</button>",
                    rid,
                    html_escape(label)
                )
            }
            UiNode::Input { value, .. } => {
                let rid = node_id.index() as u32;
                format!(
                    "<input data-rid=\"{}\" value=\"{}\" />",
                    rid,
                    html_escape(value.value())
                )
            }
            UiNode::Checkbox { checked, .. } => {
                let rid = node_id.index() as u32;
                let checked_attr = if *checked { " checked" } else { "" };
                format!(
                    "<input type=\"checkbox\" data-rid=\"{}\"{} />",
                    rid, checked_attr
                )
            }
            UiNode::Column { children } => {
                let body = self.render_children(registry, children);
                format!(
                    "<div style=\"display:flex;flex-direction:column;gap:8px;\">{}</div>",
                    body
                )
            }
            UiNode::Row { children } => {
                let body = self.render_children(registry, children);
                format!(
                    "<div style=\"display:flex;flex-direction:row;gap:8px;align-items:center;\">{}</div>",
                    body
                )
            }
            UiNode::View { children }
            | UiNode::Stack { children }
            | UiNode::List { children, .. } => {
                let body = self.render_children(registry, children);
                format!("<div>{}</div>", body)
            }
            UiNode::ScrollBox { child, .. }
            | UiNode::FullScreen { child, .. }
            | UiNode::KeyArea { child, .. }
            | UiNode::Conditional { child, .. } => child
                .map(|id| self.render_node(registry, id))
                .unwrap_or_default(),
            UiNode::Image { src } => format!("<img src=\"{}\" />", html_escape(src)),
        }
    }

    fn render_children(&self, registry: &UiRegistry, children: &[NodeId]) -> String {
        let mut out = String::new();
        for child in children {
            out.push_str(&self.render_node(registry, *child));
        }
        out
    }
}

impl Renderer for WebRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        self.rebuild(registry);
    }

    fn update(&mut self, registry: &UiRegistry, _dirty_nodes: &[NodeId]) {
        self.rebuild(registry);
    }

    fn node_added(&mut self, registry: &UiRegistry, _node_id: NodeId) {
        self.rebuild(registry);
    }

    fn node_removed(&mut self, _node_id: NodeId) {}

    fn target(&self) -> UiTarget {
        UiTarget::Web
    }
}

pub struct Runtime {
    lua: Lua,
    signal_runtime: Rc<SignalRuntime>,
    registry: Rc<RefCell<UiRegistry>>,
    scheduler: SharedScheduler,
    events: EventQueue,
    renderer: WebRenderer,
    running: bool,
    html: CString,
    last_error: CString,
}

impl Runtime {
    fn new() -> Result<Self, String> {
        let lua = Lua::new();
        let signal_runtime = Rc::new(SignalRuntime::new());
        let registry = Rc::new(RefCell::new(UiRegistry::new()));
        let scheduler: SharedScheduler = Rc::new(RefCell::new(Scheduler::new()));
        let runtime_config = UiRuntimeConfig::new(UiTarget::Web);
        let viewport_signals = ViewportSignals {
            width: signal_runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_WIDTH as i64)),
            height: signal_runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_HEIGHT as i64)),
        };

        lua.set_app_data(signal_runtime.clone());
        lua.set_app_data(registry.clone());
        lua.set_app_data(scheduler.clone());
        lua.set_app_data(runtime_config);
        lua.set_app_data(viewport_signals);

        let rover_table = lua.create_table().map_err(|e| e.to_string())?;
        register_ui_module(&lua, &rover_table).map_err(|e| e.to_string())?;
        lua.globals()
            .set("rover", rover_table)
            .map_err(|e| e.to_string())?;

        Ok(Self {
            lua,
            signal_runtime,
            registry,
            scheduler,
            events: EventQueue::new(),
            renderer: WebRenderer::new(),
            running: false,
            html: CString::new("<div></div>").expect("static html has no nul"),
            last_error: CString::new("").expect("empty string has no nul"),
        })
    }

    fn set_html(&mut self, html: &str) {
        let safe = html.replace('\0', "");
        self.html = CString::new(safe).expect("nul bytes removed");
    }

    fn sync_html_from_renderer(&mut self) {
        let html = self.renderer.html().to_string();
        self.set_html(&html);
    }

    fn clear_error(&mut self) {
        self.last_error = CString::new("").expect("empty string has no nul");
    }

    fn set_error(&mut self, err: impl AsRef<str>) {
        let normalized = format_error(err.as_ref()).replace('\0', "");
        self.last_error = CString::new(normalized).expect("nul bytes removed");
    }

    fn last_error_ptr(&self) -> *const c_char {
        self.last_error.as_ptr()
    }

    fn run_script(&mut self, source: &str) -> Result<(), String> {
        self.lua
            .load("package.path = '/project/?.lua;/project/?/init.lua;' .. package.path")
            .exec()
            .map_err(|e| e.to_string())?;
        self.lua.load(source).exec().map_err(|e| e.to_string())
    }

    fn mount(&mut self) -> Result<(), String> {
        let rover_table: mlua::Table =
            self.lua.globals().get("rover").map_err(|e| e.to_string())?;
        let render_fn: Option<mlua::Function> =
            rover_table.get("render").map_err(|e| e.to_string())?;

        if let Some(render_fn) = render_fn {
            let root_node: LuaNode = render_fn.call(()).map_err(|e| e.to_string())?;
            self.registry.borrow_mut().set_root(root_node.id());
        }

        self.renderer.mount(&self.registry.borrow());
        self.running = true;
        Ok(())
    }

    fn process_events(&mut self) -> Result<(), String> {
        let events: Vec<_> = self.events.drain().collect();
        if events.is_empty() {
            return Ok(());
        }

        self.signal_runtime.begin_batch();

        for event in events {
            match event {
                UiEvent::Click { node_id } => {
                    let on_click = {
                        let reg = self.registry.borrow();
                        match reg.get_node(node_id) {
                            Some(UiNode::Button { on_click, .. }) => *on_click,
                            _ => None,
                        }
                    };

                    if let Some(effect_id) = on_click {
                        self.signal_runtime
                            .call_effect(&self.lua, effect_id, Value::Nil)
                            .map_err(|e| e.to_string())?;
                    }
                }
                _ => {}
            }
        }

        self.signal_runtime
            .end_batch(&self.lua)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn run_ready_timers(&mut self) -> Result<(), String> {
        let now = Instant::now();
        let ready_ids = self.scheduler.borrow_mut().tick(now);

        for id in ready_ids {
            let Ok(pending) = self.scheduler.borrow_mut().take_pending(id) else {
                continue;
            };

            match run_coroutine_with_delay(
                &self.lua,
                &self.signal_runtime,
                &pending.thread,
                Value::Nil,
            )
            .map_err(|e| e.to_string())?
            {
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

        Ok(())
    }

    fn flush_runtime(&mut self) -> Result<(), String> {
        if !self.running {
            self.mount()?;
        }

        self.run_ready_timers()?;

        self.process_events()?;
        self.signal_runtime
            .run_pending_effects(&self.lua)
            .map_err(|e| e.to_string())?;

        let dirty_set = self.registry.borrow_mut().take_dirty_nodes();
        if !dirty_set.is_empty() {
            let dirty: Vec<_> = dirty_set.into_iter().collect();
            self.renderer.update(&self.registry.borrow(), &dirty);
        }

        self.sync_html_from_renderer();
        Ok(())
    }

    fn next_wake_ms(&self) -> i32 {
        let now = Instant::now();
        let Some(wake) = self.scheduler.borrow().next_wake_time() else {
            return -1;
        };
        let duration = wake.saturating_duration_since(now);
        let millis = duration.as_millis().min(i32::MAX as u128) as i32;
        millis
    }

    fn load_lua(&mut self, source: &str) -> Result<(), String> {
        self.run_script(source)?;
        self.flush_runtime()
    }

    fn dispatch_click(&mut self, id: i32) -> Result<(), String> {
        if id < 0 {
            return Ok(());
        }
        self.events.push(UiEvent::Click {
            node_id: NodeId::from_u32(id as u32),
        });
        self.flush_runtime()
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_error(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown runtime error".to_string();
    }

    let mut out = trimmed.replace("\r\n", "\n");
    if !out.starts_with("Lua error:") && !out.starts_with("Runtime error:") {
        out = format!("Lua error: {out}");
    }
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_init() -> *mut Runtime {
    match Runtime::new() {
        Ok(runtime) => Box::into_raw(Box::new(runtime)),
        Err(err) => {
            eprintln!("{}", err);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_load_lua(runtime: *mut Runtime, source: *const c_char) -> i32 {
    if runtime.is_null() || source.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    let source = unsafe { CStr::from_ptr(source) };
    let script = source.to_string_lossy();

    match runtime.load_lua(script.as_ref()) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_tick(runtime: *mut Runtime) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    match runtime.flush_runtime() {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_next_wake_ms(runtime: *mut Runtime) -> i32 {
    if runtime.is_null() {
        return -1;
    }

    let runtime = unsafe { &*runtime };
    runtime.next_wake_ms()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_pull_html(runtime: *mut Runtime) -> *const c_char {
    if runtime.is_null() {
        return std::ptr::null();
    }

    let runtime = unsafe { &*runtime };
    runtime.html.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_dispatch_click(runtime: *mut Runtime, id: i32) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    match runtime.dispatch_click(id) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_last_error(runtime: *mut Runtime) -> *const c_char {
    if runtime.is_null() {
        return std::ptr::null();
    }

    let runtime = unsafe { &*runtime };
    runtime.last_error_ptr()
}
