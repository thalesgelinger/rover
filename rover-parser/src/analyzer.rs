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
pub struct Request {
    pub path_params: Vec<PathParam>,
    pub query_params: Vec<QueryParam>,
    pub headers: Vec<HeaderParam>,
    pub body_schema: Option<BodySchema>,
}

#[derive(Debug, Clone)]
pub struct PathParam {
    pub name: String,
    pub used: bool,
}

#[derive(Debug, Clone)]
pub struct QueryParam {
    pub name: String,
    pub schema: GuardSchema,
}

#[derive(Debug, Clone)]
pub struct HeaderParam {
    pub name: String,
    pub schema: GuardSchema,
}

#[derive(Debug, Clone)]
pub struct BodySchema {
    pub schema: Value,
    pub guard_defs: HashMap<String, GuardSchema>,
    pub source: ValidationSource,
}

#[derive(Debug, Clone)]
pub enum ValidationSource {
    BodyExpect,
    DirectGuard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardSchema {
    pub guard_type: GuardType,
    pub required: bool,
    pub default: Option<Value>,
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GuardType {
    String,
    Integer,
    Number,
    Boolean,
    Array(Box<GuardSchema>),
    Object(HashMap<String, GuardSchema>),
}

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
    current_context_param: Option<String>,
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
            current_context_param: None,
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
        let mut parameters_node: Option<Node> = None;
        
        for child in &children {
            if child.kind() == "dot_index_expression" {
                func_name_node = Some(*child);
            } else if child.kind() == "parameters" {
                parameters_node = Some(*child);
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

        // Extract path params from the path
        let path_params = self.extract_path_params_from_path(&path);

        if let Some(ref mut server) = self.model.server {
            server.routes.push(Route {
                method,
                path,
                handler: handler_id,
                request: Request {
                    path_params,
                    query_params: Vec::new(),
                    headers: Vec::new(),
                    body_schema: None,
                },
                responses: Vec::new(),
            });
        }

        // Extract context parameter name from parameters
        self.current_context_param = None;
        if let Some(params_node) = parameters_node {
            let mut cursor = params_node.walk();
            for child in params_node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let param_name = &self.source[child.start_byte()..child.end_byte()];
                    // This could be the context parameter - we'll detect usage patterns later
                    self.current_context_param = Some(param_name.to_string());
                    break;
                }
            }
        }

        self.current_function_name = Some(func_name);
        
        // Track context usage in the function body
        self.track_context_usage(node);
        
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
        self.current_function_name = None;
        self.current_context_param = None;
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
        let source = self.source[node.start_byte()..node.end_byte()].to_string();

        // Check for :status() modifier first
        let status = self.extract_status_code(node);

        if source.contains("api.json") {
            self.parse_json_response(node, status)
        } else if source.contains("api.text") {
            self.parse_text_response(status)
        } else if source.contains("api.html") {
            self.parse_html_response(status)
        } else if source.contains("api.error") {
            self.parse_error_response(status)
        } else {
            None
        }
    }

    fn extract_path_params_from_path(&self, path: &str) -> Vec<PathParam> {
        let mut params = Vec::new();
        let mut in_param = false;
        let mut current_param = String::new();

        for ch in path.chars() {
            match ch {
                '{' => {
                    in_param = true;
                    current_param.clear();
                }
                '}' => {
                    if in_param && !current_param.is_empty() {
                        params.push(PathParam {
                            name: current_param.clone(),
                            used: false,
                        });
                    }
                    in_param = false;
                }
                _ if in_param => {
                    current_param.push(ch);
                }
                _ => {}
            }
        }

        params
    }

