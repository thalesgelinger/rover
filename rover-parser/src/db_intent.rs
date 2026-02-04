//! Database Intent Analysis
//!
//! Analyzes Lua AST to extract database intent - what tables, fields, and types
//! the code expects to use. Uses the same tree-sitter infrastructure as the main analyzer.

use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

use crate::analyzer::{ParsingError, SourceRange};

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

/// Database schema definition (table -> fields)
#[derive(Debug, Clone, Default)]
pub struct DbSchema {
    pub tables: HashMap<String, DbSchemaTable>,
}

#[derive(Debug, Clone, Default)]
pub struct DbSchemaTable {
    pub fields: Vec<String>,
}

impl DbSchema {
    pub fn from_table_fields(mut tables: HashMap<String, Vec<String>>) -> Self {
        let mut map = HashMap::new();
        for (name, mut fields) in tables.drain() {
            fields.sort();
            fields.dedup();
            map.insert(name, DbSchemaTable { fields });
        }
        Self { tables: map }
    }

    pub fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tables.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn table_fields(&self, table: &str) -> Option<&[String]> {
        self.tables.get(table).map(|t| t.fields.as_slice())
    }
}

#[derive(Debug, Clone)]
pub struct DbTableUsage {
    pub name: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone)]
pub enum DbFieldUsageKind {
    Insert,
    Set,
    Filter { method: String },
    Select,
    GroupBy,
    OrderBy,
}

#[derive(Debug, Clone)]
pub struct DbFieldUsage {
    pub table: String,
    pub field: String,
    pub kind: DbFieldUsageKind,
    pub range: SourceRange,
}

/// Database intent extracted from code
#[derive(Debug, Clone)]
pub struct DbIntent {
    pub tables: HashMap<String, InferredTable>,
    pub table_usages: HashMap<String, DbTableUsage>,
    pub field_usages: Vec<DbFieldUsage>,
    pub db_instances: HashSet<String>,
}

impl Default for DbIntent {
    fn default() -> Self {
        Self::new()
    }
}

impl DbIntent {
    pub fn new() -> Self {
        let mut instances = HashSet::new();
        instances.insert("db".to_string());
        Self {
            tables: HashMap::new(),
            table_usages: HashMap::new(),
            field_usages: Vec::new(),
            db_instances: instances,
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

    pub fn add_db_instance(&mut self, name: String) {
        self.db_instances.insert(name);
    }

    pub fn is_db_instance(&self, name: &str) -> bool {
        self.db_instances.contains(name)
    }

    pub fn record_table_usage(&mut self, name: &str, range: SourceRange) {
        self.table_usages
            .entry(name.to_string())
            .or_insert(DbTableUsage {
                name: name.to_string(),
                range,
            });
    }

    pub fn record_field_usage(&mut self, usage: DbFieldUsage) {
        self.field_usages.push(usage);
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
            "assignment_statement" | "local_variable_declaration" | "variable_declaration" => {
                self.inspect_db_assignment(node);
            }
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
                if let Some((object_node, method, method_range)) = self.extract_method_info(child) {
                    if let Some((table_name, table_range)) =
                        self.extract_table_from_node(object_node)
                    {
                        self.intent.record_table_usage(&table_name, table_range);
                        self.handle_db_method(&table_name, &method, method_range, node);
                    }
                }
            }
        }
    }

