use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;
use tree_sitter::Node;

use crate::rule_runtime::{MemberKind, RuleContext, RuleEngine};
use crate::rules;
use crate::symbol::{Symbol, SymbolKind, ScopeType, SymbolTable};

#[derive(Debug, Clone)]
pub struct SemanticModel {
    pub server: Option<RoverServer>,
    pub errors: Vec<ParsingError>,
    pub functions: Vec<FunctionMetadata>,
    pub symbol_specs: HashMap<String, SymbolSpecMetadata>,
    pub dynamic_members: HashMap<String, Vec<String>>, // table_name -> [member_names]
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
    pub context_param: Option<String>,
    pub guard_bindings: Vec<GuardBinding>,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub path_params: Vec<PathParam>,
    pub query_params: Vec<QueryParam>,
    pub headers: Vec<HeaderParam>,
    pub body_schema: Option<BodySchema>,
    pub body_used: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceRange {
    pub fn from_node(node: Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        SourceRange {
            start: SourcePosition {
                line: start.row as usize,
                column: start.column as usize,
            },
            end: SourcePosition {
                line: end.row as usize,
                column: end.column as usize,
            },
        }
    }

    pub fn contains(&self, line: usize, column: usize) -> bool {
        let after_start =
            (line > self.start.line) || (line == self.start.line && column >= self.start.column);
        let before_end =
            (line < self.end.line) || (line == self.end.line && column <= self.end.column);
        after_start && before_end
    }
}

#[derive(Debug, Clone)]
pub struct ParsingError {
    pub message: String,
    pub function_name: Option<String>,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone)]
pub struct FunctionMetadata {
    pub id: FunctionId,
    pub name: String,
    pub range: SourceRange,
    pub context_param: Option<String>,
    pub param_types: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SymbolSpecMetadata {
    pub spec_id: String,
    pub doc: String,
    pub members: Vec<SymbolSpecMember>,
}

#[derive(Debug, Clone)]
pub struct SymbolSpecMember {
    pub name: String,
    pub doc: String,
    pub target_spec_id: String,
    pub kind: MemberKind,
}

#[derive(Debug, Clone)]
pub struct GuardBinding {
    pub name: String,
    pub schema: HashMap<String, GuardSchema>,
}

pub struct Analyzer {
    pub model: SemanticModel,
    pub symbol_table: SymbolTable,
    pub function_symbol_table: HashMap<String, FunctionId>,
    pub function_counter: FunctionId,
    pub app_var_name: Option<String>,
    pub current_function_name: Option<String>,
    pub current_context_param: Option<String>,
    pub current_function_index: Option<usize>,
    pub current_param_types: HashMap<String, String>,
    pub source: String,
    pub current_route: usize,
}

impl Analyzer {
    pub fn new(source: String) -> Self {
        let mut analyzer = Analyzer {
            model: SemanticModel {
                server: None,
                errors: Vec::new(),
                functions: Vec::new(),
                symbol_specs: HashMap::new(),
                dynamic_members: HashMap::new(),
            },
            symbol_table: SymbolTable::new(),
            function_symbol_table: HashMap::new(),
            function_counter: 0,
            app_var_name: None,
            current_function_name: None,
            current_context_param: None,
            current_function_index: None,
            current_param_types: HashMap::new(),
            source,
            current_route: 0,
        };

        analyzer.register_symbol_spec("rover", "rover");
        analyzer
    }

    fn register_symbol_spec<S: Into<String>>(&mut self, name: S, spec_id: &str) {
        if let Some(spec) = rules::lookup_spec(spec_id) {
            let members = spec
                .members
                .iter()
                .map(|member| SymbolSpecMember {
                    name: member.name.to_string(),
                    doc: member.doc.to_string(),
                    target_spec_id: member.target.to_string(),
                    kind: member.kind,
                })
                .collect();

            self.model.symbol_specs.insert(
                name.into(),
                SymbolSpecMetadata {
                    spec_id: spec.id.to_string(),
                    doc: spec.doc.to_string(),
                    members,
                },
            );
        }
    }

