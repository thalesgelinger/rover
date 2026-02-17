use crate::connection::Connection;
use mlua::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Migration tracking table
const MIGRATIONS_TABLE: &str = "_rover_migrations";

/// Migration status
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub applied: BTreeSet<String>,
    pub available: Vec<String>,
    pub pending: Vec<String>,
}

/// Migration executor handles running and tracking migrations
pub struct MigrationExecutor {
    conn: Arc<Mutex<Connection>>,
}

impl MigrationExecutor {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn ensure_migrations_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    pub fn get_applied_migrations(
        &self,
    ) -> Result<BTreeSet<String>, Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.blocking_lock();
        let sql = format!("SELECT name FROM {} ORDER BY id ASC", MIGRATIONS_TABLE);

        let rows = conn
            .query(&sql)
            .map_err(|e| format!("Failed to get applied migrations: {}", e))?;

        let applied: BTreeSet<String> = rows
            .into_iter()
            .filter_map(|row| {
                row.iter()
                    .find(|(col_name, _)| col_name == "name")
                    .and_then(|(_, v)| match v {
                        libsql::Value::Text(s) => Some(s.to_string()),
                        _ => None,
                    })
            })
            .collect();

        Ok(applied)
    }

    pub fn get_status(
        &self,
        migrations_dir: &Path,
    ) -> Result<MigrationStatus, Box<dyn std::error::Error + Send + Sync>> {
        let applied = self.get_applied_migrations()?;

        let mut available = Vec::new();
        if migrations_dir.exists() {
            let entries = fs::read_dir(migrations_dir)
                .map_err(|e| format!("Failed to read migrations directory: {}", e))?;

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        available.push(name.to_string());
                    }
                }
            }
        }

        available.sort();

        let pending: Vec<String> = available
            .iter()
            .filter(|name| !applied.contains(*name))
            .cloned()
            .collect();

        Ok(MigrationStatus {
            applied,
            available,
            pending,
        })
    }

    pub fn record_migration(
        &self,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.blocking_lock();
        let sql = format!(
            "INSERT INTO {} (name, applied_at) VALUES ('{}', datetime('now'))",
            MIGRATIONS_TABLE, name
        );

        conn.execute(&sql)
            .map_err(|e| format!("Failed to record migration: {}", e))?;

        Ok(())
    }

    pub fn remove_migration(
        &self,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.conn.blocking_lock();
        let sql = format!("DELETE FROM {} WHERE name = '{}'", MIGRATIONS_TABLE, name);

        conn.execute(&sql)
            .map_err(|e| format!("Failed to remove migration: {}", e))?;

        Ok(())
    }
}

/// Recorded migration operation (used in tests + future rollback enhancements)
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MigrationOperation {
    pub r#type: String,
    pub table: Option<String>,
    pub column: Option<String>,
    pub column_type: Option<String>,
    pub name: Option<String>,
    pub old_column: Option<String>,
    pub new_column: Option<String>,
    pub old_table: Option<String>,
    pub new_table: Option<String>,
    pub columns: Option<Vec<String>>,
    pub sql: Option<String>,
}

/// Setup Lua VM with migration DSL and guard types
fn setup_migration_lua(
    lua: &Lua,
    is_rollback: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guard_lua = include_str!("../../rover-core/src/guard.lua");
    let guard: LuaTable = lua.load(guard_lua).set_name("guard.lua").eval()?;

    // Extend guard with DB-specific modifiers (primary, auto, unique, references, index)
    // First, make guard available as a global for the extend chunk
    let globals = lua.globals();
    globals.set("_guard_to_extend", guard.clone())?;

    let extend_code = r#"
        local db_modifiers = {
            primary = function(self) self._primary = true; return self end,
            auto = function(self) self._auto = true; return self end,
            unique = function(self) self._unique = true; return self end,
            references = function(self, table, column)
                self._references = {table = table, column = column or "id"}
                return self
            end,
            index = function(self) self._index = true; return self end,
        }
        return _guard_to_extend:extend(db_modifiers)
    "#;
    let db_guard: LuaTable = lua.load(extend_code).set_name("guard_extend.lua").eval()?;

    let migration_dsl = include_str!("migration_dsl.lua");
    let migration_table: LuaTable = lua
        .load(migration_dsl)
        .set_name("migration_dsl.lua")
        .eval()?;

    let globals = lua.globals();

    let rover = lua.create_table()?;
    let db = lua.create_table()?;

    // Set up rover.guard (base guard) and rover.db.guard (extended with DB modifiers)
    rover.set("guard", guard)?;
    db.set("guard", db_guard)?;
    rover.set("db", db)?;

    globals.set("rover", rover)?;
    globals.set("migration", migration_table)?;
    globals.set("_rollback_mode", is_rollback)?;

    Ok(())
}

