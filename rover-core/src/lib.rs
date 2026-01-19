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
use server::{AppServer, Server};

use anyhow::Result;
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

pub fn run(path: &str, verbose: bool) -> Result<()> {
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

    // Create/extend debug global with print function
    let debug_print_code = r#"
        local debug = debug or {}
        debug.print = function(value, label)
            -- Format value with indentation
            local function format_val(v, depth, seen)
                depth = depth or 0
                seen = seen or {}
                
                if depth > 5 then
                    return "<max_depth>"
                end
                
                local t = type(v)
                if t == "nil" then
                    return "nil"
                elseif t == "boolean" then
                    return tostring(v)
                elseif t == "number" then
                    return tostring(v)
                elseif t == "string" then
                    return '"' .. v:gsub('"', '\\"') .. '"'
                elseif t == "table" then
                    -- Check for circular reference
                    if seen[v] then
                        return "<circular>"
                    end
                    seen[v] = true
                    
                    local indent = string.rep("  ", depth)
                    local next_indent = string.rep("  ", depth + 1)
                    local lines = {"{"}
                    
                    -- Check if array-like
                    local is_array = true
                    local max_idx = 0
                    for k in pairs(v) do
                        if type(k) == "number" then
                            if k > 0 then max_idx = math.max(max_idx, k) end
                        else
                            is_array = false
                        end
                    end
                    
                    if is_array and max_idx > 0 then
                        -- Array format
                        for i = 1, max_idx do
                            if v[i] ~= nil then
                                table.insert(lines, next_indent .. format_val(v[i], depth + 1, seen) .. ",")
                            end
                        end
                    else
                        -- Key-value format
                        for k, val in pairs(v) do
                            local key_str = type(k) == "string" and k or tostring(k)
                            table.insert(lines, next_indent .. key_str .. " = " .. format_val(val, depth + 1, seen) .. ",")
                        end
                    end
                    
                    table.insert(lines, indent .. "}")
                    return table.concat(lines, "\n")
                else
                    return "<" .. t .. ">"
                end
            end
            
            local formatted = format_val(value)
            local output
            if label then
                output = string.format("[debug.print] %s: %s", label, formatted)
            else
                output = string.format("[debug.print] %s", formatted)
            end
            
            print(output)
            return value
        end
        
        return debug
    "#;

    let debug_module: Table = lua.load(debug_print_code).eval()?;
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
        _ => Ok(()),
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
        let result = run("../examples/starter.lua", false);
        assert_eq!(result.unwrap(), ());
    }

    #[test]
    fn should_get_config_as_rust_struct() {
        let result = get_config();
        assert!(result.is_ok());
    }
}
