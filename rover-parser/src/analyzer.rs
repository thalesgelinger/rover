use serde_json::{Value, json};
use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Debug, Clone)]
pub struct SemanticModel {
    pub server: Option<RoverServer>,
    pub errors: Vec<ParsingError>,
}

#[derive(Debug, Clone)]
pub struct RoverServer {
    pub id: u8,
    pub exported: bool,
    pub routes: Vec<Route>,
}

pub type FunctionId = u16;

#[derive(Debug, Clone)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub handler: FunctionId,
    pub request: Request,
    pub responses: Vec<Response>,
}

#[derive(Debug, Clone)]
pub struct Request {}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub schema: Value,
}

#[derive(Debug, Clone)]
pub struct ParsingError {
    pub message: String,
    pub function_name: Option<String>,
}

pub struct Analyzer {
    pub model: SemanticModel,
    symbol_table: HashMap<String, FunctionId>,
    function_counter: FunctionId,
    app_var_name: Option<String>,
    current_function_name: Option<String>,
    source: String,
}

impl Analyzer {
    pub fn new(source: String) -> Self {
        Analyzer {
            model: SemanticModel {
                server: None,
                errors: Vec::new(),
            },
            symbol_table: HashMap::new(),
            function_counter: 0,
            app_var_name: None,
            current_function_name: None,
            source,
        }
    }

