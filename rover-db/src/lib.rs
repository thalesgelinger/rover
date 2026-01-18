//! Rover DB - Intent-Based Query Builder & ORM DSL
//!
//! This module provides the Rust-side implementation for rover-db,
//! handling actual SQLite/libsql execution with coroutine support.

use mlua::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

mod connection;
mod executor;
mod schema;

pub use connection::Connection;
pub use executor::QueryExecutor;
pub use schema::SchemaManager;

/// Thread-local connection pool for database connections
thread_local! {
    static CONNECTION_POOL: RefCell<HashMap<String, Arc<Mutex<Connection>>>> =
        RefCell::new(HashMap::new());
}

/// Check if we should yield for I/O (running in a coroutine)
fn should_yield_for_io(lua: &Lua) -> LuaResult<bool> {
    let globals = lua.globals();
    if let Ok(coroutine) = globals.get::<LuaTable>("coroutine") {
        if let Ok(running) = coroutine.get::<LuaFunction>("running") {
            if let Ok((thread, is_main)) = running.call::<(LuaValue, bool)>(()) {
                return Ok(!is_main && matches!(thread, LuaValue::Thread(_)));
            }
        }
    }
    Ok(false)
}

/// Yield to the event loop (simulates async behavior)
fn yield_to_event_loop(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    if let Ok(coroutine) = globals.get::<LuaTable>("coroutine") {
        if let Ok(yield_fn) = coroutine.get::<LuaFunction>("yield") {
            let _ = yield_fn.call::<()>(());
        }
    }
    Ok(())
}

/// Create the rover.db module
pub fn create_db_module(lua: &Lua) -> LuaResult<LuaTable> {
    // Load the Lua DSL module
    let db_lua: LuaTable = lua
        .load(include_str!("db.lua"))
        .set_name("db.lua")
        .eval()?;

    // Create the main db table that will be returned
    let db = lua.create_table()?;

    // Copy aggregate functions from DSL
    for func_name in &["sum", "count", "avg", "min", "max"] {
        if let Ok(func) = db_lua.get::<LuaFunction>(*func_name) {
            db.set(*func_name, func)?;
        }
    }

    // Store reference to the DSL module for creating instances
    let db_lua_ref = lua.create_registry_value(db_lua)?;

    // Create the connect function
    let connect_fn = lua.create_function(move |lua, config: Option<LuaTable>| {
        let db_lua: LuaTable = lua.registry_value(&db_lua_ref)?;
        let connect_fn: LuaFunction = db_lua.get("connect")?;

        // Call Lua's connect to get the instance
        let instance: LuaTable = connect_fn.call(config.clone())?;

        // Extract connection path from config
        let db_path = if let Some(ref cfg) = config {
            cfg.get::<String>("path").unwrap_or_else(|_| ":memory:".to_string())
        } else {
            ":memory:".to_string()
        };

        // Create executor function that will handle all DB operations
        let executor = create_executor(lua, db_path)?;
        instance.set("_executor", executor)?;

        Ok(instance)
    })?;

    db.set("connect", connect_fn)?;

    Ok(db)
}

/// Create the executor function that bridges Lua queries to actual DB operations
fn create_executor(lua: &Lua, db_path: String) -> LuaResult<LuaFunction> {
    // Initialize connection
    let conn = Connection::new(&db_path).map_err(|e| LuaError::external(e))?;
    let conn = Arc::new(Mutex::new(conn));

    // Store in thread-local pool
    let path_clone = db_path.clone();
    CONNECTION_POOL.with(|pool| {
        pool.borrow_mut().insert(path_clone, conn.clone());
    });

    let executor = QueryExecutor::new(conn);
    let executor = Arc::new(executor);

    lua.create_function(move |lua, args: LuaMultiValue| {
        let args_vec: Vec<LuaValue> = args.into_iter().collect();

        if args_vec.is_empty() {
            return Err(LuaError::RuntimeError(
                "Executor requires at least one argument".to_string(),
            ));
        }

        let operation = match &args_vec[0] {
            LuaValue::String(s) => s.to_str()?.to_string(),
            _ => {
                return Err(LuaError::RuntimeError(
                    "First argument must be operation type".to_string(),
                ))
            }
        };

        let sql = if args_vec.len() > 1 {
            match &args_vec[1] {
                LuaValue::String(s) => s.to_str()?.to_string(),
                _ => String::new(),
            }
        } else {
            String::new()
        };

        // Check if we should yield for async behavior
        let should_yield = should_yield_for_io(lua)?;

        // Execute the operation
        let result = match operation.as_str() {
            "insert" => {
                let table_name = if args_vec.len() > 3 {
                    match &args_vec[3] {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };

                let data = if args_vec.len() > 2 {
                    args_vec[2].clone()
                } else {
                    LuaValue::Nil
                };

                executor.execute_insert(lua, &sql, &table_name, data)
            }
            "query" => {
                let mode = if args_vec.len() > 3 {
                    match &args_vec[3] {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => "all".to_string(),
                    }
                } else {
                    "all".to_string()
                };

                executor.execute_query(lua, &sql, &mode)
            }
            "update" => executor.execute_update(&sql),
            "delete" => executor.execute_delete(&sql),
            "raw" => {
                let params = if args_vec.len() > 2 {
                    match &args_vec[2] {
                        LuaValue::Table(t) => Some(t.clone()),
                        _ => None,
                    }
                } else {
                    None
                };
                executor.execute_raw(lua, &sql, params)
            }
            _ => Err(LuaError::RuntimeError(format!(
                "Unknown operation: {}",
                operation
            ))),
        };

        // Yield if running in coroutine
        if should_yield {
            yield_to_event_loop(lua)?;
        }

        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_db_module() {
        let lua = Lua::new();
        let db = create_db_module(&lua).unwrap();

        // Check that connect function exists
        assert!(db.get::<LuaFunction>("connect").is_ok());

        // Check that aggregate functions exist
        assert!(db.get::<LuaFunction>("sum").is_ok());
        assert!(db.get::<LuaFunction>("count").is_ok());
        assert!(db.get::<LuaFunction>("avg").is_ok());
        assert!(db.get::<LuaFunction>("min").is_ok());
        assert!(db.get::<LuaFunction>("max").is_ok());
    }

    #[test]
    fn test_connect_memory() {
        let lua = Lua::new();
        let db = create_db_module(&lua).unwrap();
        lua.globals().set("db_module", db).unwrap();

        let result: LuaTable = lua
            .load(
                r#"
            local db = db_module.connect()
            return db
        "#,
            )
            .eval()
            .unwrap();

        // Should have _executor
        assert!(result.get::<LuaFunction>("_executor").is_ok());
    }
}