    fn rule_engine() -> &'static RuleEngine<Analyzer> {
        static RULE_ENGINE: OnceLock<RuleEngine<Analyzer>> = OnceLock::new();
        RULE_ENGINE.get_or_init(|| rules::build_rule_engine())
    }

    pub fn walk(&mut self, node: Node) {
        let matches = Self::rule_engine().apply(self, node);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }

        Self::rule_engine().finish(self, node, matches);
    }

    pub fn handle_rover_server_assignment(&mut self, node: Node) {
        if let Some(name) = self.extract_var_name_from_assignment(node) {
            self.app_var_name = Some(name.clone());
            self.model.server = Some(RoverServer {
                id: 0,
                exported: false,
                routes: Vec::new(),
            });
            self.register_symbol_spec(name, "rover_server");
        }
    }

    pub fn handle_rover_guard_assignment(&mut self, node: Node) {
        if let Some(name) = self.extract_var_name_from_assignment(node) {
            self.register_symbol_spec(name, "rover_guard");
        }
    }

    pub fn handle_potential_guard_assignment(&mut self, node: Node) {
        // Check if this assignment contains rover.guard (direct reference)
        if self.contains_rover_guard_reference(node) {
            if let Some(name) = self.extract_var_name_from_assignment(node) {
                self.register_symbol_spec(name, "rover_guard");
            }
        }
    }

    fn contains_rover_guard_reference(&self, node: Node) -> bool {
        let source = &self.source[node.start_byte()..node.end_byte()];
        source.contains("rover.guard") && !source.contains("rover.guard(")
    }

    fn extract_var_name_from_assignment(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_list" || child.kind() == "name_list" {
                let mut inner = child.walk();
                for c in child.children(&mut inner) {
                    if c.kind() == "identifier" {
                        return Some(self.source[c.start_byte()..c.end_byte()].to_string());
                    }
                }
            }
        }
        None
    }

    pub fn validate_member_access(&mut self, node: Node) {
        // Skip validation if this is a function assignment/declaration (declaring new member)
        if self.is_function_declaration_context(node) {
            return;
        }
        
        // Check for dot_index_expression (e.g., rover.something)
        if node.kind() == "dot_index_expression" {
            let access_info = self.extract_dot_access(node);
            if let Some((base, member, range)) = access_info {
                self.check_member_exists(&base, &member, range);
            }
        }
        // Check for method_index_expression (e.g., g:something())
        else if node.kind() == "method_index_expression" {
            let access_info = self.extract_method_access(node);
            if let Some((base, member, range)) = access_info {
                self.check_member_exists(&base, &member, range);
            }
        }
    }
    
    fn is_function_declaration_context(&self, node: Node) -> bool {
        // Check if we're inside a function_declaration node
        let mut current = Some(node);
        while let Some(curr) = current {
            if curr.kind() == "function_declaration" {
                // This dot_index_expression is the function name, not a member access
                return true;
            }
            // Also check assignment with function value
            if curr.kind() == "assignment_statement" {
                return self.assignment_has_function_value(curr);
            }
            current = curr.parent();
        }
        false
    }
    
    fn assignment_has_function_value(&self, assignment: Node) -> bool {
        let mut cursor = assignment.walk();
        for child in assignment.children(&mut cursor) {
            if child.kind() == "expression_list" {
                let mut expr_cursor = child.walk();
                for expr in child.children(&mut expr_cursor) {
                    if expr.kind() == "function_definition" {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    pub fn track_function_assignment(&mut self, node: Node) {
        // Called from rule when we detect function_declaration
        // Extract the dotted path (e.g., "api.users.get")
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "dot_index_expression" {
                let full_path = self.extract_dotted_name(child);
                let parts: Vec<&str> = full_path.split('.').collect();
                
                if parts.len() >= 2 {
                    let base = parts[0].to_string();
                    let member_path = parts[1..].join(".");
                    
                    self.model
                        .dynamic_members
                        .entry(base)
                        .or_insert_with(Vec::new)
                        .push(member_path);
                }
                break;
            }
        }
    }

    fn extract_dot_access(&self, node: Node) -> Option<(String, String, SourceRange)> {
        let mut cursor = node.walk();
        let mut base_node: Option<Node> = None;
        let mut member_node: Option<Node> = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if base_node.is_none() => {
                    base_node = Some(child);
                }
                "identifier" | "field" => {
                    member_node = Some(child);
                }
                _ => {}
            }
        }

        let base_n = base_node?;
        let member_n = member_node?;
        
        let base = self.source[base_n.start_byte()..base_n.end_byte()].to_string();
        let member = self.source[member_n.start_byte()..member_n.end_byte()].to_string();
        let range = SourceRange {
            start: SourcePosition {
                line: member_n.start_position().row,
                column: member_n.start_position().column,
            },
            end: SourcePosition {
                line: member_n.end_position().row,
                column: member_n.end_position().column,
            },
        };
        
        Some((base, member, range))
    }

    fn extract_method_access(&self, node: Node) -> Option<(String, String, SourceRange)> {
        let mut cursor = node.walk();
        let mut base_node: Option<Node> = None;
        let mut method_node: Option<Node> = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if base_node.is_none() => {
                    base_node = Some(child);
                }
                "identifier" | "method" => {
                    method_node = Some(child);
                }
                _ => {}
            }
        }

        let base_n = base_node?;
        let method_n = method_node?;
        
        let base = self.source[base_n.start_byte()..base_n.end_byte()].to_string();
        let method = self.source[method_n.start_byte()..method_n.end_byte()].to_string();
        let range = SourceRange {
            start: SourcePosition {
                line: method_n.start_position().row,
                column: method_n.start_position().column,
            },
            end: SourcePosition {
                line: method_n.end_position().row,
                column: method_n.end_position().column,
            },
        };
        
        Some((base, method, range))
    }

    fn check_member_exists(&mut self, base: &str, member: &str, range: SourceRange) {
        // Check if base is a known symbol
        if let Some(spec) = self.model.symbol_specs.get(base) {
            // Check if member exists in spec
            let member_exists = spec.members.iter().any(|m| m.name == member);
            
            if !member_exists {
                let valid_members: Vec<_> = spec.members.iter().map(|m| m.name.as_str()).collect();
                let suggestion = if !valid_members.is_empty() {
                    format!(" Valid members: {}", valid_members.join(", "))
                } else {
                    String::new()
                };
                
                self.model.errors.push(ParsingError {
                    message: format!(
                        "Unknown member '{}' on '{}' ({}). {}",
                        member, base, spec.spec_id, suggestion
                    ),
                    function_name: self.current_function_name.clone(),
                    range: Some(range),
                });
            }
        }
    }

    #[allow(dead_code)]
    fn get_callee_path(&self, call_node: Node) -> Option<String> {
        let mut cursor = call_node.walk();
        for child in call_node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    return Some(self.source[child.start_byte()..child.end_byte()].to_string());
                }
                "dot_index_expression" => {
                    return Some(self.extract_dotted_name(child));
                }
                "arguments" => break,
                _ => {}
            }
        }
        None
    }

    pub fn enter_handler_function(&mut self, node: Node) {
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

        let function_range = SourceRange::from_node(node);

        let handler_id = self.function_counter;
        self.function_counter += 1;
        self.function_symbol_table.insert(func_name.clone(), handler_id);
        self.current_function_name = Some(func_name.clone());

        // Extract path params from path
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
                    body_used: false,
                },
                responses: Vec::new(),
                context_param: None,
                guard_bindings: Vec::new(),
            });
            self.current_route = server.routes.len() - 1;
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

        let context_param_name = self.current_context_param.clone();
        if let Some(route) = self.current_route_mut() {
            route.context_param = context_param_name.clone();
        }
        if let Some(ref ctx_name) = self.current_context_param {
            self.register_symbol_spec(ctx_name.clone(), "ctx");
        }
        let function_index = self.model.functions.len();
        self.current_function_index = Some(function_index);
        self.current_param_types.clear();
        self.model.functions.push(FunctionMetadata {
            id: handler_id,
            name: func_name.clone(),
            range: function_range,
            context_param: context_param_name,
            param_types: HashMap::new(),
        });

        // Track context usage in the function body
    }

    pub fn exit_handler_function(&mut self) {
        self.current_function_name = None;
        self.current_context_param = None;
        self.current_function_index = None;
        self.current_param_types.clear();
    }

    pub fn handle_return(&mut self, node: Node) {
        if let Some(ref func_name) = self.current_function_name.clone() {
            self.handle_return_statement(node, func_name.clone());
        }
    }

    pub fn process_function_call(&mut self, node: Node) {
        if let Some(ref ctx_name) = self.current_context_param.clone() {
            self.handle_context_method_call(node, ctx_name);
        }
        self.inspect_guard_invocation(node);
        self.handle_assert_type(node);
    }

    fn extract_dotted_name(&self, node: Node) -> String {
        let mut parts = Vec::new();
        self.collect_dotted_parts(node, &mut parts);
        parts.join(".")
    }

    fn collect_dotted_parts(&self, node: Node, parts: &mut Vec<String>) {
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
                range: Some(SourceRange::from_node(node)),
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
        } else if source.contains("api.error") || source.contains("api:error") {
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
    fn handle_context_method_call(&mut self, node: Node, context_param: &str) {
        if node.kind() != "function_call" {
            return;
        }
        if let Some((object_node, _arguments, method_name)) = self.get_method_call_info(node) {
            if self.node_matches_context(object_node, context_param) {
                match method_name.as_str() {
                    "params" => self.handle_params_access(node),
                    "query" => self.handle_query_access(node),
                    "headers" => self.handle_headers_access(node),
                    "body" => self.handle_body_access(node),
                    _ => {}
                }
            }
        }
    }

    fn handle_assert_type(&mut self, node: Node) {
        if self.current_function_index.is_none() {
            return;
        }
        if node.kind() != "function_call" {
            return;
        }
        if !self.is_assert_invocation(node) {
            return;
        }
        let arguments = match self.find_arguments_node(node) {
            Some(args) => args,
            None => return,
        };
        let first_argument = match self.extract_first_argument(arguments) {
            Some(arg) => arg,
            None => return,
        };
        if let Some((param, ty)) = self.extract_assert_comparison(first_argument) {
            self.record_param_type(param, ty);
        }
    }

    fn is_assert_invocation(&mut self, node: Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name = &self.source[child.start_byte()..child.end_byte()];
                if name == "assert" {
                    return true;
                }
            }
            if child.kind() == "arguments" {
                break;
            }
        }
        false
    }

    fn extract_assert_comparison(&mut self, node: Node) -> Option<(String, String)> {
        if node.kind() != "binary_expression" {
            return None;
        }
        let mut left: Option<Node> = None;
        let mut right: Option<Node> = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if left.is_none() {
                    left = Some(child);
                } else if right.is_none() {
                    right = Some(child);
                }
            }
        }
        let left_node = left?;
        let right_node = right?;

        if let Some((param, ty)) = self.match_type_equals_string(left_node, right_node) {
            return Some((param, ty));
        }
        if let Some((param, ty)) = self.match_type_equals_string(right_node, left_node) {
            return Some((param, ty));
        }
        None
    }

    fn match_type_equals_string(
        &mut self,
        type_node: Node,
        string_node: Node,
    ) -> Option<(String, String)> {
        let param = self.parse_type_call_identifier(type_node)?;
        let ty = match self.extract_value(string_node) {
            Value::String(s) => s,
            _ => return None,
        };
        Some((param, ty))
    }

    fn parse_type_call_identifier(&mut self, node: Node) -> Option<String> {
        if node.kind() != "function_call" {
            return None;
        }
        let mut cursor = node.walk();
        let mut saw_type = false;
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name = &self.source[child.start_byte()..child.end_byte()];
                if name == "type" {
                    saw_type = true;
                }
            } else if child.kind() == "arguments" {
                if !saw_type {
                    return None;
                }
                let arg = self.extract_first_argument(child)?;
                if arg.kind() == "identifier" {
                    return Some(self.source[arg.start_byte()..arg.end_byte()].to_string());
                }
                return None;
            }
        }
        None
    }

    fn record_param_type(&mut self, param: String, ty: String) {
        self.current_param_types.insert(param.clone(), ty.clone());
        if let Some(index) = self.current_function_index {
            if let Some(metadata) = self.model.functions.get_mut(index) {
                metadata.param_types.insert(param, ty);
            }
        }
    }

    fn handle_params_access(&mut self, call_node: Node) {
        if let Some(field_name) = self.extract_field_name_from_call(call_node) {
            self.mark_path_param_as_used(&field_name, call_node);
        }
    }

    fn handle_query_access(&mut self, call_node: Node) {
        if let Some(field_name) = self.extract_field_name_from_call(call_node) {
            self.add_query_param(field_name);
        }
    }

    fn handle_headers_access(&mut self, call_node: Node) {
        if let Some(field_name) = self.extract_field_name_from_call(call_node) {
            self.add_header_param(field_name);
        }
    }

    fn handle_body_access(&mut self, call_node: Node) {
        if let Some(expect_call) = self.find_method_call_in_chain(call_node, "expect") {
            if let Some(body_schema) = self.parse_body_expect(expect_call) {
                self.register_guard_binding_from_call(expect_call, &body_schema.guard_defs);
                self.set_body_schema(body_schema);
            }
            self.set_body_used(true);
        } else {
            self.set_body_used(true);
        }
    }

    fn extract_field_name_from_call(&mut self, call_node: Node) -> Option<String> {
        let mut current = call_node;
        loop {
            let parent = match current.parent() {
                Some(parent) => parent,
                None => return None,
            };

            if parent.kind() == "dot_index_expression"
                && Self::is_first_named_child(parent, current)
            {
                return self.extract_field_name(parent);
            } else if parent.kind() == "bracket_index_expression"
                && Self::is_first_named_child(parent, current)
            {
                return self.extract_bracket_field_name(parent);
            } else if parent.kind() == "parenthesized_expression" {
                current = parent;
                continue;
            } else {
                return None;
            }
        }
    }

    fn node_matches_context(&self, node: Node, context_param: &str) -> bool {
        if node.kind() == "identifier" {
            let name = &self.source[node.start_byte()..node.end_byte()];
            name == context_param
        } else {
            false
        }
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

    fn mark_path_param_as_used(&mut self, param_name: &str, call_node: Node) {
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
                    message: format!(
                        "Accessing non-existent path param '{}'. Available params: {:?}",
                        param_name,
                        route
                            .request
                            .path_params
                            .iter()
                            .map(|p| &p.name)
                            .collect::<Vec<_>>()
                    ),
                    function_name: self.current_function_name.clone(),
                    range: Some(SourceRange::from_node(call_node)),
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
                                if let Some((key, guard_schema)) =
                                    self.parse_object_field(field_child)
                                {
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
            properties.insert(
                field_name.clone(),
                self.guard_schema_to_json_schema(guard_schema),
            );
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
                    props.insert(
                        field_name.clone(),
                        self.guard_schema_to_json_schema(field_schema),
                    );
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

    fn set_body_used(&mut self, used: bool) {
        if let Some(ref mut server) = self.model.server {
            if let Some(route) = server.routes.last_mut() {
                route.request.body_used = used;
            }
        }
    }

    fn current_route_mut(&mut self) -> Option<&mut Route> {
        self.model
            .server
            .as_mut()
            .and_then(|server| server.routes.get_mut(self.current_route))
    }

    fn add_guard_binding_to_current_route(&mut self, binding: GuardBinding) {
        if let Some(route) = self.current_route_mut() {
            if let Some(existing) = route
                .guard_bindings
                .iter_mut()
                .find(|b| b.name == binding.name)
            {
                *existing = binding;
            } else {
                route.guard_bindings.push(binding);
            }
        }
    }

    fn register_guard_binding_from_call(
        &mut self,
        call_node: Node,
        guard_defs: &HashMap<String, GuardSchema>,
    ) {
        if guard_defs.is_empty() {
            return;
        }
        if let Some(var_name) = self.extract_assignment_identifier(call_node) {
            self.add_guard_binding_to_current_route(GuardBinding {
                name: var_name,
                schema: guard_defs.clone(),
            });
        }
    }

    fn extract_assignment_identifier(&mut self, node: Node) -> Option<String> {
        let assignment = Self::find_enclosing_assignment(node)?;
        self.extract_identifier_from_assignment(assignment)
    }

    fn find_enclosing_assignment(node: Node) -> Option<Node> {
        let mut current = Some(node);
        while let Some(curr) = current {
            match curr.kind() {
                "assignment_statement" | "local_variable_declaration" => {
                    return Some(curr);
                }
                _ => {
                    current = curr.parent();
                }
            }
        }
        None
    }

    fn extract_identifier_from_assignment(&mut self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "variable_list" | "name_list" => {
                    return self.find_first_identifier(child);
                }
                _ => {}
            }
        }
        None
    }

    fn find_first_identifier(&mut self, node: Node) -> Option<String> {
        if node.kind() == "identifier" {
            return Some(self.source[node.start_byte()..node.end_byte()].to_string());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(name) = self.find_first_identifier(child) {
                return Some(name);
            }
        }
        None
    }

    fn inspect_guard_invocation(&mut self, node: Node) {
        if self.current_function_name.is_none() {
            return;
        }
        if let Some(guard_defs) = self.parse_rover_guard_call(node) {
            self.register_guard_binding_from_call(node, &guard_defs);
        }
    }

    fn parse_rover_guard_call(&mut self, node: Node) -> Option<HashMap<String, GuardSchema>> {
        let text = &self.source[node.start_byte()..node.end_byte()];
        if !text.contains("rover.guard") {
            return None;
        }
        let arguments = self.find_arguments_node(node)?;
        let table = self.find_table_constructor_in_arguments(arguments)?;
        let guard_defs = self.collect_guard_defs_from_table(table);
        if guard_defs.is_empty() {
            None
        } else {
            Some(guard_defs)
        }
    }

    fn collect_guard_defs_from_table(&mut self, table_node: Node) -> HashMap<String, GuardSchema> {
        let mut guard_defs = HashMap::new();
        let mut cursor = table_node.walk();
        for field_child in table_node.children(&mut cursor) {
            if field_child.kind() == "field" {
                if let Some((key, guard_schema)) = self.parse_object_field(field_child) {
                    guard_defs.insert(key, guard_schema);
                }
            }
        }
        guard_defs
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
                let has_key = field_children
                    .iter()
                    .any(|c| c.kind() == "identifier" || c.kind() == "=");
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

                self.parse_guard_modifiers(
                    node,
                    &mut guard_schema.required,
                    &mut guard_schema.default,
                    &mut guard_schema.enum_values,
                );

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

    fn parse_array_guard_type(&mut self, node: Node) -> Option<GuardType> {
        let array_call = self.find_method_call_in_chain(node, "array")?;
        let arguments = self.find_arguments_node(array_call)?;
        let inner_schema = self.find_guard_in_arguments(arguments)?;
        Some(GuardType::Array(Box::new(inner_schema)))
    }

    fn parse_object_guard_type(&mut self, node: Node) -> Option<GuardType> {
        let object_call = self.find_method_call_in_chain(node, "object")?;
        let arguments = self.find_arguments_node(object_call)?;
        let table_node = self.find_table_constructor_in_arguments(arguments)?;

        let mut properties = HashMap::new();
        let mut cursor = table_node.walk();
        for field in table_node.children(&mut cursor) {
            if field.kind() == "field" {
                if let Some((key, guard_schema)) = self.parse_object_field(field) {
                    properties.insert(key, guard_schema);
                }
            }
        }

        Some(GuardType::Object(properties))
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
                key =
                    Some(self.source[children[i].start_byte()..children[i].end_byte()].to_string());
            } else if children[i].kind() != "="
                && children[i].kind() != "identifier"
                && guard_node.is_none()
            {
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

    fn parse_guard_modifiers(
        &mut self,
        node: Node,
        required: &mut bool,
        default: &mut Option<Value>,
        enum_values: &mut Option<Vec<String>>,
    ) {
        // Only search within the immediate chain, not the entire expression tree
        self.parse_modifiers_in_node(node, required, default, enum_values);
    }

    fn parse_modifiers_in_node(
        &mut self,
        node: Node,
        required: &mut bool,
        default: &mut Option<Value>,
        enum_values: &mut Option<Vec<String>>,
    ) {
        // Check if this node is a method call with modifiers
        if node.kind() == "function_call" {
            if let Some((_object, _arguments, method_name)) = self.get_method_call_info(node) {
                match method_name.as_str() {
                    "required" => {
                        *required = true;
                        return;
                    }
                    "default" => {
                        if default.is_none() {
                            if let Some((_object, arguments, _method_name)) =
                                self.get_method_call_info(node)
                            {
                                if let Some(arg_node) = self.extract_first_argument(arguments) {
                                    *default = Some(self.extract_value(arg_node));
                                }
                            }
                        }
                        return;
                    }
                    "enum" => {
                        if enum_values.is_none() {
                            if let Some((_object, arguments, _method_name)) =
                                self.get_method_call_info(node)
                            {
                                if let Some(table_node) =
                                    self.find_table_constructor_in_arguments(arguments)
                                {
                                    let values = self.collect_string_values_from_table(table_node);
                                    *enum_values = Some(values);
                                }
                            }
                        }
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Recursively check parent if it's part of the same chain
        if let Some(parent) = node.parent() {
            if parent.kind() == "function_call" || parent.kind() == "method_index_expression" {
                self.parse_modifiers_in_node(parent, required, default, enum_values);
            }
        }
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
        let expression_root = self.find_expression_root(node);
        self.find_method_call_in_node(expression_root, method)
    }

    fn find_expression_root<'a>(&self, mut node: Node<'a>) -> Node<'a> {
        while let Some(parent) = node.parent() {
            match parent.kind() {
                "function_call"
                | "method_index_expression"
                | "arguments"
                | "parenthesized_expression" => {
                    node = parent;
                }
                _ => break,
            }
        }
        node
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

    fn extract_first_argument<'a>(&self, arguments: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = arguments.walk();
        for child in arguments.children(&mut cursor) {
            if child.is_named() {
                return Some(child);
            }
        }
        None
    }

    fn collect_string_values_from_table(&mut self, table_node: Node) -> Vec<String> {
        let mut values = Vec::new();
        let mut cursor = table_node.walk();
        for field_child in table_node.children(&mut cursor) {
            if field_child.kind() == "field" {
                let mut field_cursor = field_child.walk();
                for value_child in field_child.children(&mut field_cursor) {
                    if value_child.kind() == "string" {
                        if let Value::String(s) = self.extract_value(value_child) {
                            values.push(s);
                        }
                    }
                }
            }
        }
        values
    }

    fn is_first_named_child(parent: Node, child: Node) -> bool {
        let mut cursor = parent.walk();
        for candidate in parent.children(&mut cursor) {
            if candidate.is_named() {
                return Self::nodes_equal(candidate, child);
            }
        }
        false
    }

    fn nodes_equal(a: Node, b: Node) -> bool {
        a.start_byte() == b.start_byte() && a.end_byte() == b.end_byte()
    }

    pub fn push_scope(&mut self, scope_type: ScopeType) {
        self.symbol_table.push_scope(scope_type);
    }

    pub fn pop_scope(&mut self) {
        self.symbol_table.pop_scope();
    }

    pub fn register_variable(&mut self, name: &str, kind: SymbolKind, node: Node) {
        let range = SourceRange::from_node(node);
        let symbol = Symbol {
            name: name.to_string(),
            kind,
            range: crate::symbol::SourceRange {
                start: crate::symbol::SourcePosition {
                    line: range.start.line,
                    column: range.start.column,
                },
                end: crate::symbol::SourcePosition {
                    line: range.end.line,
                    column: range.end.column,
                },
            },
            type_annotation: None,
        };
        self.symbol_table.insert_symbol(symbol);
    }

    pub fn resolve_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbol_table.resolve_symbol(name)
    }
}

impl RuleContext for Analyzer {
    fn source(&self) -> &str {
        &self.source
    }

    fn method_name(&self, node: Node) -> Option<String> {
        self.get_method_call_info(node)
            .map(|(_, _, method)| method)
    }

    fn callee_path(&self, node: Node) -> Option<String> {
        if node.kind() != "function_call" {
            return None;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    return Some(self.source[child.start_byte()..child.end_byte()].to_string());
                }
                "dot_index_expression" => {
                    return Some(self.extract_dotted_name(child));
                }
                _ => {}
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
        parser
            .set_language(&language.into())
            .expect("Error loading Lua parser");
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

    #[test]
    fn test_guard_modifier_required() {
        let code = r#"
local g = rover.guard
local schema = g:string():required()
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let guard_call = find_method_call(&tree, &source, "string").expect("string call");
        let schema = analyzer
            .parse_guard_definition(guard_call)
            .expect("guard schema");

        assert!(schema.required);
    }

    #[test]
    fn test_guard_modifier_default_value() {
        let code = r#"
local g = rover.guard
local schema = g:string():default("light")
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let guard_call = find_method_call(&tree, &source, "string").expect("string call");
        let schema = analyzer
            .parse_guard_definition(guard_call)
            .expect("guard schema");

        assert_eq!(schema.default, Some(json!("light")));
    }

    #[test]
    fn test_guard_modifier_enum_values() {
        let code = r#"
local g = rover.guard
local schema = g:string():enum({"a", "b"})
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let guard_call = find_method_call(&tree, &source, "string").expect("string call");
        let schema = analyzer
            .parse_guard_definition(guard_call)
            .expect("guard schema");

        assert_eq!(
            schema.enum_values,
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn test_parse_guard_definition_array_schema() {
        let code = r#"
local g = rover.guard
local schema = g:array(g:integer())
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let array_call = find_method_call(&tree, &source, "array").expect("array call");
        let guard_schema = analyzer
            .parse_guard_definition(array_call)
            .expect("guard schema");

        match guard_schema.guard_type {
            GuardType::Array(inner) => assert_eq!(inner.guard_type, GuardType::Integer),
            other => panic!("expected array guard, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_guard_definition_object_schema() {
        let code = r#"
local g = rover.guard
local schema = g:object({
    name = g:string():required()
})
"#;
        let (mut analyzer, tree, source) = analyzer_fixture(code);

        let object_call = find_method_call(&tree, &source, "object").expect("object call");
        let guard_schema = analyzer
            .parse_guard_definition(object_call)
            .expect("guard schema");

        match guard_schema.guard_type {
            GuardType::Object(props) => {
                let name_field = props.get("name").expect("name field");
                assert_eq!(name_field.guard_type, GuardType::String);
            }
            other => panic!("expected object guard, got {:?}", other),
        }
    }
}
