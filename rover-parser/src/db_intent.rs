//! Database Intent Analysis
//!
//! Analyzes Lua AST to extract database intent - what tables, fields, and types
//! the code expects to use. Uses the same tree-sitter infrastructure as the main analyzer.

use std::collections::HashMap;
use tree_sitter::Node;

/// Inferred field type from AST analysis
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Integer,
    Number,
    String,
    Boolean,
    Unknown,
}

impl FieldType {
    /// Convert to guard DSL type name
    pub fn to_guard_type(&self) -> &'static str {
        match self {
            FieldType::Integer => "integer",
            FieldType::Number => "number",
            FieldType::String => "string",
            FieldType::Boolean => "boolean",
            FieldType::Unknown => "string",
        }
    }

    /// Infer type from AST node kind
    pub fn from_ast_kind(kind: &str) -> Self {
        match kind {
            "string" => FieldType::String,
            "number" => FieldType::Number, // Will refine to Integer if whole number
            "true" | "false" => FieldType::Boolean,
            _ => FieldType::Unknown,
        }
    }

    /// Refine number type based on literal value
    pub fn refine_number(value: &str) -> Self {
        if value.contains('.') {
            FieldType::Number
        } else {
            FieldType::Integer
        }
    }
}

/// Source of where a field was inferred from
#[derive(Debug, Clone)]
pub enum FieldSource {
    /// From insert: { field = value }
    Insert { value_hint: String },
    /// From filter: :by_field(value)
    Filter { method: String },
    /// From field access: db.table.field
    Access,
    /// Auto-generated (like id)
    Auto,
}

impl std::fmt::Display for FieldSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldSource::Insert { value_hint } => write!(f, "from insert: {{ {} }}", value_hint),
            FieldSource::Filter { method } => write!(f, "from :{}()", method),
            FieldSource::Access => write!(f, "from field access"),
            FieldSource::Auto => write!(f, "auto-generated"),
        }
    }
}

/// An inferred field
#[derive(Debug, Clone)]
pub struct InferredField {
    pub name: String,
    pub field_type: FieldType,
    pub source: FieldSource,
}

/// An inferred table with its fields
#[derive(Debug, Clone)]
pub struct InferredTable {
    pub name: String,
    pub fields: HashMap<String, InferredField>,
}

impl InferredTable {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: HashMap::new(),
        }
    }

    /// Add field, keeping the more specific type if field exists
    pub fn add_field(&mut self, field: InferredField) {
        let name = field.name.clone();
        if let Some(existing) = self.fields.get(&name) {
            // Keep more specific type
            if existing.field_type == FieldType::Unknown && field.field_type != FieldType::Unknown {
                self.fields.insert(name, field);
            }
        } else {
            self.fields.insert(name, field);
        }
    }

    pub fn ensure_id_field(&mut self) {
        self.fields.insert(
            "id".to_string(),
            InferredField {
                name: "id".to_string(),
                field_type: FieldType::Integer,
                source: FieldSource::Auto,
            },
        );
    }
}

/// Database intent extracted from code
#[derive(Debug, Clone, Default)]
pub struct DbIntent {
    pub tables: HashMap<String, InferredTable>,
}

impl DbIntent {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn get_or_create_table(&mut self, name: &str) -> &mut InferredTable {
        if !self.tables.contains_key(name) {
            self.tables
                .insert(name.to_string(), InferredTable::new(name));
        }
        self.tables.get_mut(name).unwrap()
    }

    pub fn table_names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }

    /// Finalize intent - add auto id fields to all tables
    pub fn finalize(&mut self) {
        for table in self.tables.values_mut() {
            table.ensure_id_field();
        }
    }
}

/// Intent analyzer that walks the AST
pub struct IntentAnalyzer<'a> {
    source: &'a str,
    pub intent: DbIntent,
}

