mod app_type;
mod auto_table;
mod guard;
pub mod html;
mod http;
mod inspect;
mod io;
mod server;
pub mod template;

use guard::BodyValue;
use html::create_html_module;
use server::{AppServer, Server};

use anyhow::{Context, Result};
use mlua::{Error, FromLua, Lua, Table, Value};

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

pub fn run(path: &str) -> Result<()> {
    let lua = Lua::new();
    let content = std::fs::read_to_string(path)?;

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

    // Add HTTP client module
    let http_module = http::create_http_module(&lua)?;
    rover.set("http", http_module)?;

    // Add rover.html global templating function
    let html_module = create_html_module(&lua)?;
    rover.set("html", html_module)?;

    let _ = lua.globals().set("rover", rover);

    let app: Value = lua
        .load(&content)
        .set_name(path)
        .eval()
        .context("Failed to execute Lua script")?;

    match app {
        Value::Table(table) => {
            if let Some(app_type) = table.app_type() {
                match app_type {
                    AppType::Server => table.run_server(&lua, &content)?,
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[derive(Debug)]
pub struct Config {
    name: String,
}

impl FromLua for Config {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Table(table) => Ok(Config {
                name: table.get("name")?,
            }),
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
    let config: Config = lua.load(&content).eval()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_read_and_print_lua_file() {
        let result = run("../examples/starter.lua");
        assert_eq!(result.unwrap(), ());
    }

    #[test]
    fn should_get_config_as_rust_struct() {
        let result = get_config();
        assert_eq!(result.unwrap().name, "rover");
    }
}