    // Context tracking methods
    fn track_context_usage(&mut self, node: Node) {
        // Track context:params(), context:query(), context:headers(), context:body() usage
        let source = &self.source[node.start_byte()..node.end_byte()];
        
        // Get the current context parameter name, or skip if not in a function
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        match node.kind() {
            "function_call" | "method_index_expression" => {
                if source.contains(&format!("{}:params", context_param)) {
                    self.track_ctx_params_usage(node);
                } else if source.contains(&format!("{}:query", context_param)) {
                    self.track_ctx_query_usage(node);
                } else if source.contains(&format!("{}:headers", context_param)) {
                    self.track_ctx_headers_usage(node);
                } else if source.contains(&format!("{}:body", context_param)) {
                    self.track_ctx_body_usage(node);
                } else if source.contains("rover.guard") {
                    self.track_rover_guard_usage(node);
                }
            }
            "binary_expression" => {
                // Look for context usage in binary expressions (like string concatenation)
                if source.contains(&format!("{}:params", context_param)) {
                    self.track_ctx_params_in_binary(node);
                } else if source.contains(&format!("{}:query", context_param)) {
                    self.track_ctx_query_in_binary(node);
                } else if source.contains(&format!("{}:headers", context_param)) {
                    self.track_ctx_headers_in_binary(node);
                }
            }
            _ => {}
        }
        
        // Recursively check all children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.track_context_usage(child);
        }
    }

    fn track_ctx_params_usage(&mut self, node: Node) {
        // Pattern: context_name:params().field_name
        let source = &self.source[node.start_byte()..node.end_byte()];
        
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        if source.contains(&format!("{}:params", context_param)) && !source.contains(&format!("{}:params()", context_param)) {
            // This is context_name:params, look for the field access in the parent chain
            // We need to find the dot_index_expression that follows this
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "dot_index_expression" {
                    if let Some(field_name) = self.extract_field_name(child) {
                        self.mark_path_param_as_used(&field_name);
                    }
                }
            }
        }
    }

    fn track_ctx_params_in_binary(&mut self, node: Node) {
        // Look for context_name:params().field_name in binary expressions
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "dot_index_expression" {
                let child_source = &self.source[child.start_byte()..child.end_byte()];
                if child_source.contains(&format!("{}:params", context_param)) && child_source.contains(").") {
                    // This is context_name:params().field_name pattern
                    // Extract the field name after the closing parenthesis
                    if let Some(field_name) = self.extract_field_name_from_ctx_params(child) {
                        self.mark_path_param_as_used(&field_name);
                    }
                }
            }
        }
    }

    fn extract_field_name_from_ctx_query(&self, node: Node) -> Option<String> {
        // Extract field name from context_name:query().field_name pattern
        let source = &self.source[node.start_byte()..node.end_byte()];
        
        // Look for the pattern context_name:query().field_name
        if let Some(after_paren) = source.split("query().").nth(1) {
            // Extract the field name (everything up to the next space, operator, or end)
            let field_name = after_paren.split_whitespace().next().unwrap_or(after_paren);
            let field_name = field_name.split(|c| matches!(c, ' ' | '\n' | '\t' | '=' | '+' | '-' | '*' | '/' | '(' | ')' | '[' | ']' | ',' | ';' | ':')).next().unwrap_or(field_name);
            return Some(field_name.to_string());
        }
        
        None
    }

    fn extract_field_name_from_ctx_headers(&self, node: Node) -> Option<String> {
        // Extract field name from context_name:headers().field_name pattern
        let source = &self.source[node.start_byte()..node.end_byte()];
        
        // Look for the pattern context_name:headers().field_name
        if let Some(after_paren) = source.split("headers().").nth(1) {
            // Extract the field name (everything up to the next space, operator, or end)
            let field_name = after_paren.split_whitespace().next().unwrap_or(after_paren);
            let field_name = field_name.split(|c| matches!(c, ' ' | '\n' | '\t' | '=' | '+' | '-' | '*' | '/' | '(' | ')' | '[' | ']' | ',' | ';' | ':')).next().unwrap_or(field_name);
            return Some(field_name.to_string());
        }
        
        None
    }

    fn extract_bracket_field_name_from_ctx_headers(&self, node: Node) -> Option<String> {
        // Extract field name from context_name:headers()["field-name"] pattern
        let source = &self.source[node.start_byte()..node.end_byte()];
        
        // Look for the pattern context_name:headers()["field-name"]
        if let Some(after_paren) = source.split("headers()[").nth(1) {
            // Extract the field name between brackets
            if let Some(before_bracket) = after_paren.split(']').next() {
                let field_name = before_bracket.trim_matches('"').trim_matches('\'');
                return Some(field_name.to_string());
            }
        }
        
        None
    }

    fn extract_field_name_from_ctx_params(&mut self, node: Node) -> Option<String> {
        // Extract field name from ctx:params().field_name pattern
        let mut cursor = node.walk();
        let mut found_paren = false;
        
        for child in node.children(&mut cursor) {
            match child.kind() {
                "." => {
                    found_paren = true;
                }
                "identifier" if found_paren => {
                    return Some(self.source[child.start_byte()..child.end_byte()].to_string());
                }
                _ => {}
            }
        }
        
        None
    }

    fn track_ctx_query_in_binary(&mut self, node: Node) {
        // Look for context_name:query().field_name in binary expressions
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        let source = self.source[node.start_byte()..node.end_byte()].to_string();
        
        if source.contains(&format!("{}:query", context_param)) && source.contains(").") {
            if let Some(field_name) = self.extract_field_name_from_ctx_query(node) {
                self.add_query_param(field_name);
            }
        }
    }

    fn track_ctx_headers_in_binary(&mut self, node: Node) {
        // Look for context_name:headers().field_name or context_name:headers()["field"] in binary expressions
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        let source = self.source[node.start_byte()..node.end_byte()].to_string();
        
        // Collect field names to add, then add them after the borrow
        let mut fields_to_add = Vec::new();
        
        // Check for dot access
        if source.contains(&format!("{}:headers", context_param)) && source.contains(").") {
            if let Some(field_name) = self.extract_field_name_from_ctx_headers(node) {
                fields_to_add.push(field_name);
            }
        }
        
        // Check for bracket access
        if source.contains(&format!("{}:headers", context_param)) && source.contains(")[") {
            if let Some(field_name) = self.extract_bracket_field_name_from_ctx_headers(node) {
                fields_to_add.push(field_name);
            }
        }
        
        // Add the collected fields
        for field_name in fields_to_add {
            self.add_header_param(field_name);
        }
    }

    fn track_ctx_query_usage(&mut self, node: Node) {
        // Pattern: context_name:query().field_name
        
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        // Look for field access in the parent chain
        if let Some(parent) = node.parent() {
            let parent_source = self.source[parent.start_byte()..parent.end_byte()].to_string();
            
            // Check if parent contains dot access after context_name:query()
            if parent_source.contains(&format!("{}:query", context_param)) && parent_source.contains(").") {
                // Extract field name from the parent
                if let Some(field_name) = self.extract_field_name_from_ctx_query(parent) {
                    self.add_query_param(field_name);
                }
            }
        }
        
        // Also check children for method_index_expression
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "method_index_expression" {
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "dot_index_expression" {
                        if let Some(field_name) = self.extract_field_name(subchild) {
                            self.add_query_param(field_name);
                        }
                    }
                }
            }
        }
    }

    fn track_ctx_headers_usage(&mut self, node: Node) {
        // Patterns: context_name:headers().Authorization or context_name:headers()["user-agent"]
        
        let context_param = match &self.current_context_param {
            Some(param) => param.clone(),
            None => return,
        };
        
        // Look for field access in the parent chain
        if let Some(parent) = node.parent() {
            let parent_source = self.source[parent.start_byte()..parent.end_byte()].to_string();
            
            // Collect field names to add, then add them after the borrow
            let mut fields_to_add = Vec::new();
            
            // Check for dot access: context_name:headers().Authorization
            if parent_source.contains(&format!("{}:headers", context_param)) && parent_source.contains(").") {
                if let Some(field_name) = self.extract_field_name_from_ctx_headers(parent) {
                    fields_to_add.push(field_name);
                }
            }
            
            // Check for bracket access: context_name:headers()["user-agent"]
            if parent_source.contains(&format!("{}:headers", context_param)) && parent_source.contains(")[") {
                if let Some(field_name) = self.extract_bracket_field_name_from_ctx_headers(parent) {
                    fields_to_add.push(field_name);
                }
            }
            
            // Add the collected fields
            for field_name in fields_to_add {
                self.add_header_param(field_name);
            }
        }
        
        // Also check children for method_index_expression
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "method_index_expression" {
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "dot_index_expression" {
                        // ctx:headers().Authorization
                        if let Some(field_name) = self.extract_field_name(subchild) {
                            self.add_header_param(field_name);
                        }
                    } else if subchild.kind() == "bracket_index_expression" {
                        // ctx:headers()["user-agent"]
                        if let Some(field_name) = self.extract_bracket_field_name(subchild) {
                            self.add_header_param(field_name);
                        }
                    }
                }
            }
        }
    }

    fn track_ctx_body_usage(&mut self, node: Node) {
        // Pattern: context_name:body():expect{...}
        let source = &self.source[node.start_byte()..node.end_byte()];
        if source.contains("expect") {
            if let Some(body_schema) = self.parse_body_expect(node) {
                self.set_body_schema(body_schema);
            }
        }
    }

    fn track_rover_guard_usage(&mut self, _node: Node) {
        // Pattern: rover.guard(data, {...})
        // Track for potential future use, but don't add to request schema
        // This is for direct guard usage, not request validation
    }

    fn extract_field_name(&mut self, node: Node) -> Option<String> {
        // Extract field name from dot_index_expression
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Some(self.source[child.start_byte()..child.end_byte()].to_string());
            }
        }
        None
    }

    fn extract_bracket_field_name(&mut self, node: Node) -> Option<String> {
        // Extract field name from bracket_index_expression like ["user-agent"]
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string" {
                let value = self.extract_value(child);
                if let Value::String(s) = value {
                    return Some(s);
                }
            }
        }
        None
    }

    fn mark_path_param_as_used(&mut self, param_name: &str) {
        if let Some(ref mut server) = self.model.server {
            if let Some(route) = server.routes.last_mut() {
                for path_param in &mut route.request.path_params {
                    if path_param.name == param_name {
                        path_param.used = true;
                        return;
                    }
                }
                // Warn if accessing non-existent param
                self.model.errors.push(ParsingError {
                    message: format!("Accessing non-existent path param '{}'. Available params: {:?}", 
                        param_name, 
                        route.request.path_params.iter().map(|p| &p.name).collect::<Vec<_>>()),
                    function_name: self.current_function_name.clone(),
                });
            }
        }
    }

    fn add_query_param(&mut self, param_name: String) {
        if let Some(ref mut server) = self.model.server {
            if let Some(route) = server.routes.last_mut() {
                // Check if already exists
                for query_param in &route.request.query_params {
                    if query_param.name == param_name {
                        return;
                    }
                }
                
                // Add new query param with default string schema
                route.request.query_params.push(QueryParam {
                    name: param_name,
                    schema: GuardSchema {
                        guard_type: GuardType::String,
                        required: false,
                        default: None,
                        enum_values: None,
                    },
                });
            }
        }
    }

    fn add_header_param(&mut self, param_name: String) {
        if let Some(ref mut server) = self.model.server {
            if let Some(route) = server.routes.last_mut() {
                // Check if already exists
                for header_param in &route.request.headers {
                    if header_param.name == param_name {
                        return;
                    }
                }
                
                // Add new header param with default string schema
                route.request.headers.push(HeaderParam {
                    name: param_name,
                    schema: GuardSchema {
                        guard_type: GuardType::String,
                        required: false,
                        default: None,
                        enum_values: None,
                    },
                });
            }
        }
    }

    fn parse_body_expect(&mut self, node: Node) -> Option<BodySchema> {
        // Parse ctx:body():expect{...}
        let mut guard_defs = HashMap::new();
        let mut cursor = node.walk();
        
        // Find the table_constructor argument to expect
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "table_constructor" {
                        // Parse each field in the table
                        let mut cursor = subchild.walk();
                        for field_child in subchild.children(&mut cursor) {
                            if field_child.kind() == "field" {
                                if let Some((key, guard_schema)) = self.parse_object_field(field_child) {
                                    guard_defs.insert(key, guard_schema);
                                }
                            }
                        }
                        
                        // Convert guard definitions to JSON schema
                        let schema = self.guard_defs_to_json_schema(&guard_defs);
                        
                        return Some(BodySchema {
                            schema,
                            guard_defs,
                            source: ValidationSource::BodyExpect,
                        });
                    }
                }
            }
        }
        
        None
    }

    fn guard_defs_to_json_schema(&mut self, guard_defs: &HashMap<String, GuardSchema>) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        
        for (field_name, guard_schema) in guard_defs {
            properties.insert(field_name.clone(), self.guard_schema_to_json_schema(guard_schema));
            if guard_schema.required {
                required.push(field_name.clone());
            }
        }
        
        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }

    fn guard_schema_to_json_schema(&mut self, guard_schema: &GuardSchema) -> Value {
        let mut schema = match &guard_schema.guard_type {
            GuardType::String => json!({"type": "string"}),
            GuardType::Integer => json!({"type": "integer"}),
            GuardType::Number => json!({"type": "number"}),
            GuardType::Boolean => json!({"type": "boolean"}),
            GuardType::Array(inner_schema) => {
                json!({
                    "type": "array",
                    "items": self.guard_schema_to_json_schema(inner_schema)
                })
            }
            GuardType::Object(properties) => {
                let mut props = serde_json::Map::new();
                let mut required = Vec::new();
                
                for (field_name, field_schema) in properties {
                    props.insert(field_name.clone(), self.guard_schema_to_json_schema(field_schema));
                    if field_schema.required {
                        required.push(field_name.clone());
                    }
                }
                
                json!({
                    "type": "object",
                    "properties": props,
                    "required": required
                })
            }
        };
        
        // Add enum values if present
        if let Some(ref enum_values) = guard_schema.enum_values {
            schema["enum"] = json!(enum_values);
        }
        
        // Add default value if present
        if let Some(ref default_value) = guard_schema.default {
            schema["default"] = default_value.clone();
        }
        
        schema
    }

    fn set_body_schema(&mut self, body_schema: BodySchema) {
        if let Some(ref mut server) = self.model.server {
            if let Some(route) = server.routes.last_mut() {
                route.request.body_schema = Some(body_schema);
            }
        }
    }

    fn extract_status_code(&mut self, node: Node) -> u16 {
        // Look for :status(code, ...) modifier
        // Look for arguments node and extract the first number (status code)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                // Look for number inside arguments
                let mut cursor = child.walk();
                for arg_child in child.children(&mut cursor) {
                    if arg_child.kind() == "number" {
                        let status_str = &self.source[arg_child.start_byte()..arg_child.end_byte()];
                        if let Ok(status_code) = status_str.parse::<u16>() {
                            return status_code;
                        }
                    }
                }
            }
        }
        
        200 // Default status code
    }

    fn parse_json_response(&mut self, node: Node, status: u16) -> Option<Response> {
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
            status,
            content_type: "application/json".to_string(),
            schema,
        })
    }

    fn parse_text_response(&mut self, status: u16) -> Option<Response> {
        Some(Response {
            status,
            content_type: "text/plain".to_string(),
            schema: json!({}),
        })
    }

    fn parse_html_response(&mut self, status: u16) -> Option<Response> {
        Some(Response {
            status,
            content_type: "text/html".to_string(),
            schema: json!({}),
        })
    }

    fn parse_error_response(&mut self, status: u16) -> Option<Response> {
        Some(Response {
            status,
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

    // Guard schema parsing methods
    fn parse_guard_definition(&mut self, node: Node) -> Option<GuardSchema> {
        if let Some((object_node, _arguments, method_name)) = self.get_method_call_info(node) {
            if self.is_guard_namespace(object_node) {
                let guard_type = match method_name.as_str() {
                    "string" => GuardType::String,
                    "integer" => GuardType::Integer,
                    "number" => GuardType::Number,
                    "boolean" => GuardType::Boolean,
                    "array" => self.parse_array_guard_type(node)?,
                    "object" => self.parse_object_guard_type(node)?,
                    _ => return None,
                };

                let mut guard_schema = GuardSchema {
                    guard_type,
                    required: false,
                    default: None,
                    enum_values: None,
                };

                self.parse_guard_modifiers(node, &mut guard_schema.required, &mut guard_schema.default, &mut guard_schema.enum_values);

                return Some(guard_schema);
            }
        }

        // Some AST nodes wrap the method call (eg. function_call)
        let named_children = node.named_child_count();
        for i in 0..named_children {
            if let Some(child) = node.named_child(i.try_into().unwrap()) {
                if let Some(schema) = self.parse_guard_definition(child) {
                    return Some(schema);
                }
            }
        }

        None
    }


    fn parse_array_guard_type(&mut self, _node: Node) -> Option<GuardType> {
        None
    }

    fn parse_object_guard_type(&mut self, _node: Node) -> Option<GuardType> {
        None
    }

    fn find_guard_in_subtree(&mut self, node: Node) -> Option<GuardSchema> {
        if let Some(guard_schema) = self.parse_guard_definition(node) {
            return Some(guard_schema);
        }

        let named_children = node.named_child_count();
        for i in 0..named_children {
            if let Some(child) = node.named_child(i.try_into().unwrap()) {
                if let Some(guard_schema) = self.find_guard_in_subtree(child) {
                    return Some(guard_schema);
                }
            }
        }

        None
    }

    fn parse_object_field(&mut self, node: Node) -> Option<(String, GuardSchema)> {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut key: Option<String> = None;
        let mut guard_node: Option<Node> = None;

        for i in 0..children.len() {
            if children[i].kind() == "identifier" && key.is_none() {
                key = Some(self.source[children[i].start_byte()..children[i].end_byte()].to_string());
            } else if children[i].kind() != "=" && children[i].kind() != "identifier" && guard_node.is_none() {
                guard_node = Some(children[i]);
            }
        }

        if let (Some(k), Some(g_node)) = (key, guard_node) {
            if let Some(guard_schema) = self.parse_guard_definition(g_node) {
                return Some((k, guard_schema));
            }
        }

        None
    }

    fn parse_guard_modifiers(&mut self, node: Node, required: &mut bool, default: &mut Option<Value>, enum_values: &mut Option<Vec<String>>) {
        // Look for method calls like :required(), :default(value), :enum({...})
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "method_index_expression" {
                let source = &self.source[child.start_byte()..child.end_byte()];
                
                if source.contains("required") {
                    *required = true;
                } else if source.contains("default") {
                    // Extract default value
                    *default = self.extract_default_value(child);
                } else if source.contains("enum") {
                    // Extract enum values
                    *enum_values = self.extract_enum_values(child);
                }
            }
        }
    }

    fn extract_default_value(&mut self, node: Node) -> Option<Value> {
        // Find the argument to :default()
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() != "," {
                        return Some(self.extract_value(subchild));
                    }
                }
            }
        }
        None
    }

    fn extract_enum_values(&mut self, node: Node) -> Option<Vec<String>> {
        // Find the array argument to :enum()
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut cursor = child.walk();
                for subchild in child.children(&mut cursor) {
                    if subchild.kind() == "table_constructor" {
                        let mut values = Vec::new();
                        
                        // Parse each string in the enum array
                        let mut cursor = subchild.walk();
                        for field_child in subchild.children(&mut cursor) {
                            if field_child.kind() == "field" {
                                let mut cursor = field_child.walk();
                                for value_child in field_child.children(&mut cursor) {
                                    if value_child.kind() == "string" {
                                        let value = self.extract_value(value_child);
                                        if let Value::String(s) = value {
                                            values.push(s);
                                        }
                                    }
                                }
                            }
                        }
                        
                        return Some(values);
                    }
                }
            }
        }
        None
    }

    // AST Helper Functions for Guard Parsing
    fn is_guard_namespace(&self, node: Node) -> bool {
        // Check if node represents 'g' or 'rover.guard' identifier
        match node.kind() {
            "identifier" => {
                let text = &self.source[node.start_byte()..node.end_byte()];
                text == "g"
            }
            "dot_index_expression" => {
                // Check if this is rover.guard
                let mut cursor = node.walk();
                let mut parts = Vec::new();
                
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        parts.push(&self.source[child.start_byte()..child.end_byte()]);
                    }
                }
                
                parts.len() == 2 && parts[0] == "rover" && parts[1] == "guard"
            }
            _ => false,
        }
    }

    fn get_method_call_info<'a>(&self, node: Node<'a>) -> Option<(Node<'a>, Node<'a>, String)> {
        match node.kind() {
            "function_call" => {
                let (method_node, arguments_node) = self.extract_function_call_parts(node)?;
                let (object_node, method_name) = self.extract_method_target(method_node)?;
                Some((object_node, arguments_node, method_name))
            }
            "method_index_expression" => {
                if let Some(parent) = node.parent() {
                    if let Some(info) = self.get_method_call_info(parent) {
                        return Some(info);
                    }
                }
                None
            }
            _ => {
                // Recursively inspect children until we find a method call
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(info) = self.get_method_call_info(child) {
                        return Some(info);
                    }
                }
                None
            }
        }
    }

    fn extract_function_call_parts<'a>(&self, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
        if node.kind() != "function_call" {
            return None;
        }

        let mut cursor = node.walk();
        let mut method_node: Option<Node<'a>> = None;
        let mut arguments_node: Option<Node<'a>> = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "method_index_expression" => {
                    if method_node.is_none() {
                        method_node = Some(child);
                    }
                }
                "arguments" => {
                    if arguments_node.is_none() {
                        arguments_node = Some(child);
                    }
                }
                _ => {}
            }
        }

        if let (Some(method_node), Some(arguments_node)) = (method_node, arguments_node) {
            Some((method_node, arguments_node))
        } else {
            None
        }
    }

    fn extract_method_target<'a>(&self, node: Node<'a>) -> Option<(Node<'a>, String)> {
        if node.kind() != "method_index_expression" {
            return None;
        }

        let mut cursor = node.walk();
        let mut object_node: Option<Node<'a>> = None;
        let mut method_name: Option<String> = None;
        let mut after_colon = false;

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == ":" {
                after_colon = true;
                continue;
            }

            if !child.is_named() {
                continue;
            }

            if !after_colon {
                if object_node.is_none() {
                    object_node = Some(child);
                }
            } else if method_name.is_none() && kind == "identifier" {
                method_name = Some(self.source[child.start_byte()..child.end_byte()].to_string());
            }
        }

        if let (Some(object_node), Some(method_name)) = (object_node, method_name) {
            Some((object_node, method_name))
        } else {
            None
        }
    }

    fn find_method_call_in_chain<'a>(&self, node: Node<'a>, method: &str) -> Option<Node<'a>> {
        // Expand search to the node, its descendants, and its ancestors
        if let Some(found) = self.find_method_call_in_node(node, method) {
            return Some(found);
        }

        let mut current = node.parent();
        while let Some(parent) = current {
            if let Some(found) = self.find_method_call_in_node(parent, method) {
                return Some(found);
            }
            current = parent.parent();
        }

        None
    }

    fn find_method_call_in_node<'a>(&self, node: Node<'a>, method: &str) -> Option<Node<'a>> {
        if node.kind() == "function_call" {
            if let Some((_object, _args, method_name)) = self.get_method_call_info(node) {
                if method_name == method {
                    return Some(node);
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_method_call_in_node(child, method) {
                return Some(found);
            }
        }

        None
    }

    fn find_arguments_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        // Check if this node is already the arguments node
        if node.kind() == "arguments" {
            return Some(node);
        }

        // Walk up the parent chain to find the enclosing function_call
        let mut current = Some(node);
        while let Some(curr) = current {
            if curr.kind() == "function_call" {
                let mut cursor = curr.walk();
                for child in curr.children(&mut cursor) {
                    if child.kind() == "arguments" {
                        return Some(child);
                    }
                }
            }
            current = curr.parent();
        }

        // Fallback: search direct children for arguments (covers method chains)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                return Some(child);
            }
        }

        None
    }

    fn find_guard_in_arguments(&mut self, arguments: Node) -> Option<GuardSchema> {
        let mut cursor = arguments.walk();
        for child in arguments.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }

            if let Some(schema) = self.parse_guard_definition(child) {
                return Some(schema);
            }

            if let Some(schema) = self.find_guard_in_subtree(child) {
                return Some(schema);
            }
        }
        None
    }

    fn find_table_constructor_in_arguments<'a>(&self, arguments: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = arguments.walk();
        for child in arguments.children(&mut cursor) {
            if child.kind() == "table_constructor" {
                return Some(child);
            }

            if child.is_named() {
                if let Some(found) = self.find_table_constructor_in_node(child) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn find_table_constructor_in_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        if node.kind() == "table_constructor" {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_table_constructor_in_node(child) {
                return Some(found);
            }
        }

        None
    }
}

