mod app_type;
mod auto_table;
mod guard;
mod http;
mod inspect;
mod io;
mod server;
pub mod template;
pub mod event_loop;

use guard::BodyValue;
use server::{AppServer, Server};

use anyhow::{Context, Result};
use mlua::{Error, FromLua, Lua, Table, Value};

use crate::app_type::AppType;

/// Create the rover.html module with templating support and component system
fn create_html_module(lua: &Lua) -> mlua::Result<Table> {
    let html_module = lua.create_table()?;

    // Create metatable with __call for rover.html(data) [=[ template ]=]
    let html_meta = lua.create_table()?;

    // rover.html(data) returns a template builder
    html_meta.set(
        "__call",
        lua.create_function(|lua, (html_table, data): (Table, Value)| {
            // Create a template builder that stores data and html_table reference
            let builder = lua.create_table()?;
            builder.set("__data", data)?;
            builder.set("__html", html_table)?;

            // Create metatable with __call for: builder [=[ template ]=]
            let builder_meta = lua.create_table()?;
            builder_meta.set(
                "__call",
                lua.create_function(|lua, (builder, template): (Table, String)| {
                    let data: Value = builder.get("__data")?;
                    let html_table: Table = builder.get("__html")?;

                    // Create environment with data and component functions
                    let data_table = match data {
                        Value::Table(t) => t,
                        Value::Nil => lua.create_table()?,
                        _ => {
                            return Err(mlua::Error::RuntimeError(
                                "rover.html() data must be a table or nil".to_string(),
                            ));
                        }
                    };

                    // Render template with component functions available
                    render_template_with_components(lua, &template, &data_table, &html_table)
                })?,
            )?;
            let _ = builder.set_metatable(Some(builder_meta));

            Ok(builder)
        })?,
    )?;

    // __index allows reading component functions from html_module
    html_meta.set("__index", html_module.clone())?;

    // __newindex allows adding component functions to html_module
    html_meta.set(
        "__newindex",
        lua.create_function(|_lua, (table, key, value): (Table, Value, Value)| {
            table.raw_set(key, value)?;
            Ok(())
        })?,
    )?;

    let _ = html_module.set_metatable(Some(html_meta));

    Ok(html_module)
}

/// Render template with component functions available in the environment
fn render_template_with_components(
    lua: &Lua,
    template: &str,
    data: &Table,
    html_table: &Table,
) -> mlua::Result<String> {
    // Parse and generate Lua code
    let segments = crate::template::parse_template(template);
    let lua_code = crate::template::generate_lua_code(&segments);

    // Create environment with data as base
    let env = lua.create_table()?;

    // Copy standard library functions
    let globals = lua.globals();
    for name in &[
        "tostring", "tonumber", "ipairs", "pairs", "table", "string", "math", "type", "next",
        "select", "unpack", "pcall", "error", "rawget", "rawset", "setmetatable", "getmetatable",
    ] {
        if let Ok(val) = globals.get::<Value>(*name) {
            env.set(*name, val)?;
        }
    }

    // Copy all data fields into environment
    for pair in data.pairs::<Value, Value>() {
        let (key, value) = pair?;
        env.set(key, value)?;
    }

    // Add rover.html reference for nested component calls
    if let Ok(rover) = globals.get::<Table>("rover") {
        env.set("rover", rover)?;
    }

    // Add component functions directly to environment
    for pair in html_table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        // Skip internal fields
        if let Value::String(ref s) = key {
            if s.to_str().map(|s| s.starts_with("__")).unwrap_or(false) {
                continue;
            }
        }
        env.set(key, value)?;
    }

    // Execute the generated code
    let chunk = lua.load(&lua_code).set_environment(env);
    chunk.eval().map_err(|e| {
        mlua::Error::RuntimeError(format!(
            "Template rendering failed: {}\nGenerated code:\n{}",
            e, lua_code
        ))
    })
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

    // Add hidden __body_value for BodyValue constructor
    guard.set(
        "__body_value",
        lua.create_function(|_lua, json_string: String| Ok(BodyValue::new(json_string)))?,
    )?;

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
