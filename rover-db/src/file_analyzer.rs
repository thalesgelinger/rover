//! File analyzer - analyzes Lua files for DB usage using AST
//!
//! Uses tree-sitter to parse Lua code and detect which tables are being used,
//! so we can prompt the user to create them before execution.

use crate::connection::Connection;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// Analyze a Lua file and return the set of tables being used
pub fn analyze_db_usage(file_content: &str) -> HashSet<String> {
    let mut tables = HashSet::new();

    // Parse the Lua code
    let mut parser = Parser::new();
    let language = tree_sitter_lua::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Lua parser");

    let Some(tree) = parser.parse(file_content, None) else {
        return tables;
    };

    // Walk the AST to find db.* patterns
    walk_tree(tree.root_node(), file_content, &mut tables);

    tables
}

/// Recursively walk the AST to find db table accesses
fn walk_tree<'a>(node: Node<'a>, source: &'a str, tables: &mut HashSet<String>) {
    match node.kind() {
        "function_call" => {
            // Handle method calls like db.users:insert(), db.orders:find()
            if let Some(table_name) = extract_table_from_call(node, source) {
                tables.insert(table_name);
            }
        }
        "dot_index_expression" => {
            // Handle field access like db.orders.id, db.users.name
            if let Some(table_name) = extract_table_from_dot_access(node, source) {
                tables.insert(table_name);
            }
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, source, tables);
    }
}

/// Extract table name from a function call like db.users:insert() or db.users:find():all()
fn extract_table_from_call(call_node: Node, source: &str) -> Option<String> {
    let mut cursor = call_node.walk();

    for child in call_node.children(&mut cursor) {
        match child.kind() {
            "method_index_expression" => {
                // This is like: db.users:insert
                // The object part is in the first child
                if let Some(object) = child.named_child(0) {
                    return extract_db_table_name(object, source);
                }
            }
            "dot_index_expression" => {
                // This could be db.users.sql or similar
                return extract_db_table_name(child, source);
            }
            "identifier" | "function_call" => {
                // Recursive call like db.users:find():all()
                // The inner function_call contains the actual db access
                if child.kind() == "function_call" {
                    return extract_table_from_call(child, source);
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract table name from a dot_index_expression like db.users or db.orders.id
fn extract_table_from_dot_access(node: Node, source: &str) -> Option<String> {
    extract_db_table_name(node, source)
}

/// Extract the table name if the expression starts with "db."
/// Returns the second component of the path (e.g., "users" from "db.users" or "db.users.id")
fn extract_db_table_name(node: Node, source: &str) -> Option<String> {
    // Build the full path by walking up/down the dot expression
    let parts = collect_dot_parts(node, source);

    // Check if this is a db.* pattern (first part must be "db")
    if parts.len() >= 2 && parts[0] == "db" {
        // Return the table name (second component)
        Some(parts[1].clone())
    } else {
        None
    }
}

/// Collect all parts of a dot expression
/// For "db.users.id", returns ["db", "users", "id"]
fn collect_dot_parts(node: Node, source: &str) -> Vec<String> {
    let mut parts = Vec::new();
    collect_dot_parts_recursive(node, source, &mut parts);
    parts
}

fn collect_dot_parts_recursive(node: Node, source: &str, parts: &mut Vec<String>) {
    match node.kind() {
        "identifier" => {
            let text = &source[node.start_byte()..node.end_byte()];
            parts.push(text.to_string());
        }
        "dot_index_expression" => {
            // First child is the base (could be another dot_index_expression or identifier)
            // Last identifier child is the field name
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();

            // Process base first (left side)
            if let Some(base) = children.first() {
                collect_dot_parts_recursive(*base, source, parts);
            }

            // Then add the field (last identifier)
            for child in children.iter().rev() {
                if child.kind() == "identifier" {
                    let text = &source[child.start_byte()..child.end_byte()];
                    parts.push(text.to_string());
                    break;
                }
            }
        }
        "method_index_expression" => {
            // Similar to dot_index_expression but for method calls with ':'
            // We only care about the object part (before the colon)
            if let Some(object) = node.named_child(0) {
                collect_dot_parts_recursive(object, source, parts);
            }
        }
        _ => {}
    }
}

/// Result of file analysis
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub file_path: String,
    pub tables_used: Vec<String>,
    pub tables_missing: Vec<String>,
}

/// Analyze a file and check which tables are missing from the database
pub fn analyze_file_with_db(
    file_path: &str,
    file_content: &str,
    db_path: &str,
) -> Result<AnalysisResult, String> {
    let tables_used = analyze_db_usage(file_content);
    let tables_missing = check_missing_tables(&tables_used, db_path)?;

    Ok(AnalysisResult {
        file_path: file_path.to_string(),
        tables_used: tables_used.into_iter().collect(),
        tables_missing,
    })
}

/// Check which tables from a set don't exist in the database
fn check_missing_tables(tables: &HashSet<String>, db_path: &str) -> Result<Vec<String>, String> {
    let connection =
        Connection::new(db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    let mut missing = Vec::new();

    // Check which tables are missing
    for table in tables {
        if !connection
            .table_exists(table)
            .map_err(|e| format!("Failed to check table: {}", e))?
        {
            missing.push(table.clone());
        }
    }

    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_db_usage_basic() {
        let code = r#"
            local db = rover.db.connect()

            db.users:insert({ name = "Alice" })
            db.orders:find():all()
            db.products:update():set({ price = 10 }):exec()
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 3);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
        assert!(tables.contains("products"));
    }

    #[test]
    fn test_analyze_db_usage_with_sql() {
        let code = r#"
            local db = rover.db.connect()

            db.users:sql():raw("SELECT * FROM users")
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 1);
        assert!(tables.contains("users"));
    }

    #[test]
    fn test_analyze_db_usage_with_chained_calls() {
        let code = r#"
            local user = db.users:find():by_id(1):first()

            db.users:update()
                :by_id(user.id)
                :set({ status = "active" })
                :exec()

            db.orders:delete():by_status("cancelled"):exec()
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 2);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_analyze_db_usage_with_field_access() {
        let code = r#"
            local order_summary = db.orders:find()
                :group_by(db.orders.user_id)
                :agg({
                    total = rover.db.sum(db.orders.amount),
                    count = rover.db.count(db.orders.id)
                })
                :all()
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 1);
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_analyze_db_usage_multiple_tables() {
        let code = r#"
            local db = rover.db.connect()

            local user1 = db.users:insert({ name = "Alice" })
            local order1 = db.orders:insert({ user_id = user1.id, amount = 100 })

            local query = db.users:find()
                :order_by(db.users.name, "ASC")
                :limit(10)
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 2);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }

    #[test]
    fn test_analyze_ignores_non_db_calls() {
        let code = r#"
            local config = app.config.get("key")
            local result = util.parse(data)
            print("hello")
        "#;

        let tables = analyze_db_usage(code);
        assert!(tables.is_empty());
    }

    #[test]
    fn test_analyze_db_usage_from_example() {
        let code = r#"
local db = rover.db.connect()

local user1 = db.users:insert({ name = "Alice", age = 30 })
local order1 = db.orders:insert({ user_id = user1.id, amount = 100.50 })

local active = db.users:find():by_status("active"):all()
local big_spenders = db.orders:find()
    :group_by(db.orders.user_id)
    :having_total_bigger_than(100)
    :all()
        "#;

        let tables = analyze_db_usage(code);
        assert_eq!(tables.len(), 2);
        assert!(tables.contains("users"));
        assert!(tables.contains("orders"));
    }
}
