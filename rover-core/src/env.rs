use anyhow::Result;
use mlua::{Lua, Table, Value};
use std::collections::HashMap;

/// Load .env file from current directory
pub fn load_dotenv() -> Result<HashMap<String, String>> {
    let mut env_vars = HashMap::new();

    // Try to load .env file from current directory
    match dotenvy::dotenv() {
        Ok(path) => {
            tracing::debug!("Loaded .env file from: {:?}", path);
        }
        Err(e) => {
            // It's ok if .env doesn't exist, just log it
            tracing::debug!("No .env file found: {}", e);
        }
    }

    // Collect all environment variables that were set (including from .env)
    for (key, value) in std::env::vars() {
        env_vars.insert(key, value);
    }

    Ok(env_vars)
}

/// Create the rover.env module for Lua
/// Access env vars directly: rover.env.MY_VAR (read-only)
pub fn create_env_module(lua: &Lua) -> Result<Table> {
    // Create the env table
    let env_module = lua.create_table()?;

    // Add __index metamethod for direct env var access (read-only)
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(
            |_lua, (_, key): (Table, String)| match std::env::var(&key) {
                Ok(value) => Ok(Value::String(_lua.create_string(&value)?)),
                Err(_) => Ok(Value::Nil),
            },
        )?,
    )?;

    // Add __newindex to make read-only (error on attempt to set)
    meta.set(
        "__newindex",
        lua.create_function(|_, (_, key): (Table, String)| {
            Err::<(), mlua::Error>(mlua::Error::RuntimeError(format!(
                "rover.env is read-only - cannot set '{}'",
                key
            )))
        })?,
    )?;

    env_module.set_metatable(Some(meta))?;

    Ok(env_module)
}

/// Create the rover.config module for Lua
pub fn create_config_module(lua: &Lua) -> Result<Table> {
    let config_module = lua.create_table()?;

    // rover.config.load(path) - Load a Lua config file and return as table
    config_module.set(
        "load",
        lua.create_function(|lua, path: String| {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Failed to load config file '{}': {}",
                        path, e
                    )));
                }
            };

            match lua.load(&content).set_name(&path).eval::<Value>() {
                Ok(value) => Ok(value),
                Err(e) => Err(mlua::Error::RuntimeError(format!(
                    "Failed to parse config file '{}': {}",
                    path, e
                ))),
            }
        })?,
    )?;

    // rover.config.from_env(prefix) - Load config from env vars with prefix
    config_module.set(
        "from_env",
        lua.create_function(|lua, prefix: String| {
            let table = lua.create_table()?;
            let prefix_upper = prefix.to_uppercase();

            for (key, value) in std::env::vars() {
                if key.starts_with(&prefix_upper) {
                    // Remove prefix and convert to nested table structure
                    let config_key = &key[prefix_upper.len()..];
                    let config_key = config_key.trim_start_matches('_');

                    // Split by underscore and create nested structure
                    let parts: Vec<&str> = config_key.split('_').collect();
                    if !parts.is_empty() {
                        let mut current = table.clone();
                        for (i, part) in parts.iter().enumerate() {
                            let part_lower = part.to_lowercase();
                            if i == parts.len() - 1 {
                                // Last part - set the value
                                current.set(part_lower, lua.create_string(&value)?)?;
                            } else {
                                // Create nested table if it doesn't exist
                                let next: Value = current.get(part_lower.clone())?;
                                let next_table = match next {
                                    Value::Table(t) => t,
                                    _ => {
                                        let t = lua.create_table()?;
                                        current.set(part_lower, t.clone())?;
                                        t
                                    }
                                };
                                current = next_table;
                            }
                        }
                    }
                }
            }

            Ok(Value::Table(table))
        })?,
    )?;

    Ok(config_module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_direct_access() {
        let lua = Lua::new();
        let env_module = create_env_module(&lua).unwrap();

        // Set a test env var
        unsafe { std::env::set_var("ROVER_TEST_VAR", "test_value") };

        // Test direct access via __index
        let result: Value = env_module.get("ROVER_TEST_VAR").unwrap();

        match result {
            Value::String(s) => {
                assert_eq!(s.to_str().unwrap(), "test_value");
            }
            _ => panic!("Expected string value, got {:?}", result),
        }

        // Clean up
        unsafe { std::env::remove_var("ROVER_TEST_VAR") };
    }

    #[test]
    fn test_env_missing_var_returns_nil() {
        let lua = Lua::new();
        let env_module = create_env_module(&lua).unwrap();

        // Access non-existent var should return nil
        let result: Value = env_module.get("ROVER_NONEXISTENT_VAR").unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn test_env_readonly() {
        let lua = Lua::new();
        let env_module = create_env_module(&lua).unwrap();

        // Attempting to set should error
        let result: Result<(), _> = env_module.set("ROVER_SET_TEST", "set_value");
        assert!(result.is_err());

        // Verify the error message mentions read-only
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("read-only"));
        }
    }

    #[test]
    fn test_config_load() {
        let lua = Lua::new();
        let config_module = create_config_module(&lua).unwrap();

        // Create a temp config file
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.lua");
        std::fs::write(&config_path, "return { name = 'test', value = 42 }").unwrap();

        let load_fn: mlua::Function = config_module.get("load").unwrap();
        let result: Value = load_fn.call(config_path.to_str().unwrap()).unwrap();

        match result {
            Value::Table(t) => {
                let name: String = t.get("name").unwrap();
                let value: i64 = t.get("value").unwrap();
                assert_eq!(name, "test");
                assert_eq!(value, 42);
            }
            _ => panic!("Expected table"),
        }
    }

    #[test]
    fn test_config_from_env() {
        let lua = Lua::new();
        let config_module = create_config_module(&lua).unwrap();

        // Set some test env vars
        unsafe {
            std::env::set_var("ROVER_CONFIG_DB_HOST", "localhost");
            std::env::set_var("ROVER_CONFIG_DB_PORT", "5432");
            std::env::set_var("ROVER_CONFIG_DEBUG", "true");
        }

        let from_env_fn: mlua::Function = config_module.get("from_env").unwrap();
        let result: Value = from_env_fn.call("ROVER_CONFIG").unwrap();

        match result {
            Value::Table(t) => {
                let db: Table = t.get("db").unwrap();
                let host: String = db.get("host").unwrap();
                let port: String = db.get("port").unwrap();
                let debug: String = t.get("debug").unwrap();

                assert_eq!(host, "localhost");
                assert_eq!(port, "5432");
                assert_eq!(debug, "true");
            }
            _ => panic!("Expected table"),
        }

        // Clean up
        unsafe {
            std::env::remove_var("ROVER_CONFIG_DB_HOST");
            std::env::remove_var("ROVER_CONFIG_DB_PORT");
            std::env::remove_var("ROVER_CONFIG_DEBUG");
        }
    }
}
