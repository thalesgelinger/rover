use anyhow::{Context, Result};
use mlua::{Error, FromLua, Lua, Value};

pub fn run(path: &str) -> Result<()> {
    let lua = Lua::new();
    let content = std::fs::read_to_string(path)?;

    lua.load(&content)
        .set_name(path)
        .exec()
        .context("Failed to execute Lua script")
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
