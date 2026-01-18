//! Schema inference and migration management
//!
//! Infers database schema from Lua data and handles automatic migrations.

use crate::connection::Connection;
use mlua::prelude::*;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("Type inference failed: {0}")]
    TypeInference(String),
    #[error("Migration failed: {0}")]
    Migration(String),
}

/// Represents an inferred column schema
#[derive(Debug, Clone)]
pub struct InferredColumn {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

/// Schema manager for inference and migrations
pub struct SchemaManager {
    conn: Arc<Mutex<Connection>>,
}

impl SchemaManager {
    /// Create a new schema manager
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Infer schema from a Lua table
    pub fn infer_schema_from_lua(&self, data: &LuaTable) -> Result<Vec<InferredColumn>, SchemaError> {
        let mut columns = Vec::new();

        // Check if 'id' field exists, if not we'll add it as auto-increment primary key
        let has_id = data.get::<LuaValue>("id").is_ok()
            && !matches!(data.get::<LuaValue>("id"), Ok(LuaValue::Nil));

        if !has_id {
            columns.push(InferredColumn {
                name: "id".to_string(),
                sql_type: "INTEGER".to_string(),
                nullable: false,
                primary_key: true,
            });
        }

        // Iterate through table keys
        for pair in data.clone().pairs::<String, LuaValue>() {
            let (key, value) = pair.map_err(|e| SchemaError::TypeInference(e.to_string()))?;

            let sql_type = infer_sql_type(&value);
            let is_primary_key = key == "id";

            columns.push(InferredColumn {
                name: key,
                sql_type,
                nullable: !is_primary_key, // Only id is NOT NULL by default
                primary_key: is_primary_key,
            });
        }

        // Sort columns to ensure 'id' comes first
        columns.sort_by(|a, b| {
            if a.name == "id" {
                std::cmp::Ordering::Less
            } else if b.name == "id" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        Ok(columns)
    }

    /// Generate CREATE TABLE SQL from inferred schema
    pub fn generate_create_table(&self, table_name: &str, columns: &[InferredColumn]) -> String {
        let mut column_defs = Vec::new();

        for col in columns {
            let mut def = format!("{} {}", col.name, col.sql_type);

            if col.primary_key {
                def.push_str(" PRIMARY KEY");
                if col.sql_type == "INTEGER" {
                    def.push_str(" AUTOINCREMENT");
                }
            }

            if !col.nullable && !col.primary_key {
                def.push_str(" NOT NULL");
            }

            column_defs.push(def);
        }

        format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            table_name,
            column_defs.join(", ")
        )
    }

    /// Check if a table needs migration (new columns added)
    pub fn check_migration_needed(
        &self,
        table_name: &str,
        new_columns: &[InferredColumn],
    ) -> Result<Vec<InferredColumn>, SchemaError> {
        let conn = self.conn.blocking_lock();

        let existing_schema = conn
            .get_table_schema(table_name)
            .map_err(|e| SchemaError::Migration(e.to_string()))?;

        let existing_names: std::collections::HashSet<String> =
            existing_schema.iter().map(|c| c.name.clone()).collect();

        let missing_columns: Vec<InferredColumn> = new_columns
            .iter()
            .filter(|c| !existing_names.contains(&c.name))
            .cloned()
            .collect();

        Ok(missing_columns)
    }

    /// Apply migration (add missing columns)
    pub fn apply_migration(
        &self,
        table_name: &str,
        new_columns: &[InferredColumn],
    ) -> Result<(), SchemaError> {
        let conn = self.conn.blocking_lock();

        for col in new_columns {
            let sql = format!(
                "ALTER TABLE {} ADD COLUMN {} {}{}",
                table_name,
                col.name,
                col.sql_type,
                if col.nullable { "" } else { " NOT NULL DEFAULT ''" }
            );

            conn.execute(&sql)
                .map_err(|e| SchemaError::Migration(e.to_string()))?;
        }

        Ok(())
    }
}

/// Infer SQL type from Lua value
fn infer_sql_type(value: &LuaValue) -> String {
    match value {
        LuaValue::Integer(_) => "INTEGER".to_string(),
        LuaValue::Number(_) => "REAL".to_string(),
        LuaValue::Boolean(_) => "INTEGER".to_string(), // SQLite uses INTEGER for boolean
        LuaValue::String(s) => {
            // Try to detect date/datetime patterns
            if let Ok(s_str) = s.to_str() {
                if is_datetime_string(&s_str) {
                    return "DATETIME".to_string();
                }
                if is_date_string(&s_str) {
                    return "DATE".to_string();
                }
            }
            "TEXT".to_string()
        }
        LuaValue::Table(_) => "TEXT".to_string(), // JSON-serialize tables
        LuaValue::Nil => "TEXT".to_string(),       // Default to TEXT for nil
        _ => "TEXT".to_string(),
    }
}

/// Check if string looks like a datetime
fn is_datetime_string(s: &str) -> bool {
    // ISO 8601 datetime pattern: YYYY-MM-DDTHH:MM:SS
    let parts: Vec<&str> = s.split(['T', ' ']).collect();
    if parts.len() >= 2 {
        return is_date_string(parts[0]) && is_time_string(parts[1]);
    }
    false
}

/// Check if string looks like a date
fn is_date_string(s: &str) -> bool {
    // Simple date pattern: YYYY-MM-DD
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 {
        return parts[0].len() == 4
            && parts[1].len() == 2
            && parts[2].len() >= 2
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
    }
    false
}

/// Check if string looks like a time
fn is_time_string(s: &str) -> bool {
    // Simple time pattern: HH:MM:SS or HH:MM
    let s = s.split('.').next().unwrap_or(s); // Remove milliseconds
    let s = s.split(['+', '-', 'Z']).next().unwrap_or(s); // Remove timezone
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_sql_type() {
        assert_eq!(infer_sql_type(&LuaValue::Integer(42)), "INTEGER");
        assert_eq!(infer_sql_type(&LuaValue::Number(3.14)), "REAL");
        assert_eq!(infer_sql_type(&LuaValue::Boolean(true)), "INTEGER");
    }

    #[test]
    fn test_is_date_string() {
        assert!(is_date_string("2024-01-15"));
        assert!(!is_date_string("not-a-date"));
        assert!(!is_date_string("2024/01/15"));
    }

    #[test]
    fn test_is_datetime_string() {
        assert!(is_datetime_string("2024-01-15T10:30:00"));
        assert!(is_datetime_string("2024-01-15 10:30:00"));
        assert!(!is_datetime_string("2024-01-15"));
    }

    #[test]
    fn test_schema_inference() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("name", "Alice").unwrap();
        data.set("age", 30).unwrap();
        data.set("score", 95.5).unwrap();
        data.set("active", true).unwrap();

        let conn = Connection::new(":memory:").unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let manager = SchemaManager::new(conn);

        let schema = manager.infer_schema_from_lua(&data).unwrap();

        // Should have 5 columns: id (auto-added) + 4 from data
        assert_eq!(schema.len(), 5);

        // id should be first and primary key
        assert_eq!(schema[0].name, "id");
        assert!(schema[0].primary_key);

        // Find other columns
        let name_col = schema.iter().find(|c| c.name == "name").unwrap();
        assert_eq!(name_col.sql_type, "TEXT");

        let age_col = schema.iter().find(|c| c.name == "age").unwrap();
        assert_eq!(age_col.sql_type, "INTEGER");

        let score_col = schema.iter().find(|c| c.name == "score").unwrap();
        assert_eq!(score_col.sql_type, "REAL");
    }

    #[test]
    fn test_generate_create_table() {
        let conn = Connection::new(":memory:").unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let manager = SchemaManager::new(conn);

        let columns = vec![
            InferredColumn {
                name: "id".to_string(),
                sql_type: "INTEGER".to_string(),
                nullable: false,
                primary_key: true,
            },
            InferredColumn {
                name: "name".to_string(),
                sql_type: "TEXT".to_string(),
                nullable: true,
                primary_key: false,
            },
        ];

        let sql = manager.generate_create_table("users", &columns);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("id INTEGER PRIMARY KEY AUTOINCREMENT"));
        assert!(sql.contains("name TEXT"));
    }
}
