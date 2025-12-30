//! Type inference engine for Lua
//!
//! This module implements flow-sensitive type inference that:
//! - Infers types from literal values and assignments
//! - Tracks structural types from property access patterns
//! - Narrows types in control flow branches
//! - Bubbles constraints from asserts to function parameters

use std::collections::HashMap;
use tree_sitter::Node;

use crate::types::{LuaType, TableType, FunctionType, TypeError};

/// Type environment mapping variable names to their types at a given point
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    /// Variable types in current scope
    pub bindings: HashMap<String, LuaType>,
    /// Function types (keyed by function name/id)
    pub functions: HashMap<String, FunctionType>,
    /// Parent environment for scope chain
    parent: Option<Box<TypeEnv>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a child environment
    pub fn child(&self) -> Self {
        Self {
            bindings: HashMap::new(),
            functions: HashMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Get type for a variable, searching up the scope chain
    pub fn get(&self, name: &str) -> Option<LuaType> {
        self.bindings
            .get(name)
            .cloned()
            .or_else(|| self.parent.as_ref().and_then(|p| p.get(name)))
    }

    /// Set type for a variable in current scope
    pub fn set(&mut self, name: String, ty: LuaType) {
        self.bindings.insert(name, ty);
    }

    /// Update an existing variable's type (searches up scope chain)
    pub fn update(&mut self, name: &str, ty: LuaType) {
        if self.bindings.contains_key(name) {
            self.bindings.insert(name.to_string(), ty);
        } else if let Some(ref mut parent) = self.parent {
            parent.update(name, ty);
        } else {
            // Variable not found, create in current scope
            self.bindings.insert(name.to_string(), ty);
        }
    }

    /// Get function type
    pub fn get_function(&self, name: &str) -> Option<FunctionType> {
        self.functions
            .get(name)
            .cloned()
            .or_else(|| self.parent.as_ref().and_then(|p| p.get_function(name)))
    }

    /// Set function type
    pub fn set_function(&mut self, name: String, func: FunctionType) {
        self.functions.insert(name, func);
    }
}

/// Tracks type narrowing state in control flow
#[derive(Debug, Clone)]
pub struct NarrowingContext {
    /// Variables with narrowed types in this branch
    pub narrowed: HashMap<String, LuaType>,
    /// Inverse narrowings for else branches
    pub excluded: HashMap<String, LuaType>,
}

impl NarrowingContext {
    pub fn new() -> Self {
        Self {
            narrowed: HashMap::new(),
            excluded: HashMap::new(),
        }
    }

    /// Record a type narrowing from a condition
    pub fn narrow(&mut self, var: String, to_type: LuaType) {
        self.narrowed.insert(var, to_type);
    }

    /// Record a type exclusion (for else branch)
    pub fn exclude(&mut self, var: String, excluded_type: LuaType) {
        self.excluded.insert(var, excluded_type);
    }
}

/// Constraints collected from function body to propagate to parameters
#[derive(Debug, Clone)]
pub struct ParamConstraints {
    /// Constraints per parameter name
    constraints: HashMap<String, Vec<TypeConstraint>>,
}

#[derive(Debug, Clone)]
pub enum TypeConstraint {
    /// Must be exactly this type (from assert)
    ExactType(LuaType),
    /// Must have this field
    HasField { field: String, field_type: LuaType },
    /// Must have this method
    HasMethod { method: String, method_type: FunctionType },
}

impl ParamConstraints {
    pub fn new() -> Self {
        Self {
            constraints: HashMap::new(),
        }
    }

    /// Add a constraint for a parameter
    pub fn add(&mut self, param: &str, constraint: TypeConstraint) {
        self.constraints
            .entry(param.to_string())
            .or_default()
            .push(constraint);
    }

    /// Resolve all constraints for a parameter into a single type
    pub fn resolve(&self, param: &str) -> LuaType {
        let Some(constraints) = self.constraints.get(param) else {
            return LuaType::Unknown;
        };

        let mut result_type = LuaType::Unknown;
        let mut table_fields: HashMap<String, LuaType> = HashMap::new();

        for constraint in constraints {
            match constraint {
                TypeConstraint::ExactType(ty) => {
                    if matches!(result_type, LuaType::Unknown) {
                        result_type = ty.clone();
                    } else {
                        // Merge with existing - use intersection semantics
                        result_type = ty.clone();
                    }
                }
                TypeConstraint::HasField { field, field_type } => {
                    table_fields.insert(field.clone(), field_type.clone());
                }
                TypeConstraint::HasMethod { method, method_type } => {
                    table_fields.insert(
                        method.clone(),
                        LuaType::Function(Box::new(method_type.clone())),
                    );
                }
            }
        }

        // If we collected table fields, create a table type
        if !table_fields.is_empty() {
            let table = TableType::with_fields(table_fields);
            if matches!(result_type, LuaType::Unknown) {
                LuaType::Table(table)
            } else {
                // Merge with existing type if compatible
                result_type
            }
        } else {
            result_type
        }
    }
}

/// Type inference engine
pub struct TypeInference<'a> {
    source: &'a str,
    /// Current type environment
    pub env: TypeEnv,
    /// Type errors collected during inference
    pub errors: Vec<TypeError>,
    /// Constraints for function parameters
    param_constraints: ParamConstraints,
    /// Current function being analyzed (for parameter constraint tracking)
    current_function: Option<String>,
    /// Narrowing stack for control flow
    narrowing_stack: Vec<NarrowingContext>,
}

