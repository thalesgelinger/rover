use mlua::{Lua, Value};
use rover_ui::coroutine::{CoroutineResult, run_coroutine_with_delay};
use rover_ui::events::{EventQueue, UiEvent};
use rover_ui::lua::register_ui_module;
use rover_ui::platform::{
    DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH, UiRuntimeConfig, UiTarget, ViewportSignals,
};
use rover_ui::scheduler::{Scheduler, SharedScheduler};
use rover_ui::signal::{SignalRuntime, SignalValue};
use rover_ui::ui::lua_node::LuaNode;
use rover_ui::ui::node::{NodeId, TextContent, UiNode};
use rover_ui::ui::registry::UiRegistry;
use rover_ui::ui::renderer::Renderer;
use rover_ui::ui::{NodeStyle, PositionType, StyleOp, StyleSize};
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};
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
            UiNode::Text { content } => format!(
                "<div{}>{}</div>",
                style_attr(registry, node_id, &[]),
                html_escape(content.value())
            ),
            UiNode::Button { label, .. } => {
                let rid = node_id.index() as u32;
                format!(
                    "<button data-rid=\"{}\"{}>{}</button>",
                    rid,
                    style_attr(registry, node_id, &[]),
                    html_escape(label)
                )
            }
            UiNode::Input { value, .. } => {
                let rid = node_id.index() as u32;
                format!(
                    "<input data-rid=\"{}\" value=\"{}\"{} />",
                    rid,
                    html_escape(value.value()),
                    style_attr(registry, node_id, &[])
                )
            }
            UiNode::Checkbox { checked, .. } => {
                let rid = node_id.index() as u32;
                let checked_attr = if *checked { " checked" } else { "" };
                format!(
                    "<input type=\"checkbox\" data-rid=\"{}\"{}{} />",
                    rid,
                    checked_attr,
                    style_attr(registry, node_id, &[])
                )
            }
            UiNode::Column { children } => {
                let body = self.render_children(registry, children);
                format!(
                    "<div{}>{}</div>",
                    style_attr(
                        registry,
                        node_id,
                        &["display:flex", "flex-direction:column", "gap:8px"]
                    ),
                    body
                )
            }
            UiNode::Row { children } => {
                let body = self.render_children(registry, children);
                format!(
                    "<div{}>{}</div>",
                    style_attr(
                        registry,
                        node_id,
                        &[
                            "display:flex",
                            "flex-direction:row",
                            "gap:8px",
                            "align-items:center",
                        ]
                    ),
                    body
                )
            }
            UiNode::View { children }
            | UiNode::List { children, .. }
            | UiNode::ScrollView { children }
            | UiNode::MacosWindow { children, .. } => {
                let body = self.render_children(registry, children);
                format!("<div{}>{}</div>", style_attr(registry, node_id, &[]), body)
            }
            UiNode::Stack { children } => {
                let body = self.render_children(registry, children);
                format!(
                    "<div{}>{}</div>",
                    style_attr(registry, node_id, &["position:relative"]),
                    body
                )
            }
            UiNode::ScrollBox { child, .. }
            | UiNode::FullScreen { child, .. }
            | UiNode::KeyArea { child, .. }
            | UiNode::Conditional { child, .. } => child
                .map(|id| self.render_node(registry, id))
                .unwrap_or_default(),
            UiNode::Image { src } => format!(
                "<img src=\"{}\"{} />",
                html_escape(src),
                style_attr(registry, node_id, &[])
            ),
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

fn style_attr(registry: &UiRegistry, node_id: NodeId, defaults: &[&str]) -> String {
    let mut css = defaults
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();

    if let Some(style) = registry.get_node_style(node_id) {
        push_style_css(&mut css, style);
    }

    if css.is_empty() {
        return String::new();
    }

    format!(" style=\"{}\"", html_escape(&css.join(";")))
}

