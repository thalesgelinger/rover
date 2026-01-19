//! Rover DB - Intent-Based Query Builder & ORM DSL
//!
//! This module provides the Rust-side implementation for rover-db,
//! handling actual SQLite/libsql execution with coroutine support.

use mlua::prelude::*;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

mod connection;
mod executor;
mod file_analyzer;
pub mod intent_handler;
mod migration;
mod schema;
mod schema_analyzer;

pub use connection::Connection;
pub use executor::QueryExecutor;
pub use file_analyzer::{AnalysisResult, analyze_db_usage, analyze_file_with_db};
pub use intent_handler::{
    IntentComparison, TableDiff, TableStatus, compare_intent_with_schemas,
    generate_migration_content, generate_schema_content, prompt_yes_no,
    update_schema_file, write_migration_file, write_schema_file,
};
pub use migration::{
    MigrationExecutor, MigrationStatus, rollback_migrations, run_pending_migrations,
};
pub use schema::SchemaManager;
pub use schema_analyzer::{
    FieldDefinition, FieldType, ForeignKey, TableDefinition, generate_create_table, load_schemas,
    load_schemas_from_dir,
};

/// Create the rover.db module for Lua
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

    // Add migration DSL
    db.set("migration", migration_dsl)?;

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

/// Create the executor function for database operations
fn create_executor(lua: &Lua, db_path: String) -> LuaResult<LuaFunction> {
    // Create the actual connection
    let conn = Connection::new(&db_path)
        .map_err(|e| LuaError::RuntimeError(format!("Failed to connect to database: {}", e)))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = QueryExecutor::new(conn.clone());

    // Create a Lua userdata wrapper for the executor
    let executor = Arc::new(executor);

    lua.create_function(move |lua, args: LuaMultiValue| {
        // Extract operation type and SQL from args
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
                    "First argument must be operation type string".to_string(),
                ));
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

        match operation.as_str() {
            "query" | "select" => {
                let mode = if args_vec.len() > 2 {
                    match &args_vec[2] {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => "all".to_string(),
                    }
                } else {
                    "all".to_string()
                };
                executor.execute_query(lua, &sql, &mode)
            }
            "insert" => {
                let table_name = if args_vec.len() > 2 {
                    match &args_vec[2] {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };

                let data = if args_vec.len() > 3 {
                    args_vec[3].clone()
                } else {
                    LuaValue::Nil
                };

                executor.execute_insert(lua, &sql, &table_name, data)
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
        }
    })
}

/// Run migrations from a directory
pub fn run_migrations(db_path: &str, migrations_dir: &Path) -> Result<usize, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    run_pending_migrations(&executor, migrations_dir).map_err(|e| e.to_string())
}

/// Rollback migrations
pub fn rollback(db_path: &str, migrations_dir: &Path, steps: usize) -> Result<usize, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    rollback_migrations(&executor, migrations_dir, steps).map_err(|e| e.to_string())
}

/// Get migration status
pub fn migration_status(db_path: &str, migrations_dir: &Path) -> Result<MigrationStatus, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    executor
        .get_status(migrations_dir)
        .map_err(|e| e.to_string())
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

        // Check that schema DSL exists
        assert!(db.get::<LuaTable>("schema").is_ok());

        // Check that migration DSL exists
        assert!(db.get::<LuaTable>("migration").is_ok());
    }
}
