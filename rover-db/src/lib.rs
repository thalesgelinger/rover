//! Rover DB - Intent-Based Query Builder & ORM DSL
//!
//! This module provides the Rust-side implementation for rover-db,
//! handling actual SQLite/libsql execution with coroutine support.

use mlua::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

mod connection;
mod executor;
mod migration;
mod schema;

pub use connection::Connection;
pub use executor::QueryExecutor;
pub use migration::{MigrationExecutor, rollback_migrations, run_pending_migrations};
pub use schema::SchemaManager;

pub fn create_db_module(lua: &Lua) -> LuaResult<LuaTable> {
    // Load Lua DSL modules
    let db_lua: LuaTable = lua.load(include_str!("db.lua")).set_name("db.lua").eval()?;
    let schema_dsl: LuaTable = lua
        .load(include_str!("schema_dsl.lua"))
        .set_name("schema_dsl.lua")
        .eval()?;
    let migration_dsl: LuaTable = lua
        .load(include_str!("migration_dsl.lua"))
        .set_name("migration_dsl.lua")
        .eval()?;
    let analyzer: LuaTable = lua
        .load(include_str!("analyzer.lua"))
        .set_name("analyzer.lua")
        .eval()?;

    // Create main db table that will be returned
    let db = lua.create_table()?;

    // Copy aggregate functions from DSL
    for func_name in &["sum", "count", "avg", "min", "max"] {
        if let Ok(func) = db_lua.get::<LuaFunction>(*func_name) {
            db.set(*func_name, func)?;
        }
    }

    // Add schema DSL
    db.set("schema", schema_dsl)?;

    // Add migration DSL and expose executor
    db.set("migration", migration_dsl.clone())?;

    // Capture executor for use in closures
    let executor_for_closure = executor.clone();

    // Add migration functions to db module
    db.set(
        "run_pending_migrations",
        lua.create_function(move |lua, _: ()| {
            let migrations_dir = std::env::current_dir().join("db/migrations");
            run_pending_migrations(lua, executor_for_closure.clone(), &migrations_dir)
                .map_err(|e| LuaError::external(e))
        }),
    )?;

    db.set(
        "rollback_migrations",
        lua.create_function(move |lua, steps: Option<i64>| {
            let steps = steps.unwrap_or(1) as usize;
            let migrations_dir = std::env::current_dir().join("db/migrations");
            rollback_migrations(lua, executor_for_closure.clone(), &migrations_dir, steps)
                .map_err(|e| LuaError::external(e))
        }),
    )?;

    // Store reference to the DSL modules for creating instances
    let db_lua_ref = lua.create_registry_value(db_lua)?;

    // Create the connect function
    let connect_fn = lua.create_function(move |lua, config: Option<LuaTable>| {
        let db_lua: LuaTable = lua.registry_value(&db_lua_ref)?;
        let connect_fn: LuaFunction = db_lua.get("connect")?;

        // Call Lua's connect to get the instance
        let instance: LuaTable = connect_fn.call(config.clone())?;

        // Extract connection path from config
        let db_path = if let Some(ref cfg) = config {
            cfg.get::<String>("path")
                .unwrap_or_else(|_| "rover.sqlite".to_string())
        } else {
            "rover.sqlite".to_string()
        };

        // Create executor function that will handle all DB operations
        let executor = create_executor(lua, db_path)?;
        instance.set("_executor", executor)?;

        Ok(instance)
    })?;

    db.set("connect", connect_fn)?;

    Ok(db)
}