impl<'a> IntentAnalyzer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            intent: DbIntent::new(),
        }
    }

    /// Walk AST node and extract database intent
    pub fn walk(&mut self, node: Node<'a>) {
        match node.kind() {
            "function_call" => self.analyze_function_call(node),
            "dot_index_expression" => self.analyze_dot_access(node),
            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
    }

    /// Analyze a function call for db operations
    fn analyze_function_call(&mut self, node: Node<'a>) {
        // Look for method_index_expression child (db.table:method())
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "method_index_expression" {
                if let Some((object_path, method)) = self.extract_method_info(child) {
                    if let Some(table_name) = self.extract_table_from_path(&object_path) {
                        self.handle_db_method(&table_name, &method, node);
                    }
                }
            }
        }
    }

    /// Handle a db method call
    fn handle_db_method(&mut self, table_name: &str, method: &str, call_node: Node<'a>) {
        match method {
            "insert" => {
                // Extract fields from table constructor argument
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_insert_fields(table_name, args_node);
                }
            }
            _ if method.starts_with("by_") => {
                let raw_field = method.strip_prefix("by_").unwrap();
                if let Some(field_name) = extract_field_from_filter(raw_field) {
                    let field_type = if let Some(args_node) = self.find_arguments(call_node) {
                        self.infer_type_from_argument(args_node)
                    } else {
                        FieldType::Unknown
                    };

                    let table = self.intent.get_or_create_table(table_name);
                    table.add_field(InferredField {
                        name: field_name.to_string(),
                        field_type,
                        source: FieldSource::Filter {
                            method: method.to_string(),
                        },
                    });
                }
            }
            _ => {
                // Just ensure table is tracked
                self.intent.get_or_create_table(table_name);
            }
        }
    }

    /// Extract fields from insert table constructor
    fn extract_insert_fields(&mut self, table_name: &str, args_node: Node<'a>) {
        // Find table_constructor in arguments
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if child.kind() == "table_constructor" {
                self.extract_table_constructor_fields(table_name, child);
            }
        }
    }

    /// Extract fields from table constructor { field = value, ... }
    fn extract_table_constructor_fields(&mut self, table_name: &str, node: Node<'a>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field" {
                self.extract_field(table_name, child);
            }
        }
    }

    /// Extract a single field from a field node
    fn extract_field(&mut self, table_name: &str, node: Node<'a>) {
        let mut field_name: Option<String> = None;
        let mut field_type = FieldType::Unknown;
        let mut value_hint = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if field_name.is_none() => {
                    field_name = Some(self.node_text(child).to_string());
                }
                "string" => {
                    field_type = FieldType::String;
                    value_hint = self.node_text(child).to_string();
                }
                "number" => {
                    let text = self.node_text(child);
                    field_type = FieldType::refine_number(text);
                    value_hint = text.to_string();
                }
                "true" | "false" => {
                    field_type = FieldType::Boolean;
                    value_hint = self.node_text(child).to_string();
                }
                "identifier" if field_name.is_some() => {
                    // Variable reference - can't determine type
                    value_hint = self.node_text(child).to_string();
                }
                _ => {}
            }
        }

        if let Some(name) = field_name {
            // Skip id field - it's auto-generated
            if name == "id" {
                return;
            }

            let table = self.intent.get_or_create_table(table_name);
            table.add_field(InferredField {
                name: name.clone(),
                field_type,
                source: FieldSource::Insert {
                    value_hint: if value_hint.is_empty() {
                        name
                    } else {
                        format!("{} = {}", name, value_hint)
                    },
                },
            });
        }
    }

    /// Analyze dot access for field references like db.table.field
    fn analyze_dot_access(&mut self, node: Node<'a>) {
        let path = self.extract_full_path(node);
        let parts: Vec<&str> = path.split('.').collect();

        // Need db.table.field (3+ parts)
        if parts.len() >= 3 && parts[0] == "db" {
            let table_name = parts[1];
            let field_name = parts[2];

            let table = self.intent.get_or_create_table(table_name);
            table.add_field(InferredField {
                name: field_name.to_string(),
                field_type: FieldType::Unknown,
                source: FieldSource::Access,
            });
        }
    }

    /// Extract method info from method_index_expression
    fn extract_method_info(&self, node: Node<'a>) -> Option<(String, String)> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // First named child is the object
        let object = children.first()?;
        let object_path = self.extract_full_path(*object);

        // Last identifier is the method name
        let method = children
            .iter()
            .rev()
            .find(|c| c.kind() == "identifier")
            .map(|c| self.node_text(*c).to_string())?;

        Some((object_path, method))
    }

    /// Extract full dotted path from expression
    fn extract_full_path(&self, node: Node<'a>) -> String {
        let mut parts = Vec::new();
        self.collect_path_parts(node, &mut parts);
        parts.join(".")
    }

    fn collect_path_parts(&self, node: Node<'a>, parts: &mut Vec<String>) {
        match node.kind() {
            "identifier" => {
                parts.push(self.node_text(node).to_string());
            }
            "dot_index_expression" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();

                // Process base first
                if let Some(base) = children.first() {
                    self.collect_path_parts(*base, parts);
                }

                // Add field (last identifier)
                for child in children.iter().rev() {
                    if child.kind() == "identifier" {
                        parts.push(self.node_text(*child).to_string());
                        break;
                    }
                }
            }
            "function_call" => {
                // For chained calls, look inside
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(
                        child.kind(),
                        "method_index_expression" | "dot_index_expression" | "function_call"
                    ) {
                        self.collect_path_parts(child, parts);
                        break;
                    }
                }
            }
            "method_index_expression" => {
                // Get object part only (before colon)
                if let Some(object) = node.named_child(0) {
                    self.collect_path_parts(object, parts);
                }
            }
            _ => {}
        }
    }

    /// Extract table name from path like "db.users"
    fn extract_table_from_path(&self, path: &str) -> Option<String> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() >= 2 && parts[0] == "db" {
            Some(parts[1].to_string())
        } else {
            None
        }
    }

    /// Find arguments node in function call
    fn find_arguments(&self, call_node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = call_node.walk();
        for child in call_node.children(&mut cursor) {
            if child.kind() == "arguments" {
                return Some(child);
            }
        }
        None
    }

    /// Infer type from first argument
    fn infer_type_from_argument(&self, args_node: Node<'a>) -> FieldType {
        if let Some(first_arg) = args_node.named_child(0) {
            match first_arg.kind() {
                "string" => FieldType::String,
                "number" => FieldType::refine_number(self.node_text(first_arg)),
                "true" | "false" => FieldType::Boolean,
                _ => FieldType::Unknown,
            }
        } else {
            FieldType::Unknown
        }
    }

    /// Get text for a node
    fn node_text(&self, node: Node) -> &str {
        &self.source[node.start_byte()..node.end_byte()]
    }
}

