//! Query execution engine
//!
//! Bridges Lua queries to actual database operations.

use crate::connection::Connection;
use mlua::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Query executor that handles all database operations
pub struct QueryExecutor {
    conn: Arc<Mutex<Connection>>,
}

impl QueryExecutor {
    /// Create a new query executor
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Execute an INSERT operation
    pub fn execute_insert(
        &self,
        lua: &Lua,
        sql: &str,
        table_name: &str,
        _data: LuaValue,
    ) -> LuaResult<LuaValue> {
        let conn = self.conn.blocking_lock();

        // Check if table exists - migrations must create tables
        if !table_name.is_empty() && !conn.table_exists(table_name).unwrap_or(false) {
            return Err(LuaError::RuntimeError(format!(
                "Table '{}' does not exist. Generate and run migration for this table first.",
                table_name
            )));
        }

        // Execute the INSERT
        conn.execute(sql)
            .map_err(|e| LuaError::RuntimeError(format!("Insert failed: {}", e)))?;

        // Return the last insert ID
        let last_id = conn.last_insert_rowid();

        // Return a table with the result
        let result = lua.create_table()?;
        result.set("id", last_id)?;
        result.set("success", true)?;

        Ok(LuaValue::Table(result))
    }

    /// Execute a SELECT query
    pub fn execute_query(&self, lua: &Lua, sql: &str, mode: &str) -> LuaResult<LuaValue> {
        let conn = self.conn.blocking_lock();

        if mode == "count" {
            // For count queries, wrap in COUNT(*)
            let count_sql = if sql.to_uppercase().contains("SELECT *") {
                sql.replace("SELECT *", "SELECT COUNT(*)")
            } else {
                format!("SELECT COUNT(*) as count FROM ({})", sql)
            };

            let rows = conn
                .query(&count_sql)
                .map_err(|e| LuaError::RuntimeError(format!("Query failed: {}", e)))?;

            if let Some(row) = rows.first() {
                for (_, value) in row {
                    if let libsql::Value::Integer(count) = value {
                        return Ok(LuaValue::Integer(*count));
                    }
                }
            }
            return Ok(LuaValue::Integer(0));
        }

        let rows = conn
            .query(sql)
            .map_err(|e| LuaError::RuntimeError(format!("Query failed: {}", e)))?;

        // Convert rows to Lua tables
        let results = lua.create_table()?;

        for (idx, row) in rows.iter().enumerate() {
            let row_table = lua.create_table()?;

            for (col_name, value) in row {
                let lua_value = libsql_value_to_lua(lua, value)?;
                row_table.set(col_name.as_str(), lua_value)?;
            }

            results.set(idx + 1, row_table)?;
        }

        Ok(LuaValue::Table(results))
    }

    /// Execute an UPDATE operation
    pub fn execute_update(&self, sql: &str) -> LuaResult<LuaValue> {
        let conn = self.conn.blocking_lock();

        let affected = conn
            .execute(sql)
            .map_err(|e| LuaError::RuntimeError(format!("Update failed: {}", e)))?;

        Ok(LuaValue::Integer(affected as i64))
    }

    /// Execute a DELETE operation
    pub fn execute_delete(&self, sql: &str) -> LuaResult<LuaValue> {
        let conn = self.conn.blocking_lock();

        let affected = conn
            .execute(sql)
            .map_err(|e| LuaError::RuntimeError(format!("Delete failed: {}", e)))?;

        Ok(LuaValue::Integer(affected as i64))
    }

    /// Execute a raw SQL query
    pub fn execute_raw(
        &self,
        lua: &Lua,
        sql: &str,
        params: Option<LuaTable>,
    ) -> LuaResult<LuaValue> {
        let conn = self.conn.blocking_lock();

        // Convert Lua params to libsql values if provided
        let libsql_params = if let Some(params_table) = params {
            let mut values = Vec::new();
            for pair in params_table.pairs::<i64, LuaValue>() {
                if let Ok((_, value)) = pair {
                    values.push(lua_value_to_libsql(&value)?);
                }
            }
            values
        } else {
            Vec::new()
        };

        // Check if it's a SELECT query
        let sql_upper = sql.trim().to_uppercase();
        if sql_upper.starts_with("SELECT") {
            let rows = if libsql_params.is_empty() {
                conn.query(sql)
            } else {
                conn.query_with_params(sql, libsql_params)
            }
            .map_err(|e| LuaError::RuntimeError(format!("Query failed: {}", e)))?;

            let results = lua.create_table()?;
            for (idx, row) in rows.iter().enumerate() {
                let row_table = lua.create_table()?;
                for (col_name, value) in row {
                    let lua_value = libsql_value_to_lua(lua, value)?;
                    row_table.set(col_name.as_str(), lua_value)?;
                }
                results.set(idx + 1, row_table)?;
            }

            Ok(LuaValue::Table(results))
        } else {
            // Execute non-SELECT statement
            let affected = conn
                .execute(sql)
                .map_err(|e| LuaError::RuntimeError(format!("Execution failed: {}", e)))?;

            Ok(LuaValue::Integer(affected as i64))
        }
    }
}

/// Convert libsql Value to Lua Value
fn libsql_value_to_lua(lua: &Lua, value: &libsql::Value) -> LuaResult<LuaValue> {
    match value {
        libsql::Value::Null => Ok(LuaValue::Nil),
        libsql::Value::Integer(i) => Ok(LuaValue::Integer(*i)),
        libsql::Value::Real(f) => Ok(LuaValue::Number(*f)),
        libsql::Value::Text(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        libsql::Value::Blob(b) => Ok(LuaValue::String(lua.create_string(b)?)),
    }
}

/// Convert Lua Value to libsql Value
fn lua_value_to_libsql(value: &LuaValue) -> LuaResult<libsql::Value> {
    match value {
        LuaValue::Nil => Ok(libsql::Value::Null),
        LuaValue::Integer(i) => Ok(libsql::Value::Integer(*i)),
        LuaValue::Number(n) => Ok(libsql::Value::Real(*n)),
        LuaValue::String(s) => Ok(libsql::Value::Text(s.to_str()?.to_string())),
        LuaValue::Boolean(b) => Ok(libsql::Value::Integer(if *b { 1 } else { 0 })),
        _ => Err(LuaError::RuntimeError(
            "Unsupported Lua type for SQL parameter".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_basic_operations() {
        let conn = Connection::new(":memory:").unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let executor = QueryExecutor::new(conn.clone());

        // Create table manually for testing
        {
            let c = conn.blocking_lock();
            c.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .unwrap();
        }

        let lua = Lua::new();

        // Test insert
        let data = lua.create_table().unwrap();
        data.set("name", "Alice").unwrap();
        data.set("age", 30).unwrap();

        let sql = "INSERT INTO users (name, age) VALUES ('Alice', 30)";
        let result = executor
            .execute_insert(&lua, sql, "users", LuaValue::Table(data))
            .unwrap();

        if let LuaValue::Table(t) = result {
            assert!(t.get::<bool>("success").unwrap());
        }

        // Test query
        let rows = executor
            .execute_query(&lua, "SELECT * FROM users", "all")
            .unwrap();

        if let LuaValue::Table(t) = rows {
            assert_eq!(t.len().unwrap(), 1);
        }
    }
}