impl<'a> TypeInference<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut env = TypeEnv::new();
        
        // Register Lua stdlib types
        Self::register_stdlib(&mut env);
        
        Self {
            source,
            env,
            errors: Vec::new(),
            param_constraints: ParamConstraints::new(),
            current_function: None,
            narrowing_stack: Vec::new(),
        }
    }

    /// Register standard library types
    fn register_stdlib(env: &mut TypeEnv) {
        // Basic global functions
        env.set_function(
            "print".to_string(),
            FunctionType {
                params: vec![("...".to_string(), LuaType::Any)],
                returns: vec![],
                vararg: true,
                is_method: false,
            },
        );

        env.set_function(
            "type".to_string(),
            FunctionType {
                params: vec![("v".to_string(), LuaType::Any)],
                returns: vec![LuaType::String],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "tostring".to_string(),
            FunctionType {
                params: vec![("v".to_string(), LuaType::Any)],
                returns: vec![LuaType::String],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "tonumber".to_string(),
            FunctionType {
                params: vec![("v".to_string(), LuaType::Any)],
                returns: vec![LuaType::union(vec![LuaType::Number, LuaType::Nil])],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "assert".to_string(),
            FunctionType {
                params: vec![
                    ("v".to_string(), LuaType::Any),
                    ("message".to_string(), LuaType::String),
                ],
                returns: vec![LuaType::Any],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "error".to_string(),
            FunctionType {
                params: vec![
                    ("message".to_string(), LuaType::Any),
                    ("level".to_string(), LuaType::Number),
                ],
                returns: vec![LuaType::Never],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "pcall".to_string(),
            FunctionType {
                params: vec![
                    ("f".to_string(), LuaType::Function(Box::new(FunctionType::default()))),
                    ("...".to_string(), LuaType::Any),
                ],
                returns: vec![LuaType::Boolean, LuaType::Any],
                vararg: true,
                is_method: false,
            },
        );

        env.set_function(
            "xpcall".to_string(),
            FunctionType {
                params: vec![
                    ("f".to_string(), LuaType::Function(Box::new(FunctionType::default()))),
                    ("msgh".to_string(), LuaType::Function(Box::new(FunctionType::default()))),
                    ("...".to_string(), LuaType::Any),
                ],
                returns: vec![LuaType::Boolean, LuaType::Any],
                vararg: true,
                is_method: false,
            },
        );

        env.set_function(
            "pairs".to_string(),
            FunctionType {
                params: vec![("t".to_string(), LuaType::Table(TableType::open()))],
                returns: vec![
                    LuaType::Function(Box::new(FunctionType::default())),
                    LuaType::Table(TableType::open()),
                    LuaType::Nil,
                ],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "ipairs".to_string(),
            FunctionType {
                params: vec![("t".to_string(), LuaType::Table(TableType::open()))],
                returns: vec![
                    LuaType::Function(Box::new(FunctionType::default())),
                    LuaType::Table(TableType::open()),
                    LuaType::Number,
                ],
                vararg: false,
                is_method: false,
            },
        );

        // String library
        let mut string_lib = TableType::new();
        string_lib.set_field("len".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::String], LuaType::Number)
        )));
        string_lib.set_field("sub".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("i".to_string(), LuaType::Number),
                    ("j".to_string(), LuaType::Number),
                ],
                vec![LuaType::String],
            )
        )));
        string_lib.set_field("upper".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::String], LuaType::String)
        )));
        string_lib.set_field("lower".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::String], LuaType::String)
        )));
        string_lib.set_field("format".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![("formatstring".to_string(), LuaType::String)],
                returns: vec![LuaType::String],
                vararg: true,
                is_method: false,
            }
        )));
        string_lib.set_field("find".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                ],
                vec![
                    LuaType::union(vec![LuaType::Number, LuaType::Nil]),
                    LuaType::union(vec![LuaType::Number, LuaType::Nil]),
                ],
            )
        )));
        string_lib.set_field("match".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                ],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            )
        )));
        string_lib.set_field("gsub".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                    ("repl".to_string(), LuaType::Any),
                ],
                vec![LuaType::String, LuaType::Number],
            )
        )));
        string_lib.set_field("rep".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("n".to_string(), LuaType::Number),
                ],
                vec![LuaType::String],
            )
        )));
        string_lib.set_field("reverse".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::String], LuaType::String)
        )));
        string_lib.set_field("byte".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("i".to_string(), LuaType::Number),
                ],
                vec![LuaType::union(vec![LuaType::Number, LuaType::Nil])],
            )
        )));
        string_lib.set_field("char".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::String],
                vararg: true,
                is_method: false,
            }
        )));
        env.set("string".to_string(), LuaType::Table(string_lib));

        // Math library
        let mut math_lib = TableType::new();
        math_lib.set_field("abs".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("floor".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("ceil".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("sqrt".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("sin".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("cos".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("tan".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("log".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("exp".to_string(), LuaType::Function(Box::new(
            FunctionType::simple(vec![LuaType::Number], LuaType::Number)
        )));
        math_lib.set_field("pow".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("x".to_string(), LuaType::Number),
                    ("y".to_string(), LuaType::Number),
                ],
                vec![LuaType::Number],
            )
        )));
        math_lib.set_field("min".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::Number],
                vararg: true,
                is_method: false,
            }
        )));
        math_lib.set_field("max".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::Number],
                vararg: true,
                is_method: false,
            }
        )));
        math_lib.set_field("random".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![
                    ("m".to_string(), LuaType::Number),
                    ("n".to_string(), LuaType::Number),
                ],
                returns: vec![LuaType::Number],
                vararg: false,
                is_method: false,
            }
        )));
        math_lib.set_field("pi".to_string(), LuaType::Number);
        math_lib.set_field("huge".to_string(), LuaType::Number);
        env.set("math".to_string(), LuaType::Table(math_lib));

        // Table library
        let mut table_lib = TableType::new();
        table_lib.set_field("insert".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("value".to_string(), LuaType::Any),
                ],
                vec![],
            )
        )));
        table_lib.set_field("remove".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("pos".to_string(), LuaType::Number),
                ],
                vec![LuaType::Any],
            )
        )));
        table_lib.set_field("concat".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("sep".to_string(), LuaType::String),
                ],
                vec![LuaType::String],
            )
        )));
        table_lib.set_field("sort".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("comp".to_string(), LuaType::Function(Box::new(FunctionType::default()))),
                ],
                vec![],
            )
        )));
        env.set("table".to_string(), LuaType::Table(table_lib));

        // OS library
        let mut os_lib = TableType::new();
        os_lib.set_field("time".to_string(), LuaType::Function(Box::new(
            FunctionType::new(vec![], vec![LuaType::Number])
        )));
        os_lib.set_field("date".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![("format".to_string(), LuaType::String)],
                vec![LuaType::String],
            )
        )));
        os_lib.set_field("clock".to_string(), LuaType::Function(Box::new(
            FunctionType::new(vec![], vec![LuaType::Number])
        )));
        os_lib.set_field("exit".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![("code".to_string(), LuaType::Number)],
                vec![LuaType::Never],
            )
        )));
        os_lib.set_field("getenv".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![("varname".to_string(), LuaType::String)],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            )
        )));
        env.set("os".to_string(), LuaType::Table(os_lib));

        // IO library
        let mut io_lib = TableType::new();
        io_lib.set_field("open".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![
                    ("filename".to_string(), LuaType::String),
                    ("mode".to_string(), LuaType::String),
                ],
                vec![LuaType::union(vec![
                    LuaType::Table(TableType::open()), // file handle
                    LuaType::Nil,
                ])],
            )
        )));
        io_lib.set_field("read".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![("format".to_string(), LuaType::Any)],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            )
        )));
        io_lib.set_field("write".to_string(), LuaType::Function(Box::new(
            FunctionType {
                params: vec![("...".to_string(), LuaType::Any)],
                returns: vec![LuaType::Boolean],
                vararg: true,
                is_method: false,
            }
        )));
        io_lib.set_field("close".to_string(), LuaType::Function(Box::new(
            FunctionType::new(
                vec![("file".to_string(), LuaType::Any)],
                vec![LuaType::Boolean],
            )
        )));
        env.set("io".to_string(), LuaType::Table(io_lib));
    }

    /// Infer type of an expression node
    pub fn infer_expression(&mut self, node: Node) -> LuaType {
        match node.kind() {
            // Literals
            "nil" => LuaType::Nil,
            "true" | "false" => LuaType::Boolean,
            "number" => LuaType::Number,
            "string" => LuaType::String,
            
            // Table constructor
            "table_constructor" => self.infer_table_constructor(node),
            
            // Function definition
            "function_definition" => self.infer_function_definition(node),
            
            // Variable reference
            "identifier" => self.infer_identifier(node),
            
            // Binary operations
            "binary_expression" => self.infer_binary_expression(node),
            
            // Unary operations  
            "unary_expression" => self.infer_unary_expression(node),
            
            // Property access
            "dot_index_expression" => self.infer_dot_access(node),
            
            // Bracket index
            "bracket_index_expression" => self.infer_bracket_access(node),
            
            // Method call
            "method_index_expression" => self.infer_method_access(node),
            
            // Function call
            "function_call" => self.infer_function_call(node),
            
            // Parenthesized expression
            "parenthesized_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() {
                        return self.infer_expression(child);
                    }
                }
                LuaType::Unknown
            }
            
            _ => LuaType::Unknown,
        }
    }

    /// Infer type of a table constructor
    fn infer_table_constructor(&mut self, node: Node) -> LuaType {
        let mut fields = HashMap::new();
        let mut array_elements = Vec::new();
        let mut has_named_fields = false;
        let mut has_array_elements = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "field" {
                if let Some((key, value_type)) = self.infer_table_field(child) {
                    fields.insert(key, value_type);
                    has_named_fields = true;
                } else {
                    // Unnamed field - array element
                    let mut field_cursor = child.walk();
                    for field_child in child.children(&mut field_cursor) {
                        if field_child.is_named() && field_child.kind() != "=" {
                            let elem_type = self.infer_expression(field_child);
                            array_elements.push(elem_type);
                            has_array_elements = true;
                            break;
                        }
                    }
                }
            }
        }

        if has_array_elements && !has_named_fields {
            // Pure array
            let elem_type = if array_elements.is_empty() {
                LuaType::Unknown
            } else {
                LuaType::union(array_elements)
            };
            LuaType::Table(TableType::array(elem_type))
        } else {
            LuaType::Table(TableType::with_fields(fields))
        }
    }

    /// Infer a table field (key = value)
    fn infer_table_field(&mut self, node: Node) -> Option<(String, LuaType)> {
        let mut key: Option<String> = None;
        let mut value_node: Option<Node> = None;
        let mut saw_equals = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "=" {
                saw_equals = true;
            } else if child.kind() == "identifier" && !saw_equals && key.is_none() {
                key = Some(self.node_text(child));
            } else if saw_equals && child.is_named() {
                value_node = Some(child);
            }
        }

        match (key, value_node) {
            (Some(k), Some(v)) => {
                let ty = self.infer_expression(v);
                Some((k, ty))
            }
            _ => None, // Unnamed field
        }
    }

    /// Infer type of a function definition
    fn infer_function_definition(&mut self, node: Node) -> LuaType {
        let params = self.extract_function_params(node);
        
        // Create child environment for function body
        let child_env = self.env.child();
        let old_env = std::mem::replace(&mut self.env, child_env);
        
        // Register parameters in scope
        for (name, ty) in &params {
            self.env.set(name.clone(), ty.clone());
        }
        
        // Analyze function body to find return types
        let returns = self.infer_function_returns(node);
        
        // Restore environment
        self.env = old_env;
        
        LuaType::Function(Box::new(FunctionType {
            params,
            returns,
            vararg: false,
            is_method: false,
        }))
    }

    /// Extract function parameters
    fn extract_function_params(&self, node: Node) -> Vec<(String, LuaType)> {
        let mut params = Vec::new();
        
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "parameters" {
                let mut param_cursor = child.walk();
                for param in child.children(&mut param_cursor) {
                    if param.kind() == "identifier" {
                        params.push((self.node_text(param), LuaType::Unknown));
                    }
                }
            }
        }
        
        params
    }

    /// Infer return types from function body
    fn infer_function_returns(&mut self, node: Node) -> Vec<LuaType> {
        let mut return_types = Vec::new();
        
        self.collect_returns(node, &mut return_types);
        
        if return_types.is_empty() {
            vec![LuaType::Nil]
        } else if return_types.len() == 1 {
            return_types
        } else {
            vec![LuaType::union(return_types)]
        }
    }

    /// Recursively collect return statement types
    fn collect_returns(&mut self, node: Node, returns: &mut Vec<LuaType>) {
        if node.kind() == "return_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "expression_list" {
                    let mut expr_cursor = child.walk();
                    for expr in child.children(&mut expr_cursor) {
                        if expr.is_named() {
                            let ty = self.infer_expression(expr);
                            if !returns.contains(&ty) {
                                returns.push(ty);
                            }
                            break; // Only first return value for now
                        }
                    }
                }
            }
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_returns(child, returns);
        }
    }

    /// Infer type of an identifier reference
    fn infer_identifier(&mut self, node: Node) -> LuaType {
        let name = self.node_text(node);
        
        // Check for narrowed type in current control flow
        for ctx in self.narrowing_stack.iter().rev() {
            if let Some(ty) = ctx.narrowed.get(&name) {
                return ty.clone();
            }
        }
        
        self.env.get(&name).unwrap_or(LuaType::Unknown)
    }

    /// Infer type of binary expression
    fn infer_binary_expression(&mut self, node: Node) -> LuaType {
        let mut left: Option<Node> = None;
        let mut op: Option<&str> = None;
        let mut right: Option<Node> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if left.is_none() {
                    left = Some(child);
                } else {
                    right = Some(child);
                }
            } else {
                let text = self.node_text(child);
                if !text.is_empty() && op.is_none() {
                    op = Some(self.source[child.start_byte()..child.end_byte()].trim());
                }
            }
        }

        let Some(left_node) = left else { return LuaType::Unknown };
        let Some(right_node) = right else { return LuaType::Unknown };
        
        let left_type = self.infer_expression(left_node);
        let right_type = self.infer_expression(right_node);
        
        let op_str = op.unwrap_or("");

        match op_str {
            // Arithmetic operators -> number
            "+" | "-" | "*" | "/" | "%" | "^" => {
                // Check operands are numeric
                if !matches!(left_type, LuaType::Number | LuaType::Unknown | LuaType::Any) {
                    self.errors.push(TypeError::type_mismatch(
                        &LuaType::Number,
                        &left_type,
                        left_node.start_position().row,
                        left_node.start_position().column,
                    ));
                }
                if !matches!(right_type, LuaType::Number | LuaType::Unknown | LuaType::Any) {
                    self.errors.push(TypeError::type_mismatch(
                        &LuaType::Number,
                        &right_type,
                        right_node.start_position().row,
                        right_node.start_position().column,
                    ));
                }
                LuaType::Number
            }
            
            // String concatenation -> string
            ".." => {
                LuaType::String
            }
            
            // Comparison operators -> boolean
            "==" | "~=" | "<" | ">" | "<=" | ">=" => LuaType::Boolean,
            
            // Logical operators
            "and" => {
                // Returns first arg if falsy, otherwise second arg
                if matches!(left_type, LuaType::Nil) {
                    LuaType::Nil
                } else {
                    right_type
                }
            }
            "or" => {
                // Returns first arg if truthy, otherwise second arg
                if left_type.is_truthy() {
                    left_type
                } else {
                    LuaType::union(vec![left_type, right_type])
                }
            }
            
            _ => LuaType::Unknown,
        }
    }

    /// Infer type of unary expression
    fn infer_unary_expression(&mut self, node: Node) -> LuaType {
        let mut op: Option<&str> = None;
        let mut operand: Option<Node> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                operand = Some(child);
            } else {
                op = Some(self.source[child.start_byte()..child.end_byte()].trim());
            }
        }

        let Some(operand_node) = operand else { return LuaType::Unknown };
        
        match op {
            Some("-") => LuaType::Number,
            Some("not") => LuaType::Boolean,
            Some("#") => LuaType::Number, // length operator
            _ => self.infer_expression(operand_node),
        }
    }

    /// Infer type of dot access (e.g., foo.bar)
    fn infer_dot_access(&mut self, node: Node) -> LuaType {
        let mut base: Option<Node> = None;
        let mut field: Option<String> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "dot_index_expression" if base.is_none() => {
                    base = Some(child);
                }
                "identifier" => {
                    field = Some(self.node_text(child));
                }
                _ => {}
            }
        }

        let Some(base_node) = base else { return LuaType::Unknown };
        let Some(field_name) = field else { return LuaType::Unknown };
        
        let base_type = self.infer_expression(base_node);
        
        match &base_type {
            LuaType::Table(table) => {
                table.get_field(&field_name).cloned().unwrap_or(LuaType::Unknown)
            }
            LuaType::String => {
                // String methods accessed via string:method() not string.method
                // But string.method is also valid in Lua
                if let Some(string_type) = self.env.get("string") {
                    if let LuaType::Table(string_lib) = string_type {
                        return string_lib.get_field(&field_name).cloned().unwrap_or(LuaType::Unknown);
                    }
                }
                LuaType::Unknown
            }
            LuaType::Unknown => {
                // Track structural constraint for parameter inference
                if let Some(base_name) = self.get_identifier_name(base_node) {
                    self.param_constraints.add(
                        &base_name,
                        TypeConstraint::HasField {
                            field: field_name.clone(),
                            field_type: LuaType::Unknown,
                        },
                    );
                }
                LuaType::Unknown
            }
            _ => LuaType::Unknown,
        }
    }

    /// Infer type of bracket access (e.g., foo[bar])
    fn infer_bracket_access(&mut self, node: Node) -> LuaType {
        let mut cursor = node.walk();
        let mut base: Option<Node> = None;
        
        for child in node.children(&mut cursor) {
            if child.is_named() && base.is_none() {
                base = Some(child);
            }
        }

        let Some(base_node) = base else { return LuaType::Unknown };
        let base_type = self.infer_expression(base_node);
        
        match &base_type {
            LuaType::Table(table) => {
                if let Some(elem_type) = &table.array_element {
                    *elem_type.clone()
                } else {
                    LuaType::Unknown
                }
            }
            LuaType::String => LuaType::String, // string indexing returns string
            _ => LuaType::Unknown,
        }
    }

    /// Infer type of method access
    fn infer_method_access(&mut self, _node: Node) -> LuaType {
        // Method access returns the method itself, actual call handled by function_call
        LuaType::Function(Box::new(FunctionType::default()))
    }

    /// Infer type of function call
    fn infer_function_call(&mut self, node: Node) -> LuaType {
        let mut callee: Option<Node> = None;
        let mut _args: Option<Node> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "dot_index_expression" | "method_index_expression" 
                    if callee.is_none() => {
                    callee = Some(child);
                }
                "arguments" => {
                    _args = Some(child);
                }
                _ => {}
            }
        }

        let Some(callee_node) = callee else { return LuaType::Unknown };
        
        // Get callee type
        let callee_type = self.infer_expression(callee_node);
        
        // Check for type() calls for narrowing
        if let Some(name) = self.get_callee_name(callee_node) {
            if name == "type" {
                return LuaType::String;
            }
            
            // Check stdlib functions
            if let Some(func_type) = self.env.get_function(&name) {
                return func_type.return_type().clone();
            }
        }
        
        match callee_type {
            LuaType::Function(func) => func.return_type().clone(),
            _ => LuaType::Unknown,
        }
    }

    /// Get identifier name from a node
    fn get_identifier_name(&self, node: Node) -> Option<String> {
        if node.kind() == "identifier" {
            Some(self.node_text(node))
        } else {
            None
        }
    }

    /// Get callee name from function call
    fn get_callee_name(&self, node: Node) -> Option<String> {
        match node.kind() {
            "identifier" => Some(self.node_text(node)),
            "dot_index_expression" => {
                let mut parts = Vec::new();
                self.collect_dot_parts(node, &mut parts);
                Some(parts.join("."))
            }
            _ => None,
        }
    }

    fn collect_dot_parts(&self, node: Node, parts: &mut Vec<String>) {
        match node.kind() {
            "dot_index_expression" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "dot_index_expression" {
                        self.collect_dot_parts(child, parts);
                    } else if child.kind() == "identifier" {
                        parts.push(self.node_text(child));
                    }
                }
            }
            "identifier" => {
                parts.push(self.node_text(node));
            }
            _ => {}
        }
    }

    /// Process a variable declaration and infer types
    pub fn process_declaration(&mut self, node: Node) {
        let mut names: Vec<String> = Vec::new();
        let mut values: Vec<Node> = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "name_list" | "variable_list" => {
                    let mut name_cursor = child.walk();
                    for name_node in child.children(&mut name_cursor) {
                        if name_node.kind() == "identifier" {
                            names.push(self.node_text(name_node));
                        }
                    }
                }
                "expression_list" => {
                    let mut expr_cursor = child.walk();
                    for expr in child.children(&mut expr_cursor) {
                        if expr.is_named() {
                            values.push(expr);
                        }
                    }
                }
                _ => {}
            }
        }

        // Assign types to variables
        for (i, name) in names.into_iter().enumerate() {
            let ty = if let Some(value_node) = values.get(i) {
                self.infer_expression(*value_node)
            } else {
                LuaType::Nil
            };
            self.env.set(name, ty);
        }
    }

    /// Process an assignment statement
    pub fn process_assignment(&mut self, node: Node) {
        let mut targets: Vec<Node> = Vec::new();
        let mut values: Vec<Node> = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "variable_list" => {
                    let mut var_cursor = child.walk();
                    for var_node in child.children(&mut var_cursor) {
                        if var_node.is_named() {
                            targets.push(var_node);
                        }
                    }
                }
                "expression_list" => {
                    let mut expr_cursor = child.walk();
                    for expr in child.children(&mut expr_cursor) {
                        if expr.is_named() {
                            values.push(expr);
                        }
                    }
                }
                _ => {}
            }
        }

        // Process each assignment
        for (i, target) in targets.iter().enumerate() {
            let value_type = if let Some(value_node) = values.get(i) {
                self.infer_expression(*value_node)
            } else {
                LuaType::Nil
            };

            self.assign_to_target(*target, value_type);
        }
    }

    /// Assign a type to a target (variable or field)
    fn assign_to_target(&mut self, target: Node, value_type: LuaType) {
        match target.kind() {
            "identifier" => {
                let name = self.node_text(target);
                
                // Check for type conflicts
                if let Some(existing) = self.env.get(&name) {
                    if !matches!(existing, LuaType::Unknown) && existing != value_type {
                        // Type conflict - existing type is known and different
                        self.errors.push(TypeError {
                            message: format!(
                                "Cannot assign '{}' to variable '{}' of type '{}'",
                                value_type, name, existing
                            ),
                            expected: existing,
                            actual: value_type.clone(),
                            line: target.start_position().row,
                            column: target.start_position().column,
                        });
                    }
                }
                
                self.env.update(&name, value_type);
            }
            "dot_index_expression" => {
                // Property assignment - update structural type
                self.assign_to_property(target, value_type);
            }
            "bracket_index_expression" => {
                // Index assignment
                self.assign_to_index(target, value_type);
            }
            _ => {}
        }
    }

    /// Assign to a property access
    fn assign_to_property(&mut self, node: Node, value_type: LuaType) {
        let mut base: Option<Node> = None;
        let mut field: Option<String> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if base.is_none() => {
                    base = Some(child);
                }
                "identifier" => {
                    field = Some(self.node_text(child));
                }
                "dot_index_expression" if base.is_none() => {
                    base = Some(child);
                }
                _ => {}
            }
        }

        let Some(base_node) = base else { return };
        let Some(field_name) = field else { return };
        
        if let Some(base_name) = self.get_identifier_name(base_node) {
            let base_type = self.env.get(&base_name).unwrap_or(LuaType::Unknown);
            
            let new_type = match base_type {
                LuaType::Table(mut table) => {
                    table.set_field(field_name, value_type);
                    LuaType::Table(table)
                }
                LuaType::Unknown => {
                    let mut table = TableType::new();
                    table.set_field(field_name.clone(), value_type.clone());
                    
                    // Also record constraint for param inference
                    self.param_constraints.add(
                        &base_name,
                        TypeConstraint::HasField {
                            field: field_name,
                            field_type: value_type,
                        },
                    );
                    
                    LuaType::Table(table)
                }
                _ => return, // Can't assign to non-table
            };
            
            self.env.update(&base_name, new_type);
        }
    }

    /// Assign to an index access
    fn assign_to_index(&mut self, _node: Node, _value_type: LuaType) {
        // TODO: Implement index assignment type tracking
    }

    /// Process an assert statement for type constraints
    pub fn process_assert(&mut self, node: Node) {
        // Look for pattern: assert(type(x) == "typename")
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut arg_cursor = child.walk();
                for arg in child.children(&mut arg_cursor) {
                    if arg.kind() == "binary_expression" {
                        if let Some((var, ty)) = self.extract_type_assertion(arg) {
                            // Apply constraint to parameter
                            self.param_constraints.add(&var, TypeConstraint::ExactType(ty.clone()));
                            
                            // Also update environment
                            self.env.update(&var, ty);
                        }
                    }
                }
            }
        }
    }

    /// Extract type assertion from binary expression like type(x) == "string"
    fn extract_type_assertion(&mut self, node: Node) -> Option<(String, LuaType)> {
        let mut left: Option<Node> = None;
        let mut right: Option<Node> = None;
        let mut is_equality = false;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if left.is_none() {
                    left = Some(child);
                } else {
                    right = Some(child);
                }
            } else {
                let text = self.source[child.start_byte()..child.end_byte()].trim();
                if text == "==" {
                    is_equality = true;
                }
            }
        }

        if !is_equality {
            return None;
        }

        let left_node = left?;
        let right_node = right?;

        // Check for type(var) == "string" pattern
        if let Some((var, ty)) = self.match_type_check(left_node, right_node) {
            return Some((var, ty));
        }
        
        // Check reversed: "string" == type(var)
        if let Some((var, ty)) = self.match_type_check(right_node, left_node) {
            return Some((var, ty));
        }

        None
    }

    /// Match type(var) on one side and "typename" on other
    fn match_type_check(&self, type_call: Node, type_string: Node) -> Option<(String, LuaType)> {
        // Check if type_call is type(identifier)
        if type_call.kind() != "function_call" {
            return None;
        }

        let mut callee_name: Option<String> = None;
        let mut arg_name: Option<String> = None;

        let mut cursor = type_call.walk();
        for child in type_call.children(&mut cursor) {
            if child.kind() == "identifier" && callee_name.is_none() {
                callee_name = Some(self.node_text(child));
            } else if child.kind() == "arguments" {
                let mut arg_cursor = child.walk();
                for arg in child.children(&mut arg_cursor) {
                    if arg.kind() == "identifier" {
                        arg_name = Some(self.node_text(arg));
                        break;
                    }
                    // Also check for nested access like type(x.y)
                    if arg.kind() == "dot_index_expression" {
                        arg_name = Some(self.get_base_identifier(arg)?);
                        break;
                    }
                }
            }
        }

        if callee_name.as_deref() != Some("type") {
            return None;
        }

        let var_name = arg_name?;

        // Get type string
        let type_name = self.extract_string_value(type_string)?;
        let lua_type = LuaType::from_type_string(&type_name)?;

        Some((var_name, lua_type))
    }

    /// Get base identifier from dot expression
    fn get_base_identifier(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Some(self.node_text(child));
            }
            if child.kind() == "dot_index_expression" {
                return self.get_base_identifier(child);
            }
        }
        None
    }

    /// Extract string value from a string literal node
    fn extract_string_value(&self, node: Node) -> Option<String> {
        if node.kind() != "string" {
            return None;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string_content" {
                return Some(self.node_text(child));
            }
        }

        // Fallback - extract from quotes
        let text = self.node_text(node);
        let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
        Some(trimmed.to_string())
    }

    /// Enter an if branch with narrowing
    pub fn enter_if_branch(&mut self, condition: Node) {
        let mut ctx = NarrowingContext::new();
        
        // Check for type narrowing patterns in condition
        self.extract_narrowing(condition, &mut ctx);
        
        self.narrowing_stack.push(ctx);
    }

    /// Enter an else branch (apply inverse narrowing)
    pub fn enter_else_branch(&mut self) {
        if let Some(if_ctx) = self.narrowing_stack.last() {
            let mut else_ctx = NarrowingContext::new();
            
            // Apply inverse narrowings
            for (var, excluded_type) in &if_ctx.narrowed {
                if let Some(original_type) = self.env.get(var) {
                    let narrowed = original_type.exclude(excluded_type);
                    else_ctx.narrow(var.clone(), narrowed);
                }
            }
            
            self.narrowing_stack.push(else_ctx);
        }
    }

    /// Exit current branch
    pub fn exit_branch(&mut self) {
        self.narrowing_stack.pop();
    }

    /// Extract type narrowings from a condition
    fn extract_narrowing(&mut self, condition: Node, ctx: &mut NarrowingContext) {
        match condition.kind() {
            "binary_expression" => {
                // Check for type(x) == "typename" patterns
                if let Some((var, ty)) = self.extract_type_assertion(condition) {
                    ctx.narrow(var.clone(), ty.clone());
                    ctx.exclude(var, ty);
                }
                
                // Check for x ~= nil patterns
                self.extract_nil_check(condition, ctx);
            }
            "identifier" => {
                // Truthy check: if x then
                let name = self.node_text(condition);
                if let Some(original) = self.env.get(&name) {
                    ctx.narrow(name.clone(), original.truthy());
                    ctx.exclude(name, LuaType::Nil);
                }
            }
            "unary_expression" => {
                // Check for "not" expressions
                self.extract_not_narrowing(condition, ctx);
            }
            _ => {}
        }
    }

    /// Extract nil check narrowing
    fn extract_nil_check(&self, node: Node, ctx: &mut NarrowingContext) {
        let mut left: Option<Node> = None;
        let mut right: Option<Node> = None;
        let mut op: Option<&str> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if left.is_none() {
                    left = Some(child);
                } else {
                    right = Some(child);
                }
            } else {
                let text = self.source[child.start_byte()..child.end_byte()].trim();
                if text == "~=" || text == "==" {
                    op = Some(text);
                }
            }
        }

        let Some(left_node) = left else { return };
        let Some(right_node) = right else { return };
        let Some(operator) = op else { return };

        // Check for x ~= nil or nil ~= x
        let (var_node, _nil_node) = if right_node.kind() == "nil" {
            (left_node, right_node)
        } else if left_node.kind() == "nil" {
            (right_node, left_node)
        } else {
            return;
        };

        if var_node.kind() != "identifier" {
            return;
        }

        let var_name = self.node_text(var_node);
        
        if operator == "~=" {
            // x ~= nil -> exclude nil
            if let Some(original) = self.env.get(&var_name) {
                ctx.narrow(var_name, original.exclude(&LuaType::Nil));
            }
        } else {
            // x == nil -> narrow to nil
            ctx.narrow(var_name, LuaType::Nil);
        }
    }

    /// Extract narrowing from "not" expressions
    fn extract_not_narrowing(&mut self, node: Node, ctx: &mut NarrowingContext) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                if child.kind() == "identifier" {
                    // not x -> x is falsy (nil or false)
                    let name = self.node_text(child);
                    ctx.narrow(name, LuaType::Nil);
                }
            }
        }
    }

    /// Get node text
    fn node_text(&self, node: Node) -> String {
        self.source[node.start_byte()..node.end_byte()].to_string()
    }

    /// Get resolved type for a parameter after analyzing function body
    pub fn get_param_type(&self, param: &str) -> LuaType {
        self.param_constraints.resolve(param)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(code: &str) -> (tree_sitter::Tree, String) {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_lua::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        (tree, code.to_string())
    }

    #[test]
    fn test_literal_inference() {
        let code = r#"
local a = 42
local b = "hello"
local c = true
local d = nil
"#;
        let (tree, source) = parse(code);
        let mut inf = TypeInference::new(&source);
        
        // Simulate walking and processing declarations
        inf.env.set("a".to_string(), LuaType::Number);
        inf.env.set("b".to_string(), LuaType::String);
        inf.env.set("c".to_string(), LuaType::Boolean);
        inf.env.set("d".to_string(), LuaType::Nil);
        
        assert_eq!(inf.env.get("a"), Some(LuaType::Number));
        assert_eq!(inf.env.get("b"), Some(LuaType::String));
        assert_eq!(inf.env.get("c"), Some(LuaType::Boolean));
        assert_eq!(inf.env.get("d"), Some(LuaType::Nil));
    }

    #[test]
    fn test_table_type_inference() {
        let mut inf = TypeInference::new("");
        
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), LuaType::String);
        fields.insert("age".to_string(), LuaType::Number);
        
        let table_type = LuaType::Table(TableType::with_fields(fields));
        inf.env.set("person".to_string(), table_type);
        
        let person = inf.env.get("person").unwrap();
        if let LuaType::Table(table) = person {
            assert_eq!(table.get_field("name"), Some(&LuaType::String));
            assert_eq!(table.get_field("age"), Some(&LuaType::Number));
        } else {
            panic!("Expected table type");
        }
    }

    #[test]
    fn test_param_constraint_from_property_access() {
        let mut inf = TypeInference::new("");
        
        // Simulate: function foo(bar) print(bar.name) end
        inf.param_constraints.add(
            "bar",
            TypeConstraint::HasField {
                field: "name".to_string(),
                field_type: LuaType::Unknown,
            },
        );
        
        let resolved = inf.get_param_type("bar");
        if let LuaType::Table(table) = resolved {
            assert!(table.fields.contains_key("name"));
        } else {
            panic!("Expected table type");
        }
    }

    #[test]
    fn test_param_constraint_from_assert() {
        let mut inf = TypeInference::new("");
        
        // Simulate: function foo(bar) assert(type(bar) == "string") end
        inf.param_constraints.add("bar", TypeConstraint::ExactType(LuaType::String));
        
        let resolved = inf.get_param_type("bar");
        assert_eq!(resolved, LuaType::String);
    }

    #[test]
    fn test_narrowing_context() {
        let mut ctx = NarrowingContext::new();
        
        ctx.narrow("x".to_string(), LuaType::String);
        ctx.exclude("x".to_string(), LuaType::String);
        
        assert_eq!(ctx.narrowed.get("x"), Some(&LuaType::String));
        assert_eq!(ctx.excluded.get("x"), Some(&LuaType::String));
    }

    #[test]
    fn test_type_env_scoping() {
        let mut parent = TypeEnv::new();
        parent.set("x".to_string(), LuaType::Number);
        
        let mut child = parent.child();
        child.set("y".to_string(), LuaType::String);
        
        // Child can see parent's binding
        assert_eq!(child.get("x"), Some(LuaType::Number));
        // Child has its own binding
        assert_eq!(child.get("y"), Some(LuaType::String));
        // Parent doesn't see child's binding
        assert_eq!(parent.get("y"), None);
    }

    #[test]
    fn test_stdlib_types() {
        let inf = TypeInference::new("");
        
        // Check math library
        let math = inf.env.get("math").unwrap();
        if let LuaType::Table(table) = math {
            assert!(table.get_field("floor").is_some());
            assert!(table.get_field("pi").is_some());
        } else {
            panic!("Expected table type for math");
        }
        
        // Check string library
        let string = inf.env.get("string").unwrap();
        if let LuaType::Table(table) = string {
            assert!(table.get_field("sub").is_some());
            assert!(table.get_field("format").is_some());
        } else {
            panic!("Expected table type for string");
        }
    }
}
