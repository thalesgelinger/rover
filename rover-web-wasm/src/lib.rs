use mlua::{Function, Lua, RegistryKey, Table, Value};
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};

const BOOTSTRAP: &str = r#"
rover = rover or {}
rover.ui = rover.ui or {}

local signal_mt = {
  __tostring = function(self)
    return tostring(self.val)
  end,
}

function rover.signal(initial)
  return setmetatable({ val = initial }, signal_mt)
end

function rover.derive(fn)
  return setmetatable({ __fn = fn }, {
    __tostring = function(self)
      return tostring(self.__fn())
    end,
  })
end

function rover.ui.text(value)
  if type(value) == 'table' and value[1] ~= nil then
    value = value[1]
  end
  return { __type = 'text', value = value }
end

function rover.ui.button(def)
  return { __type = 'button', label = def[1] or def.label or 'button', on_click = def.on_click }
end

function rover.ui.column(children)
  return { __type = 'column', children = children }
end

function rover.ui.row(children)
  return { __type = 'row', children = children }
end
"#;

struct Runtime {
    lua: Lua,
    html: CString,
    handlers: HashMap<i32, RegistryKey>,
    next_handler_id: i32,
}

impl Runtime {
    fn new() -> Self {
        Self {
            lua: Lua::new(),
            html: CString::new("<div></div>").expect("static html has no nul"),
            handlers: HashMap::new(),
            next_handler_id: 1,
        }
    }

    fn load_lua(&mut self, source: &str) -> mlua::Result<()> {
        self.lua.load(BOOTSTRAP).exec()?;
        self.lua.load(source).exec()
    }

    fn tick(&mut self) -> mlua::Result<()> {
        self.handlers.clear();
        self.next_handler_id = 1;

        let globals = self.lua.globals();
        let render_fn: Option<Function> = globals
            .get("rover")
            .ok()
            .and_then(|rover: Table| rover.get("render").ok());

        let Some(render_fn) = render_fn else {
            self.set_html("<div>missing rover.render</div>");
            return Ok(());
        };

        let tree: Value = render_fn.call(())?;
        let body = self.render_node(tree)?;
        self.set_html(&body);
        Ok(())
    }

    fn set_html(&mut self, html: &str) {
        let safe = html.replace('\0', "");
        self.html = CString::new(safe).expect("nul bytes removed");
    }

    fn render_node(&mut self, value: Value) -> mlua::Result<String> {
        let Value::Table(tbl) = value else {
            return Ok(format!(
                "<span>{}</span>",
                html_escape(&value_to_text(&self.lua, value)?)
            ));
        };

        let node_type: Option<String> = tbl.get("__type").ok();
        match node_type.as_deref() {
            Some("text") => {
                let text = value_to_text(&self.lua, tbl.get::<Value>("value")?)?;
                Ok(format!("<div>{}</div>", html_escape(&text)))
            }
            Some("button") => {
                let label = value_to_text(&self.lua, tbl.get::<Value>("label")?)?;
                let on_click: Option<Function> = tbl.get("on_click").ok();
                let attr = if let Some(handler) = on_click {
                    let id = self.next_handler_id;
                    self.next_handler_id += 1;
                    let key = self.lua.create_registry_value(handler)?;
                    self.handlers.insert(id, key);
                    format!(" data-rid=\"{}\"", id)
                } else {
                    String::new()
                };
                Ok(format!("<button{}>{}</button>", attr, html_escape(&label)))
            }
            Some("column") => {
                let children = render_children(self, tbl.get("children")?)?;
                Ok(format!(
                    "<div style=\"display:flex;flex-direction:column;gap:8px;\">{}</div>",
                    children
                ))
            }
            Some("row") => {
                let children = render_children(self, tbl.get("children")?)?;
                Ok(format!(
                    "<div style=\"display:flex;flex-direction:row;gap:8px;align-items:center;\">{}</div>",
                    children
                ))
            }
            _ => Ok("<div>unsupported node</div>".to_string()),
        }
    }

    fn dispatch_click(&self, id: i32) -> mlua::Result<()> {
        let Some(key) = self.handlers.get(&id) else {
            return Ok(());
        };
        let func: Function = self.lua.registry_value(key)?;
        func.call(())
    }
}

fn render_children(runtime: &mut Runtime, children: Value) -> mlua::Result<String> {
    let Value::Table(tbl) = children else {
        return Ok(String::new());
    };

    let mut out = String::new();
    for child in tbl.sequence_values::<Value>().flatten() {
        out.push_str(&runtime.render_node(child)?);
    }
    Ok(out)
}

fn value_to_text(lua: &Lua, value: Value) -> mlua::Result<String> {
    match value {
        Value::Nil => Ok(String::new()),
        Value::String(s) => Ok(s.to_string_lossy().to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Table(tbl) => {
            let tostring: Function = lua.globals().get("tostring")?;
            tostring.call::<String>(tbl)
        }
        other => {
            let tostring: Function = lua.globals().get("tostring")?;
            tostring.call::<String>(other)
        }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_init() -> *mut Runtime {
    Box::into_raw(Box::new(Runtime::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_load_lua(runtime: *mut Runtime, source: *const c_char) -> i32 {
    if runtime.is_null() || source.is_null() {
        return 1;
    }

    let runtime = unsafe { &mut *runtime };
    let source = unsafe { CStr::from_ptr(source) };
    let script = source.to_string_lossy();

    match runtime.load_lua(script.as_ref()) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
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
    match runtime.tick() {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
            2
        }
    }
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
    match runtime.dispatch_click(id) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
            2
        }
    }
}