    fn inspect_db_assignment(&mut self, node: Node<'a>) {
        if matches!(
            node.kind(),
            "assignment_statement" | "local_variable_declaration"
        ) {
            self.track_db_assignment(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "assignment_statement" {
                self.track_db_assignment(child);
            }
        }
    }

    fn track_db_assignment(&mut self, node: Node<'a>) {
        let (variables, expressions) = self.extract_assignment_parts(node);
        for (idx, var_name) in variables.iter().enumerate() {
            if let Some(expr) = expressions.get(idx) {
                if self.is_rover_db_connect_call(*expr) {
                    self.intent.add_db_instance(var_name.clone());
                } else if expr.kind() == "identifier" {
                    let name = self.node_text(*expr);
                    if self.intent.is_db_instance(name) {
                        self.intent.add_db_instance(var_name.clone());
                    }
                }
            }
        }
    }

    fn extract_assignment_parts(&self, node: Node<'a>) -> (Vec<String>, Vec<Node<'a>>) {
        let mut variables = Vec::new();
        let mut expressions = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "variable_list" | "name_list" => {
                    let mut var_cursor = child.walk();
                    for var in child.children(&mut var_cursor) {
                        if var.kind() == "identifier" {
                            variables.push(self.node_text(var).to_string());
                        }
                    }
                }
                "expression_list" => {
                    let mut expr_cursor = child.walk();
                    for expr in child.named_children(&mut expr_cursor) {
                        expressions.push(expr);
                    }
                }
                _ => {}
            }
        }
        (variables, expressions)
    }

    fn is_rover_db_connect_call(&self, node: Node<'a>) -> bool {
        if node.kind() != "function_call" {
            return false;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "dot_index_expression" {
                let path = self.extract_full_path(child);
                return path == "rover.db.connect";
            }
        }
        false
    }

    /// Handle a db method call
    fn handle_db_method(
        &mut self,
        table_name: &str,
        method: &str,
        method_range: SourceRange,
        call_node: Node<'a>,
    ) {
        match method {
            "insert" => {
                self.intent.get_or_create_table(table_name);
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_insert_fields(table_name, args_node, DbFieldUsageKind::Insert);
                }
            }
            "set" => {
                self.intent.get_or_create_table(table_name);
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_insert_fields(table_name, args_node, DbFieldUsageKind::Set);
                }
            }
            "select" => {
                self.intent.get_or_create_table(table_name);
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_string_argument_fields(
                        table_name,
                        args_node,
                        DbFieldUsageKind::Select,
                        None,
                    );
                }
            }
            "group_by" => {
                self.intent.get_or_create_table(table_name);
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_string_argument_fields(
                        table_name,
                        args_node,
                        DbFieldUsageKind::GroupBy,
                        None,
                    );
                }
            }
            "order_by" => {
                self.intent.get_or_create_table(table_name);
                if let Some(args_node) = self.find_arguments(call_node) {
                    self.extract_string_argument_fields(
                        table_name,
                        args_node,
                        DbFieldUsageKind::OrderBy,
                        Some(1),
                    );
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
                    self.intent.record_field_usage(DbFieldUsage {
                        table: table_name.to_string(),
                        field: field_name.to_string(),
                        kind: DbFieldUsageKind::Filter {
                            method: method.to_string(),
                        },
                        range: method_range,
                    });
                }
            }
            _ => {
                self.intent.get_or_create_table(table_name);
            }
        }
    }

    /// Extract fields from insert table constructor
    fn extract_insert_fields(
        &mut self,
        table_name: &str,
        args_node: Node<'a>,
        usage_kind: DbFieldUsageKind,
    ) {
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if child.kind() == "table_constructor" {
                self.extract_table_constructor_fields(table_name, child, &usage_kind);
            }
        }
    }

    /// Extract fields from table constructor { field = value, ... }
    fn extract_table_constructor_fields(
        &mut self,
        table_name: &str,
        node: Node<'a>,
        usage_kind: &DbFieldUsageKind,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field" {
                self.extract_field(table_name, child, usage_kind);
            }
        }
    }

    /// Extract a single field from a field node
    fn extract_field(&mut self, table_name: &str, node: Node<'a>, usage_kind: &DbFieldUsageKind) {
        let mut field_name: Option<(String, Node)> = None;
        let mut field_type = FieldType::Unknown;
        let mut value_hint = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if field_name.is_none() => {
                    field_name = Some((self.node_text(child).to_string(), child));
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
                    value_hint = self.node_text(child).to_string();
                }
                _ => {}
            }
        }

        if let Some((name, name_node)) = field_name {
            if name == "id" {
                return;
            }

            let table = self.intent.get_or_create_table(table_name);
            table.add_field(InferredField {
                name: name.clone(),
                field_type,
                source: FieldSource::Insert {
                    value_hint: if value_hint.is_empty() {
                        name.clone()
                    } else {
                        format!("{} = {}", name, value_hint)
                    },
                },
            });
            self.intent.record_field_usage(DbFieldUsage {
                table: table_name.to_string(),
                field: name,
                kind: usage_kind.clone(),
                range: SourceRange::from_node(name_node),
            });
        }
    }

    /// Analyze dot access for field references like db.table.field
    fn analyze_dot_access(&mut self, node: Node<'a>) {
        let mut parts = Vec::new();
        self.collect_path_parts_with_nodes(node, &mut parts);
        if parts.len() < 2 {
            return;
        }
        let base = &parts[0].0;
        if !self.intent.is_db_instance(base) {
            return;
        }
        let table_name = parts[1].0.clone();
        let table_range = SourceRange::from_node(parts[1].1);
        self.intent.record_table_usage(&table_name, table_range);

        if parts.len() >= 3 {
            let field_name = parts[2].0.clone();
            let table = self.intent.get_or_create_table(&table_name);
            table.add_field(InferredField {
                name: field_name,
                field_type: FieldType::Unknown,
                source: FieldSource::Access,
            });
        }
    }

    /// Extract method info from method_index_expression
    fn extract_method_info(&self, node: Node<'a>) -> Option<(Node<'a>, String, SourceRange)> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        let object = *children.first()?;

        let method_node = children.iter().rev().find(|c| c.kind() == "identifier")?;
        let method = self.node_text(*method_node).to_string();

        Some((object, method, SourceRange::from_node(*method_node)))
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

                if let Some(base) = children.first() {
                    self.collect_path_parts(*base, parts);
                }

                for child in children.iter().rev() {
                    if child.kind() == "identifier" {
                        parts.push(self.node_text(*child).to_string());
                        break;
                    }
                }
            }
            "function_call" => {
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
                if let Some(object) = node.named_child(0) {
                    self.collect_path_parts(object, parts);
                }
            }
            _ => {}
        }
    }

    fn collect_path_parts_with_nodes(&self, node: Node<'a>, parts: &mut Vec<(String, Node<'a>)>) {
        match node.kind() {
            "identifier" => {
                parts.push((self.node_text(node).to_string(), node));
            }
            "dot_index_expression" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();

                if let Some(base) = children.first() {
                    self.collect_path_parts_with_nodes(*base, parts);
                }

                for child in children.iter().rev() {
                    if child.kind() == "identifier" {
                        parts.push((self.node_text(*child).to_string(), *child));
                        break;
                    }
                }
            }
            "function_call" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(
                        child.kind(),
                        "method_index_expression" | "dot_index_expression" | "function_call"
                    ) {
                        self.collect_path_parts_with_nodes(child, parts);
                        break;
                    }
                }
            }
            "method_index_expression" => {
                if let Some(object) = node.named_child(0) {
                    self.collect_path_parts_with_nodes(object, parts);
                }
            }
            _ => {}
        }
    }

    fn extract_table_from_node(&self, node: Node<'a>) -> Option<(String, SourceRange)> {
        let mut parts = Vec::new();
        self.collect_path_parts_with_nodes(node, &mut parts);
        if parts.len() < 2 {
            return None;
        }
        if !self.intent.is_db_instance(&parts[0].0) {
            return None;
        }
        let table = parts[1].0.clone();
        let range = SourceRange::from_node(parts[1].1);
        Some((table, range))
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

    fn extract_string_argument_fields(
        &mut self,
        table_name: &str,
        args_node: Node<'a>,
        kind: DbFieldUsageKind,
        limit: Option<usize>,
    ) {
        let mut count = 0usize;
        let mut cursor = args_node.walk();
        for child in args_node.named_children(&mut cursor) {
            if let Some(field) = self.extract_string_literal(child) {
                self.intent.record_field_usage(DbFieldUsage {
                    table: table_name.to_string(),
                    field,
                    kind: kind.clone(),
                    range: SourceRange::from_node(child),
                });
                count += 1;
                if let Some(max) = limit {
                    if count >= max {
                        break;
                    }
                }
            }
        }
    }

    fn extract_string_literal(&self, node: Node<'a>) -> Option<String> {
        if node.kind() != "string" {
            return None;
        }
        let text = self.node_text(node).trim();
        if text.len() >= 2 {
            let bytes = text.as_bytes();
            let first = bytes[0] as char;
            let last = bytes[text.len() - 1] as char;
            if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
                return Some(text[1..text.len() - 1].to_string());
            }
        }
        Some(text.to_string())
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

pub fn db_warnings_from_intent(intent: &DbIntent, schema: &DbSchema) -> Vec<ParsingError> {
    let mut warnings = Vec::new();
    let known_tables = schema.table_names();

    for (table_name, usage) in &intent.table_usages {
        if !schema.tables.contains_key(table_name) {
            let mut message = format!("No schema for table '{}'", table_name);
            if !known_tables.is_empty() {
                message.push_str(&format!(". Known tables: {}", known_tables.join(", ")));
            }
            warnings.push(ParsingError {
                message,
                function_name: None,
                range: Some(usage.range),
            });
        }
    }

    for usage in &intent.field_usages {
        let Some(table) = schema.tables.get(&usage.table) else {
            continue;
        };
        if table.fields.iter().any(|f| f == &usage.field) {
            continue;
        }
        let usage_label = match &usage.kind {
            DbFieldUsageKind::Insert => "insert".to_string(),
            DbFieldUsageKind::Set => "set".to_string(),
            DbFieldUsageKind::Select => "select".to_string(),
            DbFieldUsageKind::GroupBy => "group_by".to_string(),
            DbFieldUsageKind::OrderBy => "order_by".to_string(),
            DbFieldUsageKind::Filter { method } => format!(":{}()", method),
        };
        let mut message = format!(
            "Unknown field '{}' on table '{}' in {}",
            usage.field, usage.table, usage_label
        );
        if !table.fields.is_empty() {
            message.push_str(&format!(". Valid fields: {}", table.fields.join(", ")));
        }
        warnings.push(ParsingError {
            message,
            function_name: None,
            range: Some(usage.range),
        });
    }

    warnings
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

    #[test]
    fn test_db_instance_assignment() {
        let code = r#"
local conn = rover.db.connect { path = "rover.sqlite" }
local other = conn
other.users:insert({ name = "Ada" })
"#;
        let intent = analyze_db_intent(code);

        assert!(intent.db_instances.contains("conn"));
        assert!(intent.db_instances.contains("other"));
        assert!(intent.tables.contains_key("users"));
    }
}
