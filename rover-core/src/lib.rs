mod app_type;
mod auto_table;
mod error_reporter;
mod guard;
pub mod html;
mod http;
mod io;
mod server;
pub mod template;

use html::create_html_module;
use rover_db::create_db_module;
use rover_ui::{SharedSignalRuntime, SignalRuntime, register_ui_module};
use server::{AppServer, Server};

use anyhow::Result;
use mlua::{Error, FromLua, Lua, Table, Value};
use std::rc::Rc;

use crate::app_type::AppType;
use rover_ui::platform::tui::{PlatformEvent, PlatformHandler, TuiPlatform};
use rover_ui::signal::SignalId;
use rover_ui::signal::SignalValue;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Platform {
    Http,
    Tui,
    Web,
    Native,
}

pub struct GenericEventLoop {
    platform: Box<dyn PlatformHandler>,
    key_bindings: HashMap<String, SignalId>,
    lua: mlua::Lua,
}

impl GenericEventLoop {
    pub fn new(platform: Box<dyn PlatformHandler>, lua: mlua::Lua) -> Self {
        Self {
            platform,
            key_bindings: HashMap::new(),
            lua,
        }
    }

    pub fn bind_key(&mut self, key: String, signal: SignalId) {
        self.key_bindings.insert(key, signal);
    }

    pub fn unbind_key(&mut self, key: &str) {
        self.key_bindings.remove(key);
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        self.platform.init()?;

        loop {
            match self
                .platform
                .wait_for_event(std::time::Duration::from_millis(16))?
            {
                Some(PlatformEvent::Quit) => break,
                Some(PlatformEvent::KeyDown { key, .. }) => {
                    if let Some(signal_id) = self.key_bindings.get(&key) {
                        if let Some(runtime) =
                            self.lua.app_data_ref::<rover_ui::SharedSignalRuntime>()
                        {
                            runtime
                                .set_signal(*signal_id, rover_ui::signal::SignalValue::Bool(true));
                        }
                    }
                }
                Some(PlatformEvent::Tick { .. }) => {
                    self.platform.render()?;
                }
                _ => {}
            }
        }

        self.platform.cleanup()?;
        Ok(())
    }
}

trait RoverApp {
    fn app_type(&self) -> Option<AppType>;
}

impl RoverApp for Table {
    fn app_type(&self) -> Option<AppType> {
        match self.get("__rover_app_type") {
            Ok(Value::Integer(t)) => AppType::from_i64(t),
            _ => None,
        }
    }
}

pub fn run(path: &str, verbose: bool, platform: Option<&str>) -> Result<()> {
    let lua = Lua::new();
    let content = std::fs::read_to_string(path)?;

    // Initialize signal runtime (interior mutability now handled by runtime itself)
    let runtime: SharedSignalRuntime = Rc::new(SignalRuntime::new());
    lua.set_app_data(runtime);

    let rover = lua.create_table()?;

    rover.set(
        "server",
        lua.create_function(|lua, opts: Table| {
            let server = lua.create_server(opts)?;
            Ok(server)
        })?,
    )?;

    // Load guard from embedded Lua file
    let guard: Table = lua
        .load(include_str!("guard.lua"))
        .set_name("guard.lua")
        .eval()?;

    // Add __call metamethod for rover.guard(data, schema)
    let guard_meta = lua.create_table()?;
    guard_meta.set("__index", guard.clone())?;
    guard_meta.set(
        "__call",
        lua.create_function(|lua, (data, schema): (Value, Value)| {
            use crate::guard::{ValidationErrors, validate_table};

            // Extract the table from data
            let data_table = match data {
                Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "First argument must be a table".to_string(),
                    ));
                }
            };

            // Extract the table from schema
            let schema_table = match schema {
                Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "Second argument must be a table".to_string(),
                    ));
                }
            };

            match validate_table(lua, &data_table, &schema_table, "") {
                Ok(validated) => Ok(validated),
                Err(errors) => {
                    // Return ValidationErrors that formats nicely when converted to string
                    let validation_errors = ValidationErrors::new(errors);
                    Err(mlua::Error::ExternalError(std::sync::Arc::new(
                        validation_errors,
                    )))
                }
            }
        })?,
    )?;

    let _ = guard.set_metatable(Some(guard_meta));

    rover.set("guard", guard)?;

    // Override global io module with async version
    let io_module = io::create_io_module(&lua)?;
    lua.globals().set("io", io_module)?;

    // Load debug module from embedded Lua file
    let debug_module: Table = lua
        .load(include_str!("debug.lua"))
        .set_name("debug.lua")
        .eval()?;
    lua.globals().set("debug", debug_module)?;

    // Add HTTP client module
    let http_module = http::create_http_module(&lua)?;
    rover.set("http", http_module)?;

    // Add rover.html global templating function
    let html_module = create_html_module(&lua)?;
    rover.set("html", html_module)?;

    // Add rover.db database module
    let db_module = create_db_module(&lua)?;
    rover.set("db", db_module)?;

    // Register UI module (signals, effects, derive)
    register_ui_module(&lua, &rover)?;

    let _ = lua.globals().set("rover", rover);

    // Make migration global via Lua (accessing rover.db.migration)
    let _ = lua.load("_G.migration = rover.db.migration").eval::<()>();

    let app: Value = match lua.load(&content).set_name(path).eval() {
        Ok(app) => app,
        Err(err) => {
            let error_str = err.to_string();
            let (error_info, stack_trace) = error_reporter::parse_lua_error(&error_str, path);

            if verbose {
                error_reporter::display_error_with_stack(&error_info, stack_trace.as_deref());
            } else {
                error_reporter::display_error(&error_info);
            }

            return Err(err.into());
        }
    };

    match app {
        Value::Table(table) => {
            if let Some(app_type) = table.app_type() {
                match app_type {
                    AppType::Server => table.run_server(&lua, &content)?,
                }
            }
            Ok(())
        }
        Value::UserData(ud) => {
            let result = ud.borrow::<rover_ui::lua::node::LuaNode>();
            match result {
                Ok(root_node) => {
                    let platform_type = match platform {
                        Some("http") => Platform::Http,
                        Some("tui") => Platform::Tui,
                        Some("web") => Platform::Web,
                        Some("native") => Platform::Native,
                        _ => Platform::Tui,
                    };

                    match platform_type {
                        Platform::Tui => {
                            let runtime = lua
                                .app_data_ref::<rover_ui::SharedSignalRuntime>()
                                .unwrap()
                                .clone();
                            let platform =
                                rover_ui::platform::tui::TuiPlatform::new(root_node.id, runtime)?;
                            let mut event_loop = GenericEventLoop::new(Box::new(platform), lua);
                            event_loop.run()?;
                        }
                        _ => unimplemented!("Platform {:?} not yet implemented", platform_type),
                    }
                    Ok(())
                }
                Err(_) => Ok(()),
            }
        }
        other => {
            eprintln!("Got value: {:?}", other);
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct Config;

impl FromLua for Config {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Table(_table) => Ok(Config),
            _ => Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Config".into(),
                message: Some("expected table".to_string()),
            }),
        }
    }
}

pub fn get_config() -> Result<Config> {
    let lua = Lua::new();
    let content = std::fs::read_to_string("rover.lua")?;
    let _config: Config = lua.load(&content).eval()?;
    Ok(Config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_read_and_print_lua_file() {
        let result = run("../examples/starter.lua", false, None);
        assert_eq!(result.unwrap(), ());
    }

    #[test]
    fn should_get_config_as_rust_struct() {
        let result = get_config();
        assert!(result.is_ok());
    }
}
