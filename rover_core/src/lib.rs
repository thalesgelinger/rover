use anyhow::{Context, Result};
use mlua::{Error, FromLua, Lua, Table, Value};

trait AutoTable {
    fn create_auto_table(&self) -> Result<Table>;
}
impl AutoTable for Lua {
    fn create_auto_table(&self) -> Result<Table> {
        let table = self.create_table()?;
        let metatable = self.create_table()?;

        metatable.set(
            "__index",
            self.create_function(|lua, (tbl, k): (Table, String)| {
                let is_sealed = tbl.raw_get::<bool>("__sealed").unwrap_or(false);
                if is_sealed {
                    return Err(Error::RuntimeError(format!("Unkown key {:?}", k)));
                } else {
                    let new_table = lua.create_auto_table()?;
                    tbl.raw_set(k, &new_table)?;
                    Ok(new_table)
                }
            })?,
        )?;

        metatable.set(
            "__newindex",
            self.create_function(|_, (tbl, k, v): (Table, String, Value)| tbl.raw_set(k, v))?,
        )?;

        let _ = table.set_metatable(Some(metatable));
        Ok(table)
    }
}

pub fn run(path: &str) -> Result<()> {
    let lua = Lua::new();
    let content = std::fs::read_to_string(path)?;

    let rover = lua.create_table()?;

    rover.set(
        "server",
        lua.create_function(|lua, opts: Table| {
            let server = lua.create_auto_table()?;
            Ok(server)
        })?,
    )?;

    let _ = lua.globals().set("rover", rover);

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