#[cfg(test)]
mod guard_ast_tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_lua_code(code: &str) -> (tree_sitter::Tree, String) {
        let mut parser = Parser::new();
        let language = tree_sitter_lua::LANGUAGE;
        parser.set_language(&language.into()).expect("Error loading Lua parser");
        let tree = parser.parse(code, None).unwrap();
        (tree, code.to_string())
    }

    fn analyzer_fixture(code: &str) -> (Analyzer, tree_sitter::Tree, String) {
        let (tree, source) = parse_lua_code(code);
        let analyzer = Analyzer::new(source.clone());
        (analyzer, tree, source)
    }

    fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
        &source[node.start_byte()..node.end_byte()]
    }

    fn find_method_call<'a>(
        tree: &'a tree_sitter::Tree,
        source: &'a str,
        method_name: &'a str,
    ) -> Option<Node<'a>> {
        let mut cursor = tree.walk();
        for child in tree.root_node().children(&mut cursor) {
            if let Some(node) = find_method_call_recursive(child, source, method_name) {
                return Some(node);
            }
        }
        None
    }

    fn find_method_call_recursive<'a>(
        node: Node<'a>,
        source: &'a str,
        target_method: &'a str,
    ) -> Option<Node<'a>> {
        if node.kind() == "method_index_expression" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let method = &source[child.start_byte()..child.end_byte()];
                    if method == target_method {
                        return Some(node);
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_method_call_recursive(child, source, target_method) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn test_is_guard_namespace_identifier_g() {
        let code = r#"
local g = rover.guard
local schema = g:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        if let Some(method_call) = find_method_call(&tree, &source, "string") {
            let mut cursor = method_call.walk();
            for child in method_call.children(&mut cursor) {
                if child.kind() == "identifier" {
                    assert!(
                        analyzer.is_guard_namespace(child),
                        "Should identify 'g' as guard namespace"
                    );
                    return;
                }
            }
        }
        panic!("Could not find g:string() method call");
    }

    #[test]
    fn test_is_guard_namespace_rover_guard() {
        let code = r#"
local schema = rover.guard:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        if let Some(method_call) = find_method_call(&tree, &source, "string") {
            let mut cursor = method_call.walk();
            for child in method_call.children(&mut cursor) {
                if child.kind() == "dot_index_expression" {
                    assert!(
                        analyzer.is_guard_namespace(child),
                        "Should identify 'rover.guard' as guard namespace"
                    );
                    return;
                }
            }
        }
        panic!("Could not find rover.guard:string() method call");
    }

    #[test]
    fn test_is_guard_namespace_non_guard() {
        let code = r#"
local someVar = {}
local schema = someVar:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        if let Some(method_call) = find_method_call(&tree, &source, "string") {
            let mut cursor = method_call.walk();
            for child in method_call.children(&mut cursor) {
                if child.kind() == "identifier" {
                    assert!(
                        !analyzer.is_guard_namespace(child),
                        "Should NOT identify 'someVar' as guard namespace"
                    );
                    return;
                }
            }
        }
        panic!("Could not find someVar:string() method call");
    }

    #[test]
    fn test_get_method_call_info_simple() {
        let code = r#"
local g = rover.guard
local schema = g:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "string").expect("method call");
        let (object_node, arguments_node, method_name) = analyzer
            .get_method_call_info(method_node)
            .expect("method info");

        assert_eq!(method_name, "string");
        assert_eq!(node_text(&source, object_node), "g");
        assert_eq!(arguments_node.kind(), "arguments");
    }

    #[test]
    fn test_get_method_call_info_rover_guard_object() {
        let code = r#"
local schema = rover.guard:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "string").expect("method call");
        let (object_node, _arguments_node, method_name) = analyzer
            .get_method_call_info(method_node)
            .expect("method info");

        assert_eq!(method_name, "string");
        assert_eq!(node_text(&source, object_node), "rover.guard");
    }

    #[test]
    fn test_get_method_call_info_nested_call() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:string())
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "array").expect("method call");
        let (object_node, arguments_node, method_name) = analyzer
            .get_method_call_info(method_node)
            .expect("method info");

        assert_eq!(method_name, "array");
        assert_eq!(node_text(&source, object_node), "g");
        let args_text = node_text(&source, arguments_node);
        assert!(args_text.contains("g:string"));
    }

    #[test]
    fn test_find_arguments_node_simple() {
        let code = r#"
local g = rover.guard
local schema = g:string()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "string").expect("method call");
        let arguments_node = analyzer
            .find_arguments_node(method_node)
            .expect("arguments node");

        assert_eq!(arguments_node.kind(), "arguments");
        let args_text = node_text(&source, arguments_node);
        assert!(args_text.contains("()"));
    }

    #[test]
    fn test_find_arguments_node_nested_chain() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:string()):required()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "array").expect("method call");
        let arguments_node = analyzer
            .find_arguments_node(method_node)
            .expect("arguments node");

        assert_eq!(arguments_node.kind(), "arguments");
        let args_text = node_text(&source, arguments_node);
        assert!(args_text.contains("g:string"));
    }

    #[test]
    fn test_find_method_call_in_chain_array() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:string()):required()
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let method_node = find_method_call(&tree, &source, "required").expect("method call");
        let array_call = analyzer
            .find_method_call_in_chain(method_node, "array")
            .expect("array method call");

        if let Some((_obj, _args, method_name)) = analyzer.get_method_call_info(array_call) {
            assert_eq!(method_name, "array");
        } else {
            panic!("Expected array call info");
        }
    }

    #[test]
    fn test_find_method_call_in_chain_object_inside_array() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:object({
    name = g:string()
}))
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let array_call = find_method_call(&tree, &source, "array").expect("array method call");
        let object_call = analyzer
            .find_method_call_in_chain(array_call, "object")
            .expect("object method call");

        if let Some((_obj, _args, method_name)) = analyzer.get_method_call_info(object_call) {
            assert_eq!(method_name, "object");
        } else {
            panic!("Expected object call info");
        }
    }

    #[test]
    fn test_find_guard_in_arguments_string_schema() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:string())
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let array_call = find_method_call(&tree, &source, "array").expect("array method call");
        let arguments = analyzer
            .find_arguments_node(array_call)
            .expect("arguments node");

        let guard_schema = analyzer
            .find_guard_in_arguments(arguments)
            .expect("guard schema");

        assert_eq!(guard_schema.guard_type, GuardType::String);
    }

    #[test]
    fn test_find_table_constructor_in_arguments_extracts_table() {
        let code = r#"
local g = rover.guard
local schema = g:object({
    name = g:string()
})
"#;
        let (analyzer, tree, source) = analyzer_fixture(code);

        let object_call = find_method_call(&tree, &source, "object").expect("object method call");
        let arguments = analyzer
            .find_arguments_node(object_call)
            .expect("arguments node");

        let table_node = analyzer
            .find_table_constructor_in_arguments(arguments)
            .expect("table constructor");

        let table_text = node_text(&source, table_node);
        assert!(table_text.contains("name = g:string"));
    }
}

