mod app_type;
mod auto_table;
pub mod cookie;
mod env;
mod error_reporter;
pub mod guard;
pub mod html;
pub mod http;
pub mod io;
pub mod middleware;
pub mod permissions;
pub mod security;
pub mod server;
pub mod session;
mod surface;
pub mod template;
pub mod ws_client;

use env::load_dotenv;
use permissions::PermissionsConfig;
use rover_ui::platform::{
    DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH, UiRuntimeConfig, UiTarget, ViewportSignals,
};
use rover_ui::scheduler::{Scheduler, SharedScheduler};
use rover_ui::signal::SignalValue;
use rover_ui::{SharedSignalRuntime, SignalRuntime, ui::UiRegistry};
use server::Server;
use std::cell::RefCell;

use anyhow::Result;
use mlua::{AnyUserData, Error, FromLua, Function, Lua, Table, Value};
use std::rc::Rc;

use crate::app_type::AppType;

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

struct BootSource<'a> {
    source: &'a str,
    source_name: &'a str,
    argv0: &'a str,
}

struct RuntimeBootstrap {
    lua: Lua,
}

impl RuntimeBootstrap {
    fn new(args: &[String], source: &BootSource<'_>) -> Result<Self> {
        let lua = Lua::new();
        set_lua_args(&lua, args, source.argv0)?;
        initialize_runtime_app_data(&lua)?;

        let rover = surface::install_rover_surface(&lua)?;
        lua.globals().set("rover", rover)?;
        let _ = lua.load("_G.migration = rover.db.migration").eval::<()>();

        Ok(Self { lua })
    }

    fn execute(self, source: &BootSource<'_>, verbose: bool) -> Result<()> {
        let app = evaluate_app(&self.lua, source.source, source.source_name, verbose)?;
        dispatch_app(&self.lua, app, source.source)
    }
}

fn set_lua_args(lua: &Lua, args: &[String], argv0: &str) -> mlua::Result<()> {
    let arg_table = lua.create_table()?;
    arg_table.set(0, argv0)?;
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i + 1, arg.as_str())?;
    }
    arg_table.set(-1, "rover")?;
    lua.globals().set("arg", arg_table)
}

fn initialize_runtime_app_data(lua: &Lua) -> mlua::Result<()> {
    let runtime: SharedSignalRuntime = Rc::new(SignalRuntime::new());
    lua.set_app_data(runtime.clone());

    let ui_registry = Rc::new(RefCell::new(UiRegistry::new()));
    lua.set_app_data(ui_registry);

    let scheduler: SharedScheduler = Rc::new(RefCell::new(Scheduler::new()));
    lua.set_app_data(scheduler);

    let runtime_config = UiRuntimeConfig::new(UiTarget::Tui);
    lua.set_app_data(runtime_config);

    let permissions_config = PermissionsConfig::new();
    lua.set_app_data(permissions_config);

    let viewport_signals = ViewportSignals {
        width: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_WIDTH as i64)),
        height: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_HEIGHT as i64)),
    };
    lua.set_app_data(viewport_signals);

    Ok(())
}

fn evaluate_app(lua: &Lua, source: &str, source_name: &str, verbose: bool) -> Result<Value> {
    match lua.load(source).set_name(source_name).eval() {
        Ok(app) => Ok(app),
        Err(err) => {
            let error_str = err.to_string();
            let (error_info, stack_trace) =
                error_reporter::parse_lua_error(&error_str, source_name);

            if verbose {
                error_reporter::display_error_with_stack(&error_info, stack_trace.as_deref());
            } else {
                error_reporter::display_error(&error_info);
            }

            Err(err.into())
        }
    }
}

fn dispatch_app(lua: &Lua, app: Value, source: &str) -> Result<()> {
    match app {
        Value::Table(table) => {
            if let Some(app_type) = table.app_type() {
                match app_type {
                    AppType::Server => table.run_server(lua, source)?,
                }
            }

            Ok(())
        }
        _ => try_mount_ui(lua),
    }
}