/// Execute migration Lua code and return SQL statements
fn execute_migration_lua(
    lua: &Lua,
    content: &str,
    _name: &str,
    is_rollback: bool,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let globals = lua.globals();
    let migration: LuaTable = globals.get("migration")?;

    // Clear any previous operations
    let clear_fn: LuaFunction = migration.get("clear_operations")?;
    clear_fn.call::<()>(())?;

    // Load the migration file (defines functions but doesn't execute them)
    lua.load(content).set_name(_name).exec()?;

    // Check which functions are defined
    let has_change = globals.get::<LuaFunction>("change").is_ok();
    let has_up = globals.get::<LuaFunction>("up").is_ok();
    let has_down = globals.get::<LuaFunction>("down").is_ok();

    // Validate: cannot have both change() and up()/down()
    if has_change && (has_up || has_down) {
        return Err("migration cannot define both change() and up()/down(). Use either change() OR up()/down(), not both.".into());
    }

    // Validate: must have at least one migration function
    if !has_change && !has_up && !has_down {
        return Err("migration must define change() or up()/down() functions.".into());
    }

    // Call the appropriate function
    if has_change {
        let change_fn: LuaFunction = globals.get("change")?;
        change_fn.call::<()>(())?;
    } else if is_rollback && has_down {
        let down_fn: LuaFunction = globals.get("down")?;
        down_fn.call::<()>(())?;
    } else if !is_rollback && has_up {
        let up_fn: LuaFunction = globals.get("up")?;
        up_fn.call::<()>(())?;
    }

    // Get recorded operations
    let operations: LuaTable = migration
        .get::<LuaFunction>("get_operations")?
        .call::<LuaTable>(())?;

    // Check invertibility for change() migrations
    if has_change && !is_rollback {
        let check_invertible: LuaFunction = migration.get::<LuaFunction>("is_invertible")?;
        let invertible: bool = check_invertible.call(operations.clone())?;
        if !invertible {
            return Err("migration change() contains non-invertible operations (raw SQL). Use up() and down() functions instead.".into());
        }
    }

    // Collect operations into a vec for potential reversal
    let mut ops: Vec<LuaTable> = Vec::new();
    for pair in operations.pairs::<i64, LuaTable>() {
        let (_, op) = pair?;
        ops.push(op);
    }

    // If rolling back a change() migration, reverse the operations
    if has_change && is_rollback {
        ops = reverse_lua_operations(lua, &ops)?;
    }

    // Convert operations to SQL
    let mut sql_statements = Vec::new();
    for op in &ops {
        if let Some(sql) = operation_to_sql(op)? {
            sql_statements.push(sql);
        }
    }

    // Error if no operations were generated
    if sql_statements.is_empty() {
        return Err("migration generated zero SQL operations. Ensure your migration defines actual database changes.".into());
    }

    Ok(sql_statements)
}

