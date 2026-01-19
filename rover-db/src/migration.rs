use crate::connection::Connection;
use mlua::prelude::*;
use mlua::{Lua, LuaError, LuaFunction, LuaTable};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::connection::Connection;

/// Migration tracking table
const MIGRATIONS_TABLE: &str = "_rover_migrations";

/// Migration executor
pub struct MigrationExecutor {
    conn: Arc<Mutex<Connection>>,
    executor: Arc<Mutex<Connection>>,
}

impl MigrationExecutor {
    pub fn new(conn: Arc<Mutex<Connection>>, executor: Arc<Mutex<Connection>>) -> Self {
        Self { conn, executor }
    }

impl MigrationExecutor {
    pub fn new(conn: Arc<Mutex<Connection>>, executor: Arc<Mutex<Connection>>) -> Self {
        Self { conn, executor }
    }

impl MigrationExecutor {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Ensure migrations table exists
    pub fn ensure_migrations_table(&self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.blocking_lock();
        let check_sql = format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            MIGRATIONS_TABLE
        );

        let exists = conn
            .query(&check_sql)
            .map(|rows| !rows.is_empty())
            .map_err(|e| format!("Failed to check migrations table: {}", e))?;

        if !exists {
            let create_sql = format!(
                "CREATE TABLE {} (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT UNIQUE NOT NULL,
                    applied_at TEXT NOT NULL
                )",
                MIGRATIONS_TABLE
            );
            conn.execute(&create_sql)
                .map_err(|e| format!("Failed to create migrations table: {}", e))?;
            println!("✅ Created migrations table");
        }

        Ok(())
    }

    /// Get all applied migrations
    pub fn get_applied_migrations(&self) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
        let conn = self.conn.blocking_lock();
        let sql = format!("SELECT name FROM {} ORDER BY id ASC", MIGRATIONS_TABLE);

        let rows = conn
            .query(&sql)
            .map_err(|e| format!("Failed to get applied migrations: {}", e))?;

        let applied: BTreeSet<String> = rows
            .into_iter()
            .filter_map(|row| row.get(1).and_then(|v| v.as_str().map(|s| s.to_string())))
            .collect();

        Ok(applied)
    }

    /// Record a migration as applied
    pub fn record_migration(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.blocking_lock();
        let sql = format!(
            "INSERT INTO {} (name, applied_at) VALUES (?, datetime('now'))",
            MIGRATIONS_TABLE
        );

        conn.execute(&sql)
            .map_err(|e| format!("Failed to record migration: {}", e))?;

        println!("✅ Applied migration: {}", name);
        Ok(())
    }

    /// Remove a migration from applied list
    pub fn remove_migration(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.blocking_lock();
        let sql = format!("DELETE FROM {} WHERE name = ?", MIGRATIONS_TABLE);

        conn.execute(&sql)
            .map_err(|e| format!("Failed to remove migration: {}", e))?;

        println!("✅ Rolled back migration: {}", name);
        Ok(())
    }

    /// Generate reverse SQL for an operation
    fn generate_reverse(op: &MigrationOperation) -> Result<Vec<String>, String> {
        match op.r#type.as_str() {
            "create_table" => {
                let table = op.table.as_ref().unwrap_or(&String::new());
                Ok(vec![format!("DROP TABLE {}", table)])
            }
            "add_column" => {
                let table = op.table.as_ref().unwrap_or(&String::new());
                let col = op.column.as_ref().unwrap_or(&String::new());
                Ok(vec![format!("ALTER TABLE {} DROP COLUMN {}", table, col)])
            }
            "create_index" => Ok(vec![format!(
                "DROP INDEX {}",
                op.name.as_ref().unwrap_or(&String::new())
            )]),
            _ => Err(format!("Cannot auto-reverse operation: {}", op.r#type)),
        }
    }

    /// Execute SQL migration (handles SQLite limitations)
    pub fn execute_sql_migration(&self, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.blocking_lock();
        conn.execute(sql)
            .map_err(|e| format!("Migration failed: {}", e))?;
        Ok(())
    }
}

