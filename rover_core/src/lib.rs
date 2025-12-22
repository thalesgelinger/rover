mod app_type;
mod auto_table;
mod guard;
mod inspect;
mod server;
use guard::BodyValue;
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

    // Create Lua-side guard with chainable validators
    let guard: Table = lua.load(r#"
        local Guard = {}
        
        -- Helper to create chainable validator
        local function create_validator(validator_type)
            return {
                type = validator_type,
                required = false,
                required_msg = nil,
                default = nil,
                enum = nil,
                element = nil,
                schema = nil,
                
                required = function(self, msg)
                    self.required = true
                    self.required_msg = msg
                    return self
                end,
                
                default = function(self, value)
                    self.default = value
                    return self
                end,
                
                enum = function(self, values)
                    self.enum = values
                    return self
                end
            }
        end
        
        function Guard:string()
            return create_validator("string")
        end
        
        function Guard:number()
            return create_validator("number")
        end
        
        function Guard:integer()
            return create_validator("integer")
        end
        
        function Guard:boolean()
            return create_validator("boolean")
        end
        
        function Guard:array(element_validator)
            local v = create_validator("array")
            v.element = element_validator
            return v
        end
        
        function Guard:object(schema)
            local v = create_validator("object")
            v.schema = schema
            return v
        end
        
        return Guard
    "#).eval()?;
    
    // Add __call metamethod for rover.guard(data, schema)
    let guard_meta = lua.create_table()?;
    guard_meta.set("__index", guard.clone())?;
    guard_meta.set(
        "__call",
        lua.create_function(|lua, (_self, data, schema): (Value, Table, Table)| {
            use crate::guard::validate_table;
            validate_table(lua, &data, &schema, "")
        })?,
    )?;
    
    let _ = guard.set_metatable(Some(guard_meta));
    
    // Add hidden __body_value for BodyValue constructor
    guard.set(
        "__body_value",
        lua.create_function(|_lua, json_string: String| Ok(BodyValue::new(json_string)))?,
    )?;
    
    rover.set("guard", guard)?;

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
                    AppType::Server => table.run_server(&lua)?,
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
        let result = run("examples/hello.lua");
        assert_eq!(result.unwrap(), ());
    }

    #[test]
    fn should_get_config_as_rust_struct() {
        let result = get_config();
        assert_eq!(result.unwrap().name, "rover");
    }
}