/// Reverse operations for change() rollback
fn reverse_lua_operations(
    lua: &Lua,
    ops: &[LuaTable],
) -> Result<Vec<LuaTable>, Box<dyn std::error::Error + Send + Sync>> {
    let mut reversed = Vec::new();

    // Process in reverse order
    for op in ops.iter().rev() {
        let op_type: String = op.get("type")?;
        let reversed_op = lua.create_table()?;

        match op_type.as_str() {
            "create_table" => {
                reversed_op.set("type", "drop_table")?;
                reversed_op.set("table", op.get::<String>("table")?)?;
            }
            "drop_table" => {
                // Can't auto-reverse drop_table without schema info
                // This shouldn't happen in change() - it's not invertible
                return Err("Cannot auto-reverse drop_table in change()".into());
            }
            "add_column" => {
                reversed_op.set("type", "remove_column")?;
                reversed_op.set("table", op.get::<String>("table")?)?;
                reversed_op.set("column", op.get::<String>("column")?)?;
            }
            "remove_column" => {
                // Can't auto-reverse remove_column without type info
                return Err("Cannot auto-reverse remove_column in change()".into());
            }
            "rename_column" => {
                reversed_op.set("type", "rename_column")?;
                reversed_op.set("table", op.get::<String>("table")?)?;
                reversed_op.set("old_column", op.get::<String>("new_column")?)?;
                reversed_op.set("new_column", op.get::<String>("old_column")?)?;
            }
            "create_index" => {
                reversed_op.set("type", "drop_index")?;
                reversed_op.set("table", op.get::<String>("table")?)?;
                reversed_op.set("index", op.get::<String>("index")?)?;
            }
            "drop_index" => {
                // Can't auto-reverse drop_index without column info
                return Err("Cannot auto-reverse drop_index in change()".into());
            }
            "rename_table" => {
                reversed_op.set("type", "rename_table")?;
                reversed_op.set("table", op.get::<String>("new_table")?)?;
                reversed_op.set("new_table", op.get::<String>("table")?)?;
            }
            "raw" => {
                return Err("Cannot auto-reverse raw SQL in change()".into());
            }
            _ => {
                // Unknown op type, skip
                continue;
            }
        }

        reversed.push(reversed_op);
    }

    Ok(reversed)
}

/// Convert a migration operation to SQL
fn operation_to_sql(
    op: &LuaTable,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let op_type: String = op.get("type")?;

    match op_type.as_str() {
        "create_table" => {
            let table: String = op.get("table")?;
            let definition: LuaTable = op.get("definition")?;

            let mut columns = Vec::new();
            let mut constraints = Vec::new();
            let mut has_id_column = false;

            for pair in definition.pairs::<String, LuaValue>() {
                let (col_name, col_def) = pair?;

                // Check if user defined an 'id' column
                if col_name == "id" {
                    has_id_column = true;
                }

                let col_sql = parse_column_definition(&col_name, &col_def)?;
                columns.push(col_sql);

                // Check for explicit references via guard:references("table")
                if let LuaValue::Table(t) = &col_def {
                    if let Ok(ref_table) = t.get::<String>("_references_table") {
                        constraints.push(format!(
                            "FOREIGN KEY ({}) REFERENCES {}(id)",
                            col_name, ref_table
                        ));
                    }
                }
            }

            // Only auto-add id column if user didn't define one
            if !has_id_column {
                columns.insert(0, "id INTEGER PRIMARY KEY AUTOINCREMENT".to_string());
            }

            columns.sort_by(|a, b| {
                if a.contains("PRIMARY KEY") {
                    std::cmp::Ordering::Less
                } else if b.contains("PRIMARY KEY") {
                    std::cmp::Ordering::Greater
                } else {
                    a.cmp(b)
                }
            });

            columns.extend(constraints);

            Ok(Some(format!(
                "CREATE TABLE IF NOT EXISTS {} (\n  {}\n)",
                table,
                columns.join(",\n  ")
            )))
        }
        "drop_table" => {
            let table: String = op.get("table")?;
            Ok(Some(format!("DROP TABLE IF EXISTS {}", table)))
        }
        "add_column" => {
            let table: String = op.get("table")?;
            let column: String = op.get("column")?;
            let column_type: LuaValue = op.get("column_type")?;
            let col_sql = parse_column_definition(&column, &column_type)?;
            Ok(Some(format!(
                "ALTER TABLE {} ADD COLUMN {}",
                table, col_sql
            )))
        }
        "remove_column" => {
            let table: String = op.get("table")?;
            let column: String = op.get("column")?;
            Ok(Some(format!(
                "ALTER TABLE {} DROP COLUMN {}",
                table, column
            )))
        }
        "rename_column" => {
            let table: String = op.get("table")?;
            let old_column: String = op.get("old_column")?;
            let new_column: String = op.get("new_column")?;

            Ok(Some(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table, old_column, new_column
            )))
        }
        "create_index" => {
            let table: String = op.get("table")?;
            let index: String = op.get("index")?;
            let columns: LuaTable = op.get("columns")?;

            let mut col_list = Vec::new();
            for pair in columns.pairs::<i64, String>() {
                let (_, col) = pair?;
                col_list.push(col);
            }

            Ok(Some(format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
                index,
                table,
                col_list.join(", ")
            )))
        }
        "drop_index" => {
            let index: String = op.get("index")?;
            Ok(Some(format!("DROP INDEX IF EXISTS {}", index)))
        }
        "rename_table" => {
            let table: String = op.get("table")?;
            let new_table: String = op.get("new_table")?;
            Ok(Some(format!(
                "ALTER TABLE {} RENAME TO {}",
                table, new_table
            )))
        }
        "raw" => {
            let sql: String = op.get("sql")?;
            Ok(Some(sql))
        }
        _ => Ok(None),
    }
}