fn try_mount_ui(lua: &Lua) -> Result<()> {
    let rover_table = lua.globals().get::<Table>("rover")?;
    if let Ok(ui_ud) = rover_table.get::<AnyUserData>("ui") {
        if let Ok(user_value) = ui_ud.user_value::<Table>() {
            if let Ok(render_fn) = user_value.get::<Function>("render") {
                match render_fn.call::<Value>(()) {
                    Ok(Value::UserData(node_ud)) => {
                        if let Ok(node) = node_ud.borrow::<rover_ui::ui::lua_node::LuaNode>() {
                            let registry_rc = lua
                                .app_data_ref::<Rc<RefCell<UiRegistry>>>()
                                .expect("UiRegistry not found");
                            registry_rc.borrow_mut().set_root(node.id());
                            println!("UI mounted with root node {:?}", node.id());
                        }
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("Error in rover.ui.render(): {}", e),
                }
            }
        }
    }

    Ok(())
}

pub fn run(path: &str, args: &[String], verbose: bool) -> Result<()> {
    let _ = load_dotenv()?;
    let content = std::fs::read_to_string(path)?;
    let source = BootSource {
        source: &content,
        source_name: path,
        argv0: path,
    };
    let runtime = RuntimeBootstrap::new(args, &source)?;
    runtime.execute(&source, verbose)
}

/// Register extra rover modules (http, html, db, io, debug, guard, env, config) on an existing Lua instance
/// This is useful when using a custom renderer with rover_ui::App
pub fn register_extra_modules(lua: &Lua) -> Result<()> {
    let rover: Table = lua.globals().get("rover")?;
    surface::install_extra_modules(lua, &rover)?;
    Ok(())
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

/// Run Lua code from a string (used by bundled applications)
pub fn run_from_str(source: &str, args: &[String], verbose: bool) -> Result<()> {
    let _ = load_dotenv()?;
    let boot_source = BootSource {
        source,
        source_name: "bundle",
        argv0: "bundle",
    };
    let runtime = RuntimeBootstrap::new(args, &boot_source)?;
    runtime.execute(&boot_source, verbose)
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
    use tempfile::NamedTempFile;

    #[test]
    fn should_read_and_print_lua_file() {
        let result = run("../examples/starter.lua", &[], false);
        assert_eq!(result.unwrap(), ());
    }

    #[test]
    fn should_get_config_as_rust_struct() {
        let result = get_config();
        assert!(result.is_ok());
    }

    #[test]
    fn should_run_file_and_bundle_with_same_runtime_modules() {
        let script = r#"
            assert(rover ~= nil)
            assert(rover.db ~= nil)
            assert(rover.http ~= nil)
            assert(rover.session ~= nil)
            return {}
        "#;

        run_from_str(script, &[], false).unwrap();

        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), script).unwrap();
        let path = file.path().to_string_lossy().to_string();
        run(&path, &[], false).unwrap();
    }

    #[test]
    fn should_bootstrap_runtime_seam_and_execute_script() {
        let source = BootSource {
            source: "assert(rover ~= nil); assert(rover.db ~= nil); return {}",
            source_name: "bundle",
            argv0: "bundle",
        };
        let runtime = RuntimeBootstrap::new(&[], &source).unwrap();
        runtime.execute(&source, false).unwrap();
    }

    #[test]
    fn should_install_runtime_root_and_server_surface() {
        let lua = Lua::new();
        initialize_runtime_app_data(&lua).unwrap();

        let rover = surface::install_rover_surface(&lua).unwrap();
        lua.globals().set("rover", rover.clone()).unwrap();

        for key in [
            "server",
            "guard",
            "db",
            "http",
            "html",
            "session",
            "auth",
            "config",
            "env",
            "cookie",
            "ws_client",
            "ui",
        ] {
            let value = rover.get::<Value>(key).unwrap();
            assert!(
                !matches!(value, Value::Nil),
                "expected rover.{} to be installed",
                key
            );
        }

        let script = r#"
            local api = rover.server {}
            assert(api.json)
            assert(api.text)
            assert(api.html)
            assert(api.redirect)
            assert(api.error)
            assert(api.no_content)
            assert(api.raw)
            assert(api.stream)
            assert(api.stream_with_headers)
            assert(api.sse)
            assert(api.idempotent)
            return true
        "#;
        let result: bool = lua.load(script).eval().unwrap();
        assert!(result);
    }
}