const FILTER_SUFFIXES: &[&str] = &[
    "_ends_with",
    "_starts_with",
    "_contains",
    "_bigger_than",
    "_smaller_than",
    "_between",
    "_in_list",
    "_not_in",
    "_is_null",
    "_is_not_null",
    "_like",
];

fn extract_field_from_filter(raw: &str) -> Option<String> {
    for suffix in FILTER_SUFFIXES {
        if raw.ends_with(suffix) {
            let field = raw.strip_suffix(suffix)?;
            if !field.is_empty() {
                return Some(field.to_string());
            }
        }
    }
    Some(raw.to_string())
}

pub fn analyze_db_intent(source: &str) -> DbIntent {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_lua::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Lua parser");

    let Some(tree) = parser.parse(source, None) else {
        return DbIntent::new();
    };

    let mut analyzer = IntentAnalyzer::new(source);
    analyzer.walk(tree.root_node());
    analyzer.intent.finalize();
    analyzer.intent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_fields() {
        let code = r#"db.users:insert({ name = "Alice", age = 30, active = true })"#;
        let intent = analyze_db_intent(code);

        let users = intent.tables.get("users").unwrap();
        assert_eq!(
            users.fields.get("name").unwrap().field_type,
            FieldType::String
        );
        assert_eq!(
            users.fields.get("age").unwrap().field_type,
            FieldType::Integer
        );
        assert_eq!(
            users.fields.get("active").unwrap().field_type,
            FieldType::Boolean
        );
    }

    #[test]
    fn test_filter_fields() {
        let code = r#"db.users:find():by_status("active"):all()"#;
        let intent = analyze_db_intent(code);

        let users = intent.tables.get("users").unwrap();
        assert!(users.fields.contains_key("status"));
        assert_eq!(
            users.fields.get("status").unwrap().field_type,
            FieldType::String
        );
    }

    #[test]
    fn test_field_access() {
        let code = r#"local x = db.orders.user_id"#;
        let intent = analyze_db_intent(code);

        let orders = intent.tables.get("orders").unwrap();
        assert!(orders.fields.contains_key("user_id"));
    }

    #[test]
    fn test_auto_id() {
        let code = r#"db.products:insert({ name = "Widget" })"#;
        let intent = analyze_db_intent(code);

        let products = intent.tables.get("products").unwrap();
        assert!(products.fields.contains_key("id"));
        assert_eq!(
            products.fields.get("id").unwrap().field_type,
            FieldType::Integer
        );
    }

    #[test]
    fn test_number_vs_integer() {
        let code = r#"db.orders:insert({ count = 5, amount = 99.99 })"#;
        let intent = analyze_db_intent(code);

        let orders = intent.tables.get("orders").unwrap();
        assert_eq!(
            orders.fields.get("count").unwrap().field_type,
            FieldType::Integer
        );
        assert_eq!(
            orders.fields.get("amount").unwrap().field_type,
            FieldType::Number
        );
    }
}