fn push_style_css(css: &mut Vec<String>, style: &NodeStyle) {
    for op in &style.ops {
        match op {
            StyleOp::Padding(value) => css.push(format!("padding:{value}px")),
            StyleOp::BgColor(value) => css.push(format!("background-color:{value}")),
            StyleOp::BorderColor(value) => css.push(format!("border-color:{value}")),
            StyleOp::BorderWidth(value) => {
                css.push("border-style:solid".to_string());
                css.push(format!("border-width:{value}px"));
            }
        }
    }

    if let Some(color) = &style.color {
        css.push(format!("color:{color}"));
    }

    if let Some(width) = style.width {
        css.push(format!("width:{}", css_size(width)));
    }

    if let Some(height) = style.height {
        css.push(format!("height:{}", css_size(height)));
    }

    match style.position {
        PositionType::Relative => {}
        PositionType::Absolute => css.push("position:absolute".to_string()),
        PositionType::Fixed => css.push("position:fixed".to_string()),
    }

    push_px(css, "top", style.top);
    push_px(css, "left", style.left);
    push_px(css, "right", style.right);
    push_px(css, "bottom", style.bottom);

    if let Some(grow) = style.grow {
        css.push(format!("flex-grow:{grow}"));
    }

    if let Some(gap) = style.gap {
        css.push(format!("gap:{gap}px"));
    }

    if let Some(value) = style.horizontal.as_deref() {
        css.push(format!("justify-content:{}", css_alignment(value)));
    }

    if let Some(value) = style.vertical.as_deref() {
        css.push(format!("align-items:{}", css_alignment(value)));
    }
}

fn css_size(size: StyleSize) -> String {
    match size {
        StyleSize::Full => "100%".to_string(),
        StyleSize::Px(value) => format!("{value}px"),
    }
}

fn push_px(css: &mut Vec<String>, name: &str, value: Option<i32>) {
    if let Some(value) = value {
        css.push(format!("{name}:{value}px"));
    }
}