    pub fn walk(&mut self, node: Node) {
        match node.kind() {
            "assignment_statement" => {
                self.handle_assignment(node);
            }
            "function_declaration" => {
                self.handle_function_statement(node);
            }
            "return_statement" => {
                if let Some(ref func_name) = self.current_function_name {
                    self.handle_return_statement(node, func_name.clone());
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
    }

    fn handle_assignment(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Look for pattern: identifier = function_call
        let mut var_name: Option<String> = None;
        let mut func_call_node: Option<Node> = None;

        for i in 0..children.len() {
            if children[i].kind() == "variable_list" {
                // Extract identifier from variable_list
                let mut cursor = children[i].walk();
                for child in children[i].children(&mut cursor) {
                    if child.kind() == "identifier" {
                        var_name =
                            Some(self.source[child.start_byte()..child.end_byte()].to_string());
                    }
                }
            } else if children[i].kind() == "expression_list" {
                // Look for function_call in expression_list
                let mut cursor = children[i].walk();
                for child in children[i].children(&mut cursor) {
                    if child.kind() == "function_call" {
                        func_call_node = Some(child);
                    }
                }
            }
        }

        if let (Some(name), Some(call_node)) = (var_name, func_call_node) {
            let call_source = &self.source[call_node.start_byte()..call_node.end_byte()];
            if call_source.contains("rover.server") {
                self.app_var_name = Some(name);
                self.model.server = Some(RoverServer {
                    id: 0,
                    exported: false,
                    routes: Vec::new(),
                });
            }
        }
    }

    fn handle_function_statement(&mut self, node: Node) {
        // Extract function name from dot_index_expression
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut func_name_node: Option<Node> = None;
        for child in &children {
            if child.kind() == "dot_index_expression" {
                func_name_node = Some(*child);
                break;
            }
        }

        if func_name_node.is_none() {
            return;
        }

        let func_name = self.extract_dotted_name(func_name_node.unwrap());
        if func_name.is_empty() {
            return;
        }

        if let Some(ref app_var) = self.app_var_name {
            if !func_name.starts_with(app_var) {
                return;
            }
        } else {
            return;
        }

        let parts: Vec<&str> = func_name.split('.').collect();
        if parts.len() < 2 {
            return;
        }

        let method = parts[parts.len() - 1].to_uppercase();
        let path_parts = &parts[1..parts.len() - 1];
        let path = if path_parts.is_empty() {
            "/".to_string()
        } else {
            let transformed_parts: Vec<String> = path_parts
                .iter()
                .map(|part| {
                    if part.starts_with("p_") {
                        format!("{{{}}}", &part[2..]) // p_id -> {id}
                    } else {
                        part.to_string()
                    }
                })
                .collect();
            format!("/{}", transformed_parts.join("/"))
        };

        if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
            return;
        }

        let handler_id = self.function_counter;
        self.function_counter += 1;
        self.symbol_table.insert(func_name.clone(), handler_id);

        if let Some(ref mut server) = self.model.server {
            server.routes.push(Route {
                method,
                path,
                handler: handler_id,
                request: Request {},
                responses: Vec::new(),
            });
        }

        self.current_function_name = Some(func_name);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
        self.current_function_name = None;
    }

    fn extract_dotted_name(&mut self, node: Node) -> String {
        let mut parts = Vec::new();
        self.collect_dotted_parts(node, &mut parts);
        parts.join(".")
    }

    fn collect_dotted_parts(&mut self, node: Node, parts: &mut Vec<String>) {
        match node.kind() {
            "dot_index_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "dot_index_expression" {
                        self.collect_dotted_parts(child, parts);
                    } else if child.kind() == "identifier" {
                        parts.push(self.source[child.start_byte()..child.end_byte()].to_string());
                    }
                }
            }
            "identifier" => {
                parts.push(self.source[node.start_byte()..node.end_byte()].to_string());
            }
            _ => {}
        }
    }

    fn handle_return_statement(&mut self, node: Node, func_name: String) {
        if let Some(response) = self.extract_response_from_return(node) {
            if let Some(ref mut server) = self.model.server {
                if let Some(route) = server.routes.last_mut() {
                    route.responses.push(response);
                }
            }
        } else {
            self.model.errors.push(ParsingError {
                message: "Failed to parse response".to_string(),
                function_name: Some(func_name),
            });
        }
    }

    fn extract_response_from_return(&mut self, node: Node) -> Option<Response> {
        // The return_statement has children: [return, expression_list, ...]
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "expression_list" {
                // Look for function_call in expression_list
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "function_call" {
                        return self.parse_response_call(subchild);
                    }
                }
            }
        }
        None
    }

    fn parse_response_call(&mut self, node: Node) -> Option<Response> {
        let source = &self.source[node.start_byte()..node.end_byte()];

        if source.contains("api.json") {
            self.parse_json_response(node)
        } else if source.contains("api.text") {
            self.parse_text_response()
        } else if source.contains("api.html") {
            self.parse_html_response()
        } else if source.contains("api.error") {
            self.parse_error_response()
        } else {
            None
        }
    }

    fn parse_json_response(&mut self, node: Node) -> Option<Response> {
        let mut cursor = node.walk();
        let mut schema = json!({});

        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                // Look for table_constructor inside arguments
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "table_constructor" {
                        schema = self.table_to_json_value(subchild);
                        break;
                    }
                }
            }
        }

        Some(Response {
            status: 200,
            content_type: "application/json".to_string(),
            schema,
        })
    }

    fn parse_text_response(&mut self) -> Option<Response> {
        Some(Response {
            status: 200,
            content_type: "text/plain".to_string(),
            schema: json!({}),
        })
    }

    fn parse_html_response(&mut self) -> Option<Response> {
        Some(Response {
            status: 200,
            content_type: "text/html".to_string(),
            schema: json!({}),
        })
    }

    fn parse_error_response(&mut self) -> Option<Response> {
        Some(Response {
            status: 400,
            content_type: "application/json".to_string(),
            schema: json!({"error": ""}),
        })
    }

    fn table_to_json_value(&mut self, node: Node) -> Value {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Check if this is an array-like table (has unnamed fields)
        let mut has_named_fields = false;
        let mut has_unnamed_fields = false;

        for child in &children {
            if child.kind() == "field" {
                let mut cursor = child.walk();
                let field_children: Vec<Node> = child.children(&mut cursor).collect();
                
                // Check if field has an identifier (key = value) or is just a value
                let has_key = field_children.iter().any(|c| c.kind() == "identifier" || c.kind() == "=");
                if has_key {
                    has_named_fields = true;
                } else {
                    has_unnamed_fields = true;
                }
            }
        }

        // If we have unnamed fields, treat as array
        if has_unnamed_fields && !has_named_fields {
            let mut result = Vec::new();
            
            for child in children {
                if child.kind() == "field" {
                    let mut cursor = child.walk();
                    for field_child in child.children(&mut cursor) {
                        if field_child.kind() != "," {
                            let value = self.extract_value(field_child);
                            result.push(value);
                            break;
                        }
                    }
                }
            }
            
            return json!(result);
        }

        // Otherwise treat as object with named fields
        let mut result = json!({});

        for child in children {
            if child.kind() == "field" {
                let mut cursor = child.walk();
                let field_children: Vec<Node> = child.children(&mut cursor).collect();

                let mut key: Option<String> = None;
                let mut value_node: Option<Node> = None;

                for i in 0..field_children.len() {
                    if field_children[i].kind() == "identifier" && key.is_none() {
                        key = Some(
                            self.source
                                [field_children[i].start_byte()..field_children[i].end_byte()]
                                .to_string(),
                        );
                    } else if field_children[i].kind() != "="
                        && field_children[i].kind() != "identifier"
                        && value_node.is_none()
                    {
                        value_node = Some(field_children[i]);
                    }
                }

                if let (Some(k), Some(v_node)) = (key, value_node) {
                    let value = self.extract_value(v_node);
                    if let Value::Object(ref mut obj) = result {
                        obj.insert(k, value);
                    }
                }
            }
        }

        result
    }

    fn extract_value(&mut self, node: Node) -> Value {
        match node.kind() {
            "string" => {
                // string node contains: [", string_content, "]
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "string_content" {
                        let content = self.source[child.start_byte()..child.end_byte()].to_string();
                        return json!(content);
                    }
                }
                // Fallback
                let s = self.source[node.start_byte()..node.end_byte()].to_string();
                let trimmed = s.trim_matches(|c| c == '"' || c == '\'');
                json!(trimmed)
            }
            "number" => {
                let s = &self.source[node.start_byte()..node.end_byte()];
                if let Ok(i) = s.parse::<i64>() {
                    json!(i)
                } else if let Ok(f) = s.parse::<f64>() {
                    json!(f)
                } else {
                    json!(null)
                }
            }
            "true" => json!(true),
            "false" => json!(false),
            "nil" => json!(null),
            "table_constructor" => self.table_to_json_value(node),
            _ => json!(null),
        }
    }
}