/// Recorded migration operation
#[derive(Debug, Clone)]
pub struct MigrationOperation {
    r#type: String,
    table: Option<String>,
    column: Option<String>,
    name: Option<String>,
    old_column: Option<String>,
    new_column: Option<String>,
    old_table: Option<String>,
    new_table: Option<String>,
    columns: Option<Vec<String>>,
    sql: Option<String>,
}

/// Load and execute a migration file
pub fn load_and_run_migration(
    lua: &Lua,
    executor: Arc<Mutex<Connection>>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read migration file: {}", e))?;

    // Extract filename for display
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("migration");

    // Load the migration and call change/up/down functions
    lua.load(&content)
        .set_name(filename)
        .eval::<()>()
        .map_err(|e| format!("Migration error: {}", e))?;

    Ok(())
}

/// Execute auto-reverse of operations
pub fn execute_auto_reverse(
    lua: &Lua,
    executor: Arc<Mutex<Connection>>,
    operations: Vec<MigrationOperation>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reversed_operations = Vec::new();

    for op in operations.into_iter().rev() {
        let reverse_sqls = MigrationExecutor::new(executor.clone()).generate_reverse(&op)?;

        let conn = executor.clone().blocking_lock();
        for sql in reverse_sqls {
            conn.execute(&sql)
                .map_err(|e| format!("Rollback failed: {}", e))?;
        }

        reversed_operations.push(op.clone());
    }

    Ok(())
}

/// Run all pending migrations
pub fn run_pending_migrations(
    lua: &Lua,
    executor: Arc<Mutex<Connection>>,
    migrations_dir: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let applied = MigrationExecutor::new(executor.clone()).get_applied_migrations()?;

    let mut pending: Vec<String> = Vec::new();

    // Load migration files
    if migrations_dir.exists() {
        let entries = fs::read_dir(migrations_dir)
            .map_err(|e| format!("Failed to read migrations directory: {}", e))?;

        for entry in entries {
            let path = entry.map_err(|e| format!("Failed to read entry: {}", e))?;

            if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Extract migration name (without .lua)
                let migration_name = filename.strip_suffix(".lua").unwrap_or(filename);

                if !applied.contains(migration_name) {
                    pending.push(migration_name.to_string());
                }
            }
        }
    }

    if pending.is_empty() {
        println!("✓ All migrations are up to date");
        return Ok(0);
    }

    // Sort pending by filename
    pending.sort();
    pending.dedup();

    println!("📋 {} pending migration(s):", pending.len());

    for migration_name in &pending {
        let path = migrations_dir.join(format!("{}.lua", migration_name));
        load_and_run_migration(lua, executor.clone(), &path)?;
    }

    Ok(pending.len())
}

/// Rollback last N migrations
pub fn rollback_migrations(
    lua: &Lua,
    executor: Arc<Mutex<Connection>>,
    migrations_dir: &Path,
    steps: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let executor_inst = MigrationExecutor::new(conn, executor);
    let applied = executor_inst.get_applied_migrations()?;

    if applied.is_empty() {
        println!("✓ No migrations to rollback");
        return Ok(0);
    }

    let mut to_rollback: Vec<String> = applied.into_iter().rev().take(steps).collect();

    println!("🔄 Rolling back {} migration(s):", to_rollback.len());

    for migration_name in &to_rollback {
        let path = migrations_dir.join(format!("{}.lua", migration_name));

        // Load migration to get operations
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read migration file: {}", e))?;

        // Call migration.change() to get recorded operations
        let operations: LuaTable = lua
            .load(&content)
            .set_name(migration_name)
            .call::<LuaTable>(LuaFunction::named("change"))?;

        // Extract operations array
        let ops_list: Vec<MigrationOperation> = operations
            .get("get_operations")
            .and_then(|f| f.call::<LuaTable>(LuaFunction::named("get_operations")))
            .map_err(|e| format!("Failed to get operations: {}", e))?;

        execute_auto_reverse(lua, executor.clone(), ops_list)?;

        executor_inst.remove_migration(migration_name)?;
    }

    Ok(to_rollback.len())
}
