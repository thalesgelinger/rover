//! Database connection management using libsql
//!
//! Handles SQLite connections with support for both local files and in-memory databases.

use libsql::{Builder, Database};
use std::sync::Arc;
use thiserror::Error;
use tokio::runtime::Runtime;

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Query error: {0}")]
    Query(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
}

/// Represents a database connection
pub struct Connection {
    db: Database,
    conn: libsql::Connection,
    runtime: Arc<Runtime>,
}

impl Connection {
    /// Create a new connection to the database
    pub fn new(path: &str) -> Result<Self, ConnectionError> {
        // Create a tokio runtime for async operations
        let runtime = Runtime::new().map_err(|e| ConnectionError::Runtime(e.to_string()))?;

        let (db, conn) = runtime.block_on(async {
            let db = if path == ":memory:" {
                Builder::new_local(":memory:")
                    .build()
                    .await
                    .map_err(|e| ConnectionError::Database(e.to_string()))?
            } else {
                Builder::new_local(path)
                    .build()
                    .await
                    .map_err(|e| ConnectionError::Database(e.to_string()))?
            };

            let conn = db
                .connect()
                .map_err(|e| ConnectionError::Database(e.to_string()))?;

            Ok::<_, ConnectionError>((db, conn))
        })?;

        Ok(Self {
            db,
            conn,
            runtime: Arc::new(runtime),
        })
    }

    /// Execute a SQL statement that doesn't return rows (INSERT, UPDATE, DELETE, CREATE)
    pub fn execute(&self, sql: &str) -> Result<u64, ConnectionError> {
        self.runtime.block_on(async {
            self.conn
                .execute(sql, ())
                .await
                .map_err(|e| ConnectionError::Query(e.to_string()))
        })
    }

    /// Execute a SQL query and return rows
    pub fn query(&self, sql: &str) -> Result<Vec<Vec<(String, libsql::Value)>>, ConnectionError> {
        self.runtime.block_on(async {
            let mut rows = self
                .conn
                .query(sql, ())
                .await
                .map_err(|e| ConnectionError::Query(e.to_string()))?;

            let mut results = Vec::new();

            while let Some(row) = rows
                .next()
                .await
                .map_err(|e| ConnectionError::Query(e.to_string()))?
            {
                let column_count = rows.column_count();
                let mut row_data = Vec::new();

                for i in 0..column_count {
                    let col_name = rows
                        .column_name(i)
                        .unwrap_or(&format!("col_{}", i))
                        .to_string();
                    let value = row.get_value(i as i32).unwrap_or(libsql::Value::Null);
                    row_data.push((col_name, value));
                }

                results.push(row_data);
            }

            Ok(results)
        })
    }

    /// Execute a SQL query with parameters
    pub fn query_with_params(
        &self,
        sql: &str,
        params: Vec<libsql::Value>,
    ) -> Result<Vec<Vec<(String, libsql::Value)>>, ConnectionError> {
        self.runtime.block_on(async {
            let mut rows = self
                .conn
                .query(sql, params)
                .await
                .map_err(|e| ConnectionError::Query(e.to_string()))?;

            let mut results = Vec::new();

            while let Some(row) = rows
                .next()
                .await
                .map_err(|e| ConnectionError::Query(e.to_string()))?
            {
                let column_count = rows.column_count();
                let mut row_data = Vec::new();

                for i in 0..column_count {
                    let col_name = rows
                        .column_name(i)
                        .unwrap_or(&format!("col_{}", i))
                        .to_string();
                    let value = row.get_value(i as i32).unwrap_or(libsql::Value::Null);
                    row_data.push((col_name, value));
                }

                results.push(row_data);
            }

            Ok(results)
        })
    }

    /// Get the last inserted row ID
    pub fn last_insert_rowid(&self) -> i64 {
        self.runtime.block_on(async { self.conn.last_insert_rowid() })
    }

    /// Check if a table exists
    pub fn table_exists(&self, table_name: &str) -> Result<bool, ConnectionError> {
        let sql = format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            table_name.replace('\'', "''")
        );
        let rows = self.query(&sql)?;
        Ok(!rows.is_empty())
    }

    /// Get table schema (column info)
    pub fn get_table_schema(
        &self,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>, ConnectionError> {
        let sql = format!("PRAGMA table_info({})", table_name);
        let rows = self.query(&sql)?;

        let mut columns = Vec::new();
        for row in rows {
            let mut col_info = ColumnInfo::default();

            for (name, value) in row {
                match name.as_str() {
                    "name" => {
                        if let libsql::Value::Text(s) = value {
                            col_info.name = s;
                        }
                    }
                    "type" => {
                        if let libsql::Value::Text(s) = value {
                            col_info.column_type = s;
                        }
                    }
                    "notnull" => {
                        col_info.not_null = matches!(value, libsql::Value::Integer(1));
                    }
                    "pk" => {
                        col_info.primary_key = matches!(value, libsql::Value::Integer(1));
                    }
                    "dflt_value" => {
                        if let libsql::Value::Text(s) = value {
                            col_info.default_value = Some(s);
                        }
                    }
                    _ => {}
                }
            }

            columns.push(col_info);
        }

        Ok(columns)
    }
}

/// Information about a database column
#[derive(Debug, Default, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub not_null: bool,
    pub primary_key: bool,
    pub default_value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_connection() {
        let conn = Connection::new(":memory:").unwrap();

        // Create a test table
        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)")
            .unwrap();

        // Insert data
        conn.execute("INSERT INTO test (name) VALUES ('hello')")
            .unwrap();

        // Query data
        let rows = conn.query("SELECT * FROM test").unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_table_exists() {
        let conn = Connection::new(":memory:").unwrap();

        assert!(!conn.table_exists("test").unwrap());

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY)")
            .unwrap();

        assert!(conn.table_exists("test").unwrap());
    }

    #[test]
    fn test_get_table_schema() {
        let conn = Connection::new(":memory:").unwrap();

        conn.execute(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)",
        )
        .unwrap();

        let schema = conn.get_table_schema("users").unwrap();
        assert_eq!(schema.len(), 3);

        assert_eq!(schema[0].name, "id");
        assert!(schema[0].primary_key);

        assert_eq!(schema[1].name, "name");
        assert!(schema[1].not_null);

        assert_eq!(schema[2].name, "age");
    }
}