/// Parse column definition from Lua guard type
fn parse_column_definition(
    name: &str,
    value: &LuaValue,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut col_type = "TEXT".to_string();
    let mut modifiers = Vec::new();

    match value {
        LuaValue::Table(t) => {
            // "type" is the type field (not a method, so no underscore prefix)
            let type_str = t.get::<String>("type").ok();

            if let Some(type_str) = type_str {
                col_type = match type_str.as_str() {
                    "integer" | "int" => "INTEGER".to_string(),
                    "real" | "float" | "number" => "REAL".to_string(),
                    "boolean" | "bool" => "INTEGER".to_string(),
                    "blob" => "BLOB".to_string(),
                    _ => "TEXT".to_string(),
                };
            }

            // Use underscore-prefixed fields (set by methods like :primary())
            let is_primary = t.get::<bool>("_primary").unwrap_or(false);
            if is_primary {
                modifiers.push("PRIMARY KEY");
            }

            let is_auto = t.get::<bool>("_auto").unwrap_or(false);
            if is_auto && col_type == "INTEGER" {
                modifiers.push("AUTOINCREMENT");
            }

            let is_unique = t.get::<bool>("_unique").unwrap_or(false);
            if is_unique {
                modifiers.push("UNIQUE");
            }

            let is_required = t.get::<bool>("_required").unwrap_or(false);
            if is_required {
                modifiers.push("NOT NULL");
            }

            // Check nullable (false means NOT NULL)
            let is_nullable = t.get::<bool>("_nullable").unwrap_or(true);
            if !is_nullable && !modifiers.contains(&"NOT NULL") {
                modifiers.push("NOT NULL");
            }
        }
        LuaValue::String(s) => {
            if let Ok(type_str) = s.to_str() {
                col_type = match type_str.as_ref() {
                    "integer" | "int" => "INTEGER".to_string(),
                    "real" | "float" | "number" => "REAL".to_string(),
                    "boolean" | "bool" => "INTEGER".to_string(),
                    "blob" => "BLOB".to_string(),
                    _ => "TEXT".to_string(),
                };
            }
        }
        _ => {}
    }

    if modifiers.is_empty() {
        Ok(format!("{} {}", name, col_type))
    } else {
        Ok(format!("{} {} {}", name, col_type, modifiers.join(" ")))
    }
}

/// Run all pending migrations
pub fn run_pending_migrations(
    executor: &MigrationExecutor,
    migrations_dir: &Path,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let conn = executor.conn.clone();

    let mut pending: Vec<(String, std::path::PathBuf)> = Vec::new();

    if migrations_dir.exists() {
        let entries = fs::read_dir(migrations_dir)
            .map_err(|e| format!("Failed to read migrations directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    if !executor.get_applied_migrations()?.contains(name) {
                        pending.push((name.to_string(), path));
                    }
                }
            }
        }
    }

    if pending.is_empty() {
        println!("✓ All migrations are up to date");
        return Ok(0);
    }

    pending.sort_by(|a, b| a.0.cmp(&b.0));

    println!("📋 Running {} pending migration(s):", pending.len());

    let lua = Lua::new();
    setup_migration_lua(&lua, false)?;

    for (name, path) in &pending {
        println!("  → {}", name);
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read migration {}: {}", name, e))?;

        let sql_statements = execute_migration_lua(&lua, &content, name, false)?;

        for sql in &sql_statements {
            conn.blocking_lock()
                .execute(&sql)
                .map_err(|e| format!("Migration SQL failed: {}", e))?;
        }

        executor.record_migration(name)?;
        println!("    ✓ Applied");
    }

    Ok(pending.len())
}