fn css_alignment(value: &str) -> &str {
    match value {
        "start" => "flex-start",
        "end" => "flex-end",
        other => other,
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
    viewport: ViewportSignals,
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
            viewport: viewport_signals,
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

        let Some(render_fn) = render_fn else {
            return Err(
                "web target needs `function rover.render()`; server/runtime modules not mounted in wasm yet"
                    .to_string(),
            );
        };

        let root_node: LuaNode = render_fn.call(()).map_err(|e| e.to_string())?;
        self.registry.borrow_mut().set_root(root_node.id());

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
            let node_id = event.node_id();

            if let UiEvent::Change { value, .. } = &event {
                let signal_id = {
                    let registry = self.registry.borrow();
                    match registry.get_node(node_id) {
                        Some(UiNode::Input {
                            value: text_content,
                            ..
                        }) => text_content.signal_id(),
                        _ => None,
                    }
                };

                if let Some(signal_id) = signal_id {
                    self.signal_runtime.set_signal(
                        &self.lua,
                        signal_id,
                        SignalValue::String(value.clone().into()),
                    );
                }
            }

            let effect_id = {
                let registry = self.registry.borrow();
                let Some(node) = registry.get_node(node_id) else {
                    continue;
                };

                match (&event, node) {
                    (UiEvent::Click { .. }, UiNode::Button { on_click, .. }) => *on_click,
                    (UiEvent::Change { .. }, UiNode::Input { on_change, .. }) => *on_change,
                    (UiEvent::Submit { .. }, UiNode::Input { on_submit, .. }) => *on_submit,
                    (UiEvent::Toggle { .. }, UiNode::Checkbox { on_toggle, .. }) => *on_toggle,
                    _ => None,
                }
            };

            if let Some(effect_id) = effect_id {
                let args = match &event {
                    UiEvent::Click { .. } => Value::Nil,
                    UiEvent::Change { value, .. } | UiEvent::Submit { value, .. } => {
                        Value::String(self.lua.create_string(value).map_err(|e| e.to_string())?)
                    }
                    UiEvent::Toggle { checked, .. } => Value::Boolean(*checked),
                    UiEvent::Key { .. } => Value::Nil,
                };

                self.signal_runtime
                    .call_effect(&self.lua, effect_id, args)
                    .map_err(|e| e.to_string())?;
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

    fn dispatch_input(&mut self, id: i32, value: &str) -> Result<(), String> {
        if id < 0 {
            return Ok(());
        }

        self.events.push(UiEvent::Change {
            node_id: NodeId::from_u32(id as u32),
            value: value.to_string(),
        });
        self.flush_runtime()
    }

    fn dispatch_submit(&mut self, id: i32, value: &str) -> Result<(), String> {
        if id < 0 {
            return Ok(());
        }

        self.events.push(UiEvent::Submit {
            node_id: NodeId::from_u32(id as u32),
            value: value.to_string(),
        });
        self.flush_runtime()
    }

    fn dispatch_toggle(&mut self, id: i32, checked: bool) -> Result<(), String> {
        if id < 0 {
            return Ok(());
        }

        {
            let mut registry = self.registry.borrow_mut();
            if let Some(UiNode::Checkbox {
                checked: current_checked,
                ..
            }) = registry.get_node_mut(NodeId::from_u32(id as u32))
            {
                *current_checked = checked;
                registry.mark_dirty(NodeId::from_u32(id as u32));
            }
        }

        self.events.push(UiEvent::Toggle {
            node_id: NodeId::from_u32(id as u32),
            checked,
        });
        self.flush_runtime()
    }

    fn set_viewport(&mut self, width: i32, height: i32) -> Result<(), String> {
        let width = width.max(1) as i64;
        let height = height.max(1) as i64;

        self.signal_runtime
            .set_signal(&self.lua, self.viewport.width, SignalValue::Int(width));
        self.signal_runtime
            .set_signal(&self.lua, self.viewport.height, SignalValue::Int(height));
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
pub unsafe extern "C" fn rover_web_dispatch_input(
    runtime: *mut Runtime,
    id: i32,
    value: *const c_char,
) -> i32 {
    if runtime.is_null() || value.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    let value = unsafe { CStr::from_ptr(value) };
    let value = value.to_string_lossy();
    match runtime.dispatch_input(id, value.as_ref()) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_dispatch_submit(
    runtime: *mut Runtime,
    id: i32,
    value: *const c_char,
) -> i32 {
    if runtime.is_null() || value.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    let value = unsafe { CStr::from_ptr(value) };
    let value = value.to_string_lossy();
    match runtime.dispatch_submit(id, value.as_ref()) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_dispatch_toggle(
    runtime: *mut Runtime,
    id: i32,
    checked: i32,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    match runtime.dispatch_toggle(id, checked != 0) {
        Ok(_) => 0,
        Err(err) => {
            runtime.set_error(err.as_str());
            eprintln!("{}", runtime.last_error.to_string_lossy());
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_set_viewport(
    runtime: *mut Runtime,
    width: i32,
    height: i32,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    runtime.clear_error();
    match runtime.set_viewport(width, height) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_modifier_styles_on_nodes() {
        let mut registry = UiRegistry::new();
        let text = registry.create_node(UiNode::Text {
            content: TextContent::Static("hello".to_string()),
        });
        let root = registry.create_node(UiNode::View {
            children: vec![text],
        });
        registry.set_root(root);
        registry.set_node_style(
            root,
            NodeStyle {
                ops: vec![
                    StyleOp::Padding(12),
                    StyleOp::BgColor("#101010".to_string()),
                ],
                width: Some(StyleSize::Full),
                ..NodeStyle::default()
            },
        );

        let html = WebRenderer::new().render_node(&registry, root);
        assert!(html.contains("padding:12px"));
        assert!(html.contains("background-color:#101010"));
        assert!(html.contains("width:100%"));
    }

    #[test]
    fn row_mod_overrides_default_gap_and_alignment() {
        let mut registry = UiRegistry::new();
        let text = registry.create_node(UiNode::Text {
            content: TextContent::Static("hello".to_string()),
        });
        let root = registry.create_node(UiNode::Row {
            children: vec![text],
        });
        registry.set_root(root);
        registry.set_node_style(
            root,
            NodeStyle {
                gap: Some(24),
                vertical: Some("start".to_string()),
                ..NodeStyle::default()
            },
        );

        let html = WebRenderer::new().render_node(&registry, root);
        assert!(html.contains("gap:24px"));
        assert!(html.contains("align-items:flex-start"));
    }
}
