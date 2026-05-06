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
    generate_migration_content, generate_schema_content, prompt_yes_no, update_schema_file,
    write_migration_file, write_schema_file,
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

    // Inject schema_dsl into db_lua for schema-aware query methods
    let set_schema_dsl: LuaFunction = db_lua.get("_set_schema_dsl")?;
    set_schema_dsl.call::<()>(schema_dsl.clone())?;

    let db = lua.create_table()?;
    db.set("schema", schema_dsl)?;
    db.set("migration", migration_dsl)?;

    // Expose aggregate functions from db.lua (sum, count, avg, min, max)
    if let Ok(sum_fn) = db_lua.get::<LuaFunction>("sum") {
        db.set("sum", sum_fn)?;
    }
    if let Ok(count_fn) = db_lua.get::<LuaFunction>("count") {
        db.set("count", count_fn)?;
    }
    if let Ok(avg_fn) = db_lua.get::<LuaFunction>("avg") {
        db.set("avg", avg_fn)?;
    }
    if let Ok(min_fn) = db_lua.get::<LuaFunction>("min") {
        db.set("min", min_fn)?;
    }
    if let Ok(max_fn) = db_lua.get::<LuaFunction>("max") {
        db.set("max", max_fn)?;
    }

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

enum ExecutorCommand {
    Query { sql: String, mode: String },
    Insert {
        sql: String,
        table_name: String,
        data: LuaValue,
    },
    Update { sql: String },
    Delete { sql: String },
    Raw {
        sql: String,
        params: Option<LuaTable>,
    },
}

fn string_arg(args: &[LuaValue], idx: usize) -> LuaResult<String> {
    match args.get(idx) {
        Some(LuaValue::String(s)) => Ok(s.to_str()?.to_string()),
        _ => Ok(String::new()),
    }
}

fn parse_executor_command(args: &[LuaValue]) -> LuaResult<ExecutorCommand> {
    if args.is_empty() {
        return Err(LuaError::RuntimeError(
            "Executor requires at least one argument".to_string(),
        ));
    }

    let operation = match &args[0] {
        LuaValue::String(s) => s.to_str()?.to_string(),
        _ => {
            return Err(LuaError::RuntimeError(
                "First argument must be operation type string".to_string(),
            ));
        }
    };

    let sql = string_arg(args, 1)?;

    match operation.as_str() {
        "query" | "select" => {
            let mode = match args.get(2) {
                Some(LuaValue::String(s)) => s.to_str()?.to_string(),
                _ => "all".to_string(),
            };
            Ok(ExecutorCommand::Query { sql, mode })
        }
        "insert" => {
            let table_name = string_arg(args, 2)?;
            let data = args.get(3).cloned().unwrap_or(LuaValue::Nil);
            Ok(ExecutorCommand::Insert {
                sql,
                table_name,
                data,
            })
        }
        "update" => Ok(ExecutorCommand::Update { sql }),
        "delete" => Ok(ExecutorCommand::Delete { sql }),
        "raw" => {
            let params = match args.get(2) {
                Some(LuaValue::Table(t)) => Some(t.clone()),
                _ => None,
            };
            Ok(ExecutorCommand::Raw { sql, params })
        }
        _ => Err(LuaError::RuntimeError(format!(
            "Unknown operation: {}",
            operation
        ))),
    }
}

fn execute_command(
    lua: &Lua,
    executor: &Arc<QueryExecutor>,
    command: ExecutorCommand,
) -> LuaResult<LuaValue> {
    match command {
        ExecutorCommand::Query { sql, mode } => executor.execute_query(lua, &sql, &mode),
        ExecutorCommand::Insert {
            sql,
            table_name,
            data,
        } => executor.execute_insert(lua, &sql, &table_name, data),
        ExecutorCommand::Update { sql } => executor.execute_update(&sql),
        ExecutorCommand::Delete { sql } => executor.execute_delete(&sql),
        ExecutorCommand::Raw { sql, params } => executor.execute_raw(lua, &sql, params),
    }
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
        let args_vec: Vec<LuaValue> = args.into_iter().collect();
        let command = parse_executor_command(&args_vec)?;
        execute_command(lua, &executor, command)
    })
}

/// Run migrations from a directory
pub fn run_migrations(db_path: &str, migrations_dir: &Path) -> Result<usize, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    executor
        .ensure_migrations_table()
        .map_err(|e| e.to_string())?;

    run_pending_migrations(&executor, migrations_dir).map_err(|e| e.to_string())
}

/// Rollback migrations
pub fn rollback(db_path: &str, migrations_dir: &Path, steps: usize) -> Result<usize, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    executor
        .ensure_migrations_table()
        .map_err(|e| e.to_string())?;

    rollback_migrations(&executor, migrations_dir, steps).map_err(|e| e.to_string())
}

/// Get migration status
pub fn migration_status(db_path: &str, migrations_dir: &Path) -> Result<MigrationStatus, String> {
    let conn = Connection::new(db_path).map_err(|e| format!("Failed to connect: {}", e))?;

    let conn = Arc::new(Mutex::new(conn));
    let executor = MigrationExecutor::new(conn);

    executor
        .ensure_migrations_table()
        .map_err(|e| e.to_string())?;

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