/// Rollback last N migrations
pub fn rollback_migrations(
    executor: &MigrationExecutor,
    migrations_dir: &Path,
    steps: usize,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let conn = executor.conn.clone();

    let applied: Vec<String> = executor.get_applied_migrations()?.into_iter().collect();

    if applied.is_empty() {
        println!("✓ No migrations to rollback");
        return Ok(0);
    }

    let to_rollback: Vec<&String> = applied.iter().rev().take(steps).collect();

    println!("🔄 Rolling back {} migration(s):", to_rollback.len());

    let lua = Lua::new();
    setup_migration_lua(&lua, true)?;

    for name in &to_rollback {
        println!("  ← {}", name);
        let path = migrations_dir.join(format!("{}.lua", name));

        if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read migration {}: {}", name, e))?;

            let sql_statements = execute_migration_lua(&lua, &content, name, true)?;

            for sql in &sql_statements {
                conn.blocking_lock()
                    .execute(&sql)
                    .map_err(|e| format!("Rollback SQL failed: {}", e))?;
            }

            executor.remove_migration(&name)?;
            println!("    ✓ Rolled back");
        }
    }

    Ok(to_rollback.len())
}

/// Generate reverse operations for rollback (used in tests + future rollback enhancements)
#[allow(dead_code)]
fn generate_reverse_ops(ops: &[MigrationOperation]) -> Vec<MigrationOperation> {
    ops.iter()
        .rev()
        .map(|op| match op.r#type.as_str() {
            "create_table" => MigrationOperation {
                r#type: "drop_table".to_string(),
                table: op.table.clone(),
                column: None,
                column_type: None,
                name: None,
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "drop_table" => MigrationOperation {
                r#type: "create_table".to_string(),
                table: op.table.clone(),
                column: None,
                column_type: None,
                name: None,
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "add_column" => MigrationOperation {
                r#type: "remove_column".to_string(),
                table: op.table.clone(),
                column: op.column.clone(),
                column_type: None,
                name: None,
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "remove_column" => MigrationOperation {
                r#type: "add_column".to_string(),
                table: op.table.clone(),
                column: op.column.clone(),
                column_type: None,
                name: None,
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "rename_column" => MigrationOperation {
                r#type: "rename_column".to_string(),
                table: op.table.clone(),
                column: None,
                column_type: None,
                name: None,
                old_column: op.new_column.clone(),
                new_column: op.old_column.clone(),
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "create_index" => MigrationOperation {
                r#type: "drop_index".to_string(),
                table: op.table.clone(),
                column: None,
                column_type: None,
                name: op.name.clone(),
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "drop_index" => MigrationOperation {
                r#type: "create_index".to_string(),
                table: op.table.clone(),
                column: None,
                column_type: None,
                name: op.name.clone(),
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: None,
                columns: None,
                sql: None,
            },
            "rename_table" => MigrationOperation {
                r#type: "rename_table".to_string(),
                table: op.new_table.clone(),
                column: None,
                column_type: None,
                name: None,
                old_column: None,
                new_column: None,
                old_table: None,
                new_table: op.table.clone(),
                columns: None,
                sql: None,
            },
            "raw" => op.clone(),
            _ => MigrationOperation::default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_reverse_ops() {
        let create_op = MigrationOperation {
            r#type: "create_table".to_string(),
            table: Some("users".to_string()),
            column: None,
            column_type: None,
            name: None,
            old_column: None,
            new_column: None,
            old_table: None,
            new_table: None,
            columns: None,
            sql: None,
        };

        let reversed = generate_reverse_ops(&[create_op]);
        assert_eq!(reversed[0].r#type, "drop_table");
        assert_eq!(reversed[0].table, Some("users".to_string()));

        let add_op = MigrationOperation {
            r#type: "add_column".to_string(),
            table: Some("users".to_string()),
            column: Some("email".to_string()),
            column_type: None,
            name: None,
            old_column: None,
            new_column: None,
            old_table: None,
            new_table: None,
            columns: None,
            sql: None,
        };

        let reversed = generate_reverse_ops(&[add_op]);
        assert_eq!(reversed[0].r#type, "remove_column");
        assert_eq!(reversed[0].table, Some("users".to_string()));
        assert_eq!(reversed[0].column, Some("email".to_string()));
    }
}
