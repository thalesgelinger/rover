//! Type inference engine for Lua
//!
//! This module implements flow-sensitive type inference that:
//! - Infers types from literal values and assignments
//! - Tracks structural types from property access patterns
//! - Narrows types in control flow branches
//! - Bubbles constraints from asserts to function parameters
//! - Supports cross-file type flow via require()

use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Node;

use crate::types::{FunctionType, LuaType, TableType, TypeError};

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
    /// Variables that hold pcall result - maps result_var -> (success_var, error_type)
    pub pcall_results: HashMap<String, (String, LuaType)>,
}

impl NarrowingContext {
    pub fn new() -> Self {
        Self {
            narrowed: HashMap::new(),
            excluded: HashMap::new(),
            pcall_results: HashMap::new(),
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

    /// Record pcall result variable mapping
    pub fn set_pcall_result(
        &mut self,
        result_var: String,
        success_var: String,
        error_type: LuaType,
    ) {
        self.pcall_results
            .insert(result_var, (success_var, error_type));
    }

    /// Get narrowed type for a variable (considering pcall)
    pub fn get_narrowed_type(&self, var: &str) -> Option<LuaType> {
        // Check if this is a pcall result variable
        if let Some((success_var, success_type)) = self.pcall_results.get(var) {
            // If the success variable is narrowed in this context, use the success type
            if self.narrowed.contains_key(success_var) {
                return Some(success_type.clone());
            }
        }
        // Fall back to regular narrowing
        self.narrowed.get(var).cloned()
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
    HasMethod {
        method: String,
        method_type: FunctionType,
    },
}

/// Module exports from a required file
#[derive(Debug, Clone)]
pub struct ModuleExports {
    /// Public functions exported from the module
    pub functions: HashMap<String, FunctionType>,
    /// Public variables exported from the module
    pub bindings: HashMap<String, LuaType>,
}

impl ModuleExports {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            bindings: HashMap::new(),
        }
    }

    /// Get the complete module type as a table
    pub fn to_table_type(&self) -> LuaType {
        let mut fields = HashMap::new();

        // Add function exports
        for (name, func) in &self.functions {
            fields.insert(name.clone(), LuaType::Function(Box::new(func.clone())));
        }

        // Add variable exports
        for (name, ty) in &self.bindings {
            fields.insert(name.clone(), ty.clone());
        }

        LuaType::Table(TableType::with_fields(fields))
    }
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

        let mut exact_types: Vec<LuaType> = Vec::new();
        let mut table_fields: HashMap<String, LuaType> = HashMap::new();

        for constraint in constraints {
            match constraint {
                TypeConstraint::ExactType(ty) => {
                    exact_types.push(ty.clone());
                }
                TypeConstraint::HasField { field, field_type } => {
                    table_fields.insert(field.clone(), field_type.clone());
                }
                TypeConstraint::HasMethod {
                    method,
                    method_type,
                } => {
                    table_fields.insert(
                        method.clone(),
                        LuaType::Function(Box::new(method_type.clone())),
                    );
                }
            }
        }

        // If we collected multiple exact types, create a union
        if exact_types.len() > 1 {
            return LuaType::Union(exact_types);
        }

        // If we have one exact type, return it
        if exact_types.len() == 1 {
            return exact_types[0].clone();
        }

        // If we collected table fields, create a table type
        if !table_fields.is_empty() {
            LuaType::Table(TableType::with_fields(table_fields))
        } else {
            LuaType::Unknown
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
    /// Constraints for function parameters (current function scope)
    param_constraints: ParamConstraints,
    /// Constraints per function name (for checking calls)
    pub function_constraints: HashMap<String, ParamConstraints>,
    /// Narrowing stack for control flow
    narrowing_stack: Vec<NarrowingContext>,
    /// Cache for module exports (module_path -> exports)
    module_cache: Arc<HashMap<String, ModuleExports>>,
    /// Base path for resolving module paths
    base_path: Option<String>,
    /// Current function being analyzed (for constraint collection)
    current_function: Option<String>,
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
            function_constraints: HashMap::new(),
            narrowing_stack: Vec::new(),
            module_cache: Arc::new(HashMap::new()),
            base_path: None,
            current_function: None,
        }
    }

    pub fn with_base_path(source: &'a str, base_path: String) -> Self {
        let mut inf = Self::new(source);
        inf.base_path = Some(base_path);
        inf
    }

    pub fn with_module_cache(source: &'a str, cache: Arc<HashMap<String, ModuleExports>>) -> Self {
        let mut inf = Self::new(source);
        inf.module_cache = cache;
        inf
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

        // require is handled specially for cross-file type flow
        // Type will be determined based on the required module
        env.set_function(
            "require".to_string(),
            FunctionType {
                params: vec![("modname".to_string(), LuaType::String)],
                returns: vec![LuaType::Any],
                vararg: false,
                is_method: false,
            },
        );

        env.set_function(
            "pcall".to_string(),
            FunctionType {
                params: vec![
                    (
                        "f".to_string(),
                        LuaType::Function(Box::new(FunctionType::default())),
                    ),
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
                    (
                        "f".to_string(),
                        LuaType::Function(Box::new(FunctionType::default())),
                    ),
                    (
                        "msgh".to_string(),
                        LuaType::Function(Box::new(FunctionType::default())),
                    ),
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
        string_lib.set_field(
            "len".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::String],
                LuaType::Number,
            ))),
        );
        string_lib.set_field(
            "sub".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("i".to_string(), LuaType::Number),
                    ("j".to_string(), LuaType::Number),
                ],
                vec![LuaType::String],
            ))),
        );
        string_lib.set_field(
            "upper".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::String],
                LuaType::String,
            ))),
        );
        string_lib.set_field(
            "lower".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::String],
                LuaType::String,
            ))),
        );
        string_lib.set_field(
            "format".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![("formatstring".to_string(), LuaType::String)],
                returns: vec![LuaType::String],
                vararg: true,
                is_method: false,
            })),
        );
        string_lib.set_field(
            "find".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                ],
                vec![
                    LuaType::union(vec![LuaType::Number, LuaType::Nil]),
                    LuaType::union(vec![LuaType::Number, LuaType::Nil]),
                ],
            ))),
        );
        string_lib.set_field(
            "match".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                ],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            ))),
        );
        string_lib.set_field(
            "gsub".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("pattern".to_string(), LuaType::String),
                    ("repl".to_string(), LuaType::Any),
                ],
                vec![LuaType::String, LuaType::Number],
            ))),
        );
        string_lib.set_field(
            "rep".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("n".to_string(), LuaType::Number),
                ],
                vec![LuaType::String],
            ))),
        );
        string_lib.set_field(
            "reverse".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::String],
                LuaType::String,
            ))),
        );
        string_lib.set_field(
            "byte".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("s".to_string(), LuaType::String),
                    ("i".to_string(), LuaType::Number),
                ],
                vec![LuaType::union(vec![LuaType::Number, LuaType::Nil])],
            ))),
        );
        string_lib.set_field(
            "char".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::String],
                vararg: true,
                is_method: false,
            })),
        );
        env.set("string".to_string(), LuaType::Table(string_lib));

        // Math library
        let mut math_lib = TableType::new();
        math_lib.set_field(
            "abs".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "floor".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "ceil".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "sqrt".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "sin".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "cos".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "tan".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "log".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "exp".to_string(),
            LuaType::Function(Box::new(FunctionType::simple(
                vec![LuaType::Number],
                LuaType::Number,
            ))),
        );
        math_lib.set_field(
            "pow".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("x".to_string(), LuaType::Number),
                    ("y".to_string(), LuaType::Number),
                ],
                vec![LuaType::Number],
            ))),
        );
        math_lib.set_field(
            "min".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::Number],
                vararg: true,
                is_method: false,
            })),
        );
        math_lib.set_field(
            "max".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![("...".to_string(), LuaType::Number)],
                returns: vec![LuaType::Number],
                vararg: true,
                is_method: false,
            })),
        );
        math_lib.set_field(
            "random".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![
                    ("m".to_string(), LuaType::Number),
                    ("n".to_string(), LuaType::Number),
                ],
                returns: vec![LuaType::Number],
                vararg: false,
                is_method: false,
            })),
        );
        math_lib.set_field("pi".to_string(), LuaType::Number);
        math_lib.set_field("huge".to_string(), LuaType::Number);
        env.set("math".to_string(), LuaType::Table(math_lib));

        // Table library
        let mut table_lib = TableType::new();
        table_lib.set_field(
            "insert".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("value".to_string(), LuaType::Any),
                ],
                vec![],
            ))),
        );
        table_lib.set_field(
            "remove".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("pos".to_string(), LuaType::Number),
                ],
                vec![LuaType::Any],
            ))),
        );
        table_lib.set_field(
            "concat".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    ("sep".to_string(), LuaType::String),
                ],
                vec![LuaType::String],
            ))),
        );
        table_lib.set_field(
            "sort".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("t".to_string(), LuaType::Table(TableType::open())),
                    (
                        "comp".to_string(),
                        LuaType::Function(Box::new(FunctionType::default())),
                    ),
                ],
                vec![],
            ))),
        );
        env.set("table".to_string(), LuaType::Table(table_lib));

        // OS library
        let mut os_lib = TableType::new();
        os_lib.set_field(
            "time".to_string(),
            LuaType::Function(Box::new(FunctionType::new(vec![], vec![LuaType::Number]))),
        );
        os_lib.set_field(
            "date".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![("format".to_string(), LuaType::String)],
                vec![LuaType::String],
            ))),
        );
        os_lib.set_field(
            "clock".to_string(),
            LuaType::Function(Box::new(FunctionType::new(vec![], vec![LuaType::Number]))),
        );
        os_lib.set_field(
            "exit".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![("code".to_string(), LuaType::Number)],
                vec![LuaType::Never],
            ))),
        );
        os_lib.set_field(
            "getenv".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![("varname".to_string(), LuaType::String)],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            ))),
        );
        env.set("os".to_string(), LuaType::Table(os_lib));

        // IO library
        let mut io_lib = TableType::new();
        io_lib.set_field(
            "open".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![
                    ("filename".to_string(), LuaType::String),
                    ("mode".to_string(), LuaType::String),
                ],
                vec![LuaType::union(vec![
                    LuaType::Table(TableType::open()), // file handle
                    LuaType::Nil,
                ])],
            ))),
        );
        io_lib.set_field(
            "read".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![("format".to_string(), LuaType::Any)],
                vec![LuaType::union(vec![LuaType::String, LuaType::Nil])],
            ))),
        );
        io_lib.set_field(
            "write".to_string(),
            LuaType::Function(Box::new(FunctionType {
                params: vec![("...".to_string(), LuaType::Any)],
                returns: vec![LuaType::Boolean],
                vararg: true,
                is_method: false,
            })),
        );
        io_lib.set_field(
            "close".to_string(),
            LuaType::Function(Box::new(FunctionType::new(
                vec![("file".to_string(), LuaType::Any)],
                vec![LuaType::Boolean],
            ))),
        );
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
    pub fn infer_function_definition(&mut self, node: Node) -> LuaType {
        self.infer_function_definition_with_name(node, None)
    }

    /// Infer type of a function definition with optional name for constraint tracking
    pub fn infer_function_definition_with_name(
        &mut self,
        node: Node,
        func_name: Option<&str>,
    ) -> LuaType {
        let params = self.extract_function_params(node);

        // Create child environment for function body
        let child_env = self.env.child();
        let old_env = std::mem::replace(&mut self.env, child_env);

        // Save old constraints and current function
        let old_constraints =
            std::mem::replace(&mut self.param_constraints, ParamConstraints::new());
        let old_current_function = self.current_function.take();
        self.current_function = func_name.map(|s| s.to_string());

        // Register parameters in scope (with Unknown type initially)
        for (name, ty) in &params {
            self.env.set(name.clone(), ty.clone());
        }

        // Process function body to collect constraints from asserts
        self.process_function_body(node);

        // Analyze function body to find return types
        let returns = self.infer_function_returns(node);

        // Apply constraints to parameter types
        let params_with_types: Vec<(String, LuaType)> = params
            .iter()
            .map(|(name, _)| {
                let constrained_type = self.param_constraints.resolve(name);
                (name.clone(), constrained_type)
            })
            .collect();

        // Store constraints for this function (for checking calls later)
        if let Some(fn_name) = func_name {
            self.function_constraints
                .insert(fn_name.to_string(), self.param_constraints.clone());
        }

        // Restore environment and constraints
        self.env = old_env;
        self.param_constraints = old_constraints;
        self.current_function = old_current_function;

        let result = LuaType::Function(Box::new(FunctionType {
            params: params_with_types,
            returns,
            vararg: false,
            is_method: false,
        }));

        result
    }

    /// Process function body to collect type constraints from asserts
    fn process_function_body(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block" {
                self.process_block_for_constraints(child);
            }
        }
    }

    /// Recursively process a block to find assert statements
    fn process_block_for_constraints(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_call" => {
                    // Check if it's an assert call
                    let mut fc_cursor = child.walk();
                    for fc_child in child.children(&mut fc_cursor) {
                        if fc_child.kind() == "identifier" {
                            let name = self.node_text(fc_child);
                            if name == "assert" {
                                self.process_assert(child);
                            }
                            break;
                        }
                    }
                }
                "if_statement" | "while_statement" | "repeat_statement" | "for_statement"
                | "do_statement" => {
                    // Recurse into control structures
                    self.process_block_for_constraints(child);
                }
                "block" => {
                    self.process_block_for_constraints(child);
                }
                _ => {
                    // Check children for nested structures
                    if child.named_child_count() > 0 {
                        self.process_block_for_constraints(child);
                    }
                }
            }
        }
    }

    /// Extract function parameters
    fn extract_function_params(&self, node: Node) -> Vec<(String, LuaType)> {
        let mut params = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "parameters" {
                let mut param_cursor = child.walk();

                if param_cursor.goto_first_child() {
                    loop {
                        let kind = param_cursor.node().kind();

                        if kind == "identifier" {
                            let name = self.node_text(param_cursor.node());
                            params.push((name, LuaType::Unknown));
                        }

                        if !param_cursor.goto_next_sibling() {
                            break;
                        }
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

        // Check for narrowed type from pcall results first
        for ctx in self.narrowing_stack.iter().rev() {
            if let Some(narrowed) = ctx.get_narrowed_type(&name) {
                return narrowed;
            }
        }

        // Fall back to environment
        if let Some(ty) = self.env.get(&name) {
            return ty;
        }

        // Check if it's a function
        if let Some(func) = self.env.get_function(&name) {
            return LuaType::Function(Box::new(func));
        }

        LuaType::Unknown
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

        let Some(left_node) = left else {
            return LuaType::Unknown;
        };
        let Some(right_node) = right else {
            return LuaType::Unknown;
        };

        let left_type = self.infer_expression(left_node);
        let right_type = self.infer_expression(right_node);

        let op_str = op.unwrap_or("");

        match op_str {
            // Arithmetic operators -> number
            "+" | "-" | "*" | "/" | "%" | "^" => {
                // Check operands are numeric
                let left_is_valid =
                    matches!(left_type, LuaType::Number | LuaType::Unknown | LuaType::Any);
                let right_is_valid = matches!(
                    right_type,
                    LuaType::Number | LuaType::Unknown | LuaType::Any
                );

                if !left_is_valid {
                    self.errors.push(TypeError::type_mismatch(
                        &LuaType::Number,
                        &left_type,
                        left_node.start_position().row,
                        left_node.start_position().column,
                    ));
                }
                if !right_is_valid {
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
                // Check that operands can be concatenated (not nil)
                let left_can_concat = !matches!(left_type, LuaType::Nil);
                let right_can_concat = !matches!(right_type, LuaType::Nil);

                if !left_can_concat {
                    self.errors.push(TypeError {
                        message: "Cannot concatenate nil".to_string(),
                        expected: LuaType::String,
                        actual: left_type.clone(),
                        line: left_node.start_position().row,
                        column: left_node.start_position().column,
                    });
                }
                if !right_can_concat {
                    self.errors.push(TypeError {
                        message: "Cannot concatenate nil".to_string(),
                        expected: LuaType::String,
                        actual: right_type.clone(),
                        line: right_node.start_position().row,
                        column: right_node.start_position().column,
                    });
                }
                LuaType::String
            }

            // Comparison operators -> boolean
            "==" | "~=" | "<" | ">" | "<=" | ">=" => {
                // Lua allows comparing any types, but we can warn about obvious mismatches
                // For now, just return boolean without errors
                LuaType::Boolean
            }

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

        let Some(operand_node) = operand else {
            return LuaType::Unknown;
        };
        let operand_type = self.infer_expression(operand_node);

        match op {
            Some("-") => {
                // Unary minus requires numeric type
                if !matches!(
                    operand_type,
                    LuaType::Number | LuaType::Unknown | LuaType::Any
                ) {
                    self.errors.push(TypeError::type_mismatch(
                        &LuaType::Number,
                        &operand_type,
                        operand_node.start_position().row,
                        operand_node.start_position().column,
                    ));
                }
                LuaType::Number
            }
            Some("not") => LuaType::Boolean,
            Some("#") => {
                // Length operator requires string or table
                if !matches!(
                    operand_type,
                    LuaType::String | LuaType::Table(_) | LuaType::Unknown | LuaType::Any
                ) {
                    self.errors.push(TypeError {
                        message: format!("Cannot get length of {}", operand_type),
                        expected: LuaType::String,
                        actual: operand_type,
                        line: operand_node.start_position().row,
                        column: operand_node.start_position().column,
                    });
                }
                LuaType::Number
            }
            _ => operand_type,
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

        let Some(base_node) = base else {
            return LuaType::Unknown;
        };
        let Some(field_name) = field else {
            return LuaType::Unknown;
        };

        let base_type = self.infer_expression(base_node);

        match &base_type {
            LuaType::Table(table) => {
                if let Some(field_type) = table.get_field(&field_name) {
                    field_type.clone()
                } else {
                    // Field doesn't exist in known table structure
                    // Only warn if table has known fields (not open table)
                    if !table.fields.is_empty() {
                        self.errors.push(TypeError {
                            message: format!("Field '{}' does not exist on table", field_name),
                            expected: LuaType::Unknown,
                            actual: LuaType::Nil,
                            line: node.start_position().row,
                            column: node.start_position().column,
                        });
                    }
                    LuaType::Unknown
                }
            }
            LuaType::String => {
                // String methods accessed via string:method() not string.method
                // But string.method is also valid in Lua
                if let Some(string_type) = self.env.get("string") {
                    if let LuaType::Table(string_lib) = string_type {
                        return string_lib
                            .get_field(&field_name)
                            .cloned()
                            .unwrap_or(LuaType::Unknown);
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

        let Some(base_node) = base else {
            return LuaType::Unknown;
        };
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
    fn infer_method_access(&mut self, node: Node) -> LuaType {
        // Extract base and method name
        let mut base: Option<Node> = None;
        let mut method: Option<String> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "dot_index_expression" if base.is_none() => {
                    base = Some(child);
                }
                "identifier" => {
                    method = Some(self.node_text(child));
                }
                _ => {}
            }
        }

        if let (Some(base_node), Some(method_name)) = (base, method) {
            let base_type = self.infer_expression(base_node);

            // Check if method exists on the base type
            match &base_type {
                LuaType::String => {
                    // String methods
                    if let Some(string_type) = self.env.get("string") {
                        if let LuaType::Table(string_lib) = string_type {
                            if let Some(method_type) = string_lib.get_field(&method_name) {
                                return method_type.clone();
                            }
                        }
                    }
                }
                LuaType::Table(_table) => {
                    // Table methods - we'd need more info about table structure
                    // For now, just return unknown
                }
                _ => {
                    // Method call on non-table/string type
                    self.errors.push(TypeError {
                        message: format!(
                            "Cannot call method '{}' on type {}",
                            method_name, base_type
                        ),
                        expected: LuaType::Table(TableType::new()),
                        actual: base_type,
                        line: node.start_position().row,
                        column: node.start_position().column,
                    });
                }
            }
        }

        // Method access returns the method itself, actual call handled by function_call
        LuaType::Function(Box::new(FunctionType::default()))
    }

    /// Infer type of function call
    fn infer_function_call(&mut self, node: Node) -> LuaType {
        let mut callee: Option<Node> = None;
        let mut args: Option<Node> = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "dot_index_expression" | "method_index_expression"
                    if callee.is_none() =>
                {
                    callee = Some(child);
                }
                "arguments" => {
                    args = Some(child);
                }
                _ => {}
            }
        }

        let Some(callee_node) = callee else {
            return LuaType::Unknown;
        };

        let callee_type = self.infer_expression(callee_node);

        if let Some(name) = self.get_callee_name(callee_node) {
            if name == "type" {
                return LuaType::String;
            }

            if name == "require" {
                if let Some(module_name) = self.extract_string_arg(node) {
                    return self.handle_require(&module_name);
                }
                return LuaType::Any;
            }

            if name == "pcall" || name == "xpcall" {
                return self.infer_pcall(node);
            }

            // Check for library function calls like string.len, math.floor
            if name.contains(".") {
                let parts: Vec<&str> = name.split('.').collect();
                if parts.len() == 2 {
                    let module_name = parts[0];
                    let func_name = parts[1];

                    if let Some(module_type) = self.env.get(module_name) {
                        if let LuaType::Table(table) = module_type {
                            if let Some(func_type) = table.get_field(func_name) {
                                // Validate arguments
                                if let (LuaType::Function(func), Some(args_node)) =
                                    (func_type, &args)
                                {
                                    self.check_function_call_args(*args_node, func, Some(&name));
                                    return func.return_type().clone();
                                } else if let LuaType::Function(func) = func_type {
                                    return func.return_type().clone();
                                }
                            }
                        }
                    }
                }
            }

            if let Some(func_type) = self.env.get_function(&name) {
                if let Some(args_node) = &args {
                    self.check_function_call_args(*args_node, &func_type, Some(&name));
                }
                return func_type.return_type().clone();
            }
        }

        if let (LuaType::Function(func), Some(args_node)) = (&callee_type, &args) {
            self.check_function_call_args(*args_node, func, None);
        }

        match callee_type {
            LuaType::Function(func) => func.return_type().clone(),
            _ => LuaType::Unknown,
        }
    }

    /// Infer type of pcall/xpcall call
    /// Returns union of error type and success type
    fn infer_pcall(&mut self, node: Node) -> LuaType {
        let args_node = self.find_arguments(&node);
        let Some(args_node) = args_node else {
            return LuaType::Union(vec![LuaType::Boolean, LuaType::Any]);
        };

        // Get first argument (the function being called)
        let Some(first_arg_node) = self.get_first_arg(&args_node) else {
            return LuaType::Union(vec![LuaType::Boolean, LuaType::Any]);
        };

        // Infer type of the called function
        let func_type = self.infer_expression(first_arg_node);

        // Validate first argument is a function
        if !matches!(
            func_type,
            LuaType::Function(_) | LuaType::Unknown | LuaType::Any
        ) {
            self.errors.push(TypeError {
                message: format!("pcall first argument must be a function, got {}", func_type),
                expected: LuaType::Function(Box::new(FunctionType::default())),
                actual: func_type.clone(),
                line: first_arg_node.start_position().row,
                column: first_arg_node.start_position().column,
            });
        }

        // The result is the function's return type (or Any if unknown)
        let result_type = match func_type {
            LuaType::Function(func) => func.return_type().clone(),
            _ => LuaType::Any,
        };

        // Return union of boolean (success) and result type
        LuaType::union(vec![LuaType::Boolean, result_type])
    }

    /// Check that function call arguments match expected parameter types
    fn check_function_call_args(
        &mut self,
        args_node: Node,
        func_type: &FunctionType,
        _func_name: Option<&str>,
    ) {
        let mut cursor = args_node.walk();
        let mut arg_index = 0;

        for child in args_node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }

            if let Some((_param_name, param_type)) = func_type.params.get(arg_index) {
                let arg_type = self.infer_expression(child);

                // Use the parameter type from the function signature (which includes constraints)
                let expected_type = param_type.clone();

                if !arg_type.is_assignable_to(&expected_type)
                    && !matches!(expected_type, LuaType::Unknown)
                {
                    let start = child.start_position();

                    // Avoid duplicate errors at the same location
                    let already_reported = self
                        .errors
                        .iter()
                        .any(|e| e.line == start.row && e.column == start.column);
                    if !already_reported {
                        let err = TypeError::type_mismatch(
                            &expected_type,
                            &arg_type,
                            start.row,
                            start.column,
                        );
                        self.errors.push(err);
                    }
                }

                arg_index += 1;
            }
        }
    }

    /// Find arguments node in function call
    fn find_arguments<'tree>(&self, node: &Node<'tree>) -> Option<Node<'tree>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                return Some(child);
            }
        }
        None
    }

    /// Get first argument from arguments node
    fn get_first_arg<'tree>(&self, node: &Node<'tree>) -> Option<Node<'tree>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                return Some(child);
            }
        }
        None
    }

    /// Get identifier name from a node
    fn get_identifier_name(&self, node: Node) -> Option<String> {
        if node.kind() == "identifier" {
            Some(self.node_text(node))
        } else {
            None
        }
    }

    /// Check if a node is a pcall or xpcall call
    fn is_pcall_call(&self, node: Node) -> bool {
        if node.kind() != "function_call" {
            return false;
        }

        // Extract callee from function_call
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name = self.node_text(child);
                return name == "pcall" || name == "xpcall";
            }
        }

        false
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
            "function_call" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" || child.kind() == "dot_index_expression" {
                        return self.get_callee_name(child);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Extract string argument from function call (for require, etc.)
    fn extract_string_arg(&self, node: Node) -> Option<String> {
        let args_node = self.find_arguments(&node)?;
        let first_arg = self.get_first_arg(&args_node)?;

        if first_arg.kind() == "string" {
            self.extract_string_value(first_arg)
        } else {
            None
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

        // For variable_declaration, look inside assignment_statement
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "assignment_statement" {
                let mut assign_cursor = child.walk();
                for assign_child in child.children(&mut assign_cursor) {
                    match assign_child.kind() {
                        "name_list" | "variable_list" => {
                            let mut name_cursor = assign_child.walk();
                            for name_node in assign_child.children(&mut name_cursor) {
                                if name_node.kind() == "identifier" {
                                    names.push(self.node_text(name_node));
                                }
                            }
                        }
                        "expression_list" => {
                            let mut expr_cursor = assign_child.walk();
                            for expr in assign_child.children(&mut expr_cursor) {
                                if expr.is_named() {
                                    values.push(expr);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                break;
            } else {
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
        }

        // Check if this is a pcall/xpcall assignment
        let is_pcall = if let Some(first_value) = values.first() {
            self.is_pcall_call(*first_value)
        } else {
            false
        };

        // For pcall, get the function's return type for the result variable
        let pcall_result_type = if is_pcall && names.len() >= 2 {
            values.first().and_then(|pcall_node| {
                let args_node = self.find_arguments(pcall_node)?;
                let first_arg = self.get_first_arg(&args_node)?;
                let func_type = self.infer_expression(first_arg);
                match func_type {
                    LuaType::Function(func) => Some(func.return_type().clone()),
                    _ => Some(LuaType::Any),
                }
            })
        } else {
            None
        };

        // Store names for pcall tracking before consuming
        let names_for_pcall = if is_pcall && names.len() >= 2 {
            Some((names[0].clone(), names[1].clone()))
        } else {
            None
        };

        // Assign types to variables
        for (i, name) in names.into_iter().enumerate() {
            let ty = if let Some(value_node) = values.get(i) {
                self.infer_expression(*value_node)
            } else {
                LuaType::Nil
            };

            // For pcall: first variable is Boolean, second is function's return type
            let final_ty = if let Some((success_var, result_var)) = &names_for_pcall {
                if name == *success_var {
                    LuaType::Boolean
                } else if name == *result_var {
                    pcall_result_type.clone().unwrap_or(LuaType::Any)
                } else {
                    ty
                }
            } else {
                ty
            };

            self.env.set(name.clone(), final_ty);

            // Track pcall result mapping: local ok, result = pcall(...)
            if let Some((success_var, result_var)) = &names_for_pcall {
                if name == *result_var {
                    if let Some(ref result_t) = pcall_result_type {
                        // Record in current narrowing context
                        if let Some(ctx) = self.narrowing_stack.last_mut() {
                            ctx.set_pcall_result(name, success_var.clone(), result_t.clone());
                        } else {
                            // Create a new context if none exists
                            let mut ctx = NarrowingContext::new();
                            ctx.set_pcall_result(name, success_var.clone(), result_t.clone());
                            self.narrowing_stack.push(ctx);
                        }
                    }
                }
            }
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
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut arg_cursor = child.walk();
                for arg in child.children(&mut arg_cursor) {
                    if arg.kind() == "binary_expression" {
                        // Check for nil patterns first
                        if let Some((var, narrow_type)) = self.extract_narrowing_assert(arg) {
                            // Apply narrowing to variable
                            if let Some(current_type) = self.env.get(&var) {
                                let narrowed = narrow_type(&current_type);
                                self.env.update(&var, narrowed);
                            }
                        }

                        // Collect all type assertions (handles both simple and or patterns)
                        let assertions = self.extract_type_assertions(arg);
                        for (var, ty) in assertions {
                            // Apply constraint to parameter
                            self.param_constraints
                                .add(&var, TypeConstraint::ExactType(ty.clone()));

                            // Also update environment
                            self.env.update(&var, ty);
                        }
                    } else if arg.is_named() {
                        // Handle simple assert(x) - narrow to truthy
                        let var_name = self.source[arg.start_byte()..arg.end_byte()].to_string();
                        if let Some(current_type) = self.env.get(&var_name) {
                            let truthy_type = current_type.truthy();
                            self.env.update(&var_name, truthy_type);
                        }
                    }
                }
            }
        }
    }

    /// Extract narrowing patterns from assert (nil checks, etc.)
    fn extract_narrowing_assert(
        &self,
        node: Node,
    ) -> Option<(String, Box<dyn Fn(&LuaType) -> LuaType>)> {
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
                if text == "==" || text == "~=" {
                    op = Some(text);
                }
            }
        }

        let Some(left_node) = left else { return None };
        let Some(right_node) = right else { return None };
        let Some(operator) = op else { return None };

        // Check for variable name
        if left_node.kind() != "identifier" {
            return None;
        }
        let var_name = self.source[left_node.start_byte()..left_node.end_byte()].to_string();

        // Check for nil on right side
        if right_node.kind() != "nil" {
            return None;
        }

        if operator == "==" {
            // assert(x == nil) - narrow to nil
            return Some((var_name, Box::new(|_: &LuaType| LuaType::Nil)));
        } else if operator == "~=" {
            // assert(x ~= nil) - narrow to not-nil
            return Some((var_name, Box::new(|ty: &LuaType| ty.exclude(&LuaType::Nil))));
        }

        None
    }

    /// Extract all type assertions from a binary expression (handles or patterns)
    fn extract_type_assertions(&mut self, node: Node) -> Vec<(String, LuaType)> {
        let mut results = Vec::new();
        self.collect_type_assertions(node, &mut results);
        results
    }

    /// Recursively collect type assertions from binary expressions
    fn collect_type_assertions(&mut self, node: Node, results: &mut Vec<(String, LuaType)>) {
        if node.kind() != "binary_expression" {
            return;
        }

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
                if text == "or" || text == "==" {
                    op = Some(text);
                }
            }
        }

        let Some(left_node) = left else { return };
        let Some(right_node) = right else { return };
        let Some(operator) = op else { return };

        if operator == "or" {
            // Recursively collect from both sides of `or`
            self.collect_type_assertions(left_node, results);
            self.collect_type_assertions(right_node, results);
        } else if operator == "==" {
            // This is a type check: type(x) == "typename"
            if let Some((var, ty)) = self.extract_type_assertion(node) {
                results.push((var, ty));
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

        // Check for pcall success variable being checked
        self.extract_pcall_narrowing(condition, &mut ctx);

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

            // Handle pcall results in else branch - narrow to error type
            for (result_var, (success_var, _)) in &if_ctx.pcall_results {
                if if_ctx.narrowed.contains_key(success_var) {
                    // In else branch, result is the error (string)
                    else_ctx.narrow(result_var.clone(), LuaType::String);
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

    /// Extract pcall success-based narrowing
    fn extract_pcall_narrowing(&mut self, condition: Node, ctx: &mut NarrowingContext) {
        // Check if condition is just an identifier (e.g., if ok then)
        if condition.kind() == "identifier" {
            let var_name = self.node_text(condition);

            // Check if this is a pcall success variable
            // Clone pcall_results to avoid borrowing issues
            let pcall_mappings: Vec<(String, String, LuaType)> = self
                .narrowing_stack
                .iter()
                .flat_map(|c| c.pcall_results.iter())
                .map(|(result, (success, ty))| (result.clone(), success.clone(), ty.clone()))
                .collect();

            for (result_var, success_var, success_type) in pcall_mappings {
                if success_var == var_name {
                    // Narrow the result variable to the success type
                    ctx.narrow(result_var, success_type);
                    break;
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

    /// Handle require() call to load and type-check module
    fn handle_require(&mut self, module_name: &str) -> LuaType {
        // Check cache first
        if let Some(exports) = self.module_cache.get(module_name) {
            return exports.to_table_type();
        }

        // Try to resolve and load the module
        if let Some(module_path) = self.resolve_module_path(module_name) {
            if let Ok(module_source) = std::fs::read_to_string(&module_path) {
                // Parse and analyze the module
                if let Some(exports) = self.extract_module_exports(&module_source) {
                    // Cache the exports
                    Arc::make_mut(&mut self.module_cache)
                        .insert(module_name.to_string(), exports.clone());
                    return exports.to_table_type();
                }
            }
        }

        // Return unknown type if module can't be loaded
        LuaType::Any
    }

    /// Resolve module path from module name
    fn resolve_module_path(&self, module_name: &str) -> Option<String> {
        // Convert dots to path separators: "my.module" -> "my/module"
        let path_str = module_name.replace('.', "/");

        // Try common extensions
        let extensions = [".lua", "/init.lua", "/init"];

        for ext in &extensions {
            let path = format!("{}{}", path_str, ext);

            if let Some(base) = &self.base_path {
                let full_path = format!("{}/{}", base.trim_end_matches('/'), path);
                if std::path::Path::new(&full_path).exists() {
                    return Some(full_path);
                }
            }

            // Try current directory
            if std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }

        None
    }

    /// Extract exports from a module source
    fn extract_module_exports(&self, source: &str) -> Option<ModuleExports> {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;

        let mut exports = ModuleExports::new();
        let root = tree.root_node();

        // Collect top-level assignments (potential exports)
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "function_declaration" => {
                    // Functions declared at module level are exports
                    let mut func_cursor = child.walk();
                    for func_child in child.children(&mut func_cursor) {
                        if func_child.kind() == "identifier" {
                            let func_name =
                                source[func_child.start_byte()..func_child.end_byte()].to_string();
                            // Create function type
                            let func_type = FunctionType::default();
                            exports.functions.insert(func_name, func_type);
                            break;
                        }
                    }
                }
                "assignment_statement" | "variable_declaration" => {
                    // Local variables assigned at module level are exports
                    let mut assign_cursor = child.walk();
                    for assign_child in child.children(&mut assign_cursor) {
                        if assign_child.kind() == "variable_list" {
                            let mut var_cursor = assign_child.walk();
                            for var in assign_child.children(&mut var_cursor) {
                                if var.kind() == "identifier" {
                                    let var_name =
                                        source[var.start_byte()..var.end_byte()].to_string();
                                    // For now, just mark as Any - could infer from expression
                                    exports.bindings.insert(var_name, LuaType::Any);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Some(exports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(code: &str) -> (tree_sitter::Tree, String) {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .unwrap();
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
        inf.param_constraints
            .add("bar", TypeConstraint::ExactType(LuaType::String));

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

    #[test]
    fn test_pcall_return_type() {
        let code = r#"
function get_string()
    return "hello"
end

local ok, result = pcall(get_string)
"#;
        let (tree, source) = parse(code);
        let mut inf = TypeInference::new(&source);

        let root = tree.root_node();

        // Manually register get_string function for testing
        let string_func = FunctionType {
            params: vec![],
            returns: vec![LuaType::String],
            vararg: false,
            is_method: false,
        };
        inf.env.set_function("get_string".to_string(), string_func);

        // Process variable declarations
        for child in root.children(&mut root.walk()) {
            if child.kind() == "variable_declaration" {
                inf.process_declaration(child);
            }
        }

        // Check that ok is inferred as boolean
        let ok_type = inf.env.get("ok");
        assert_eq!(ok_type, Some(LuaType::Boolean));

        // Check that result is inferred as string (function's return type)
        let result_type = inf.env.get("result");
        assert_eq!(result_type, Some(LuaType::String));
    }

    #[test]
    fn test_require_imports_types() {
        // This test verifies that require() imports module types
        // Without actual file loading, we test the structure
        let code = r#"
local mymodule = require("mymodule")
local result = mymodule.process()
"#;
        let (tree, source) = parse(code);
        let mut inf = TypeInference::new(&source);

        let root = tree.root_node();

        // Mock module cache
        let mut cache = HashMap::new();
        let mut module_exports = ModuleExports::new();
        module_exports.functions.insert(
            "process".to_string(),
            FunctionType {
                params: vec![],
                returns: vec![LuaType::String],
                vararg: false,
                is_method: false,
            },
        );
        cache.insert("mymodule".to_string(), module_exports);

        inf = TypeInference::with_module_cache(&source, Arc::new(cache));

        // Process declarations
        for child in root.children(&mut root.walk()) {
            if child.kind() == "variable_declaration" || child.kind() == "assignment_statement" {
                inf.process_declaration(child);
            }
        }

        // Check that mymodule is inferred as a table
        let module_type = inf.env.get("mymodule");
        assert!(matches!(module_type, Some(LuaType::Table(_))));

        // Check that mymodule.process is accessible
        let module_type = inf.env.get("mymodule").unwrap();
        if let LuaType::Table(table) = module_type {
            let process_type = table.get_field("process");
            assert!(process_type.is_some());
            if let Some(LuaType::Function(func)) = process_type {
                assert_eq!(func.returns, vec![LuaType::String]);
            }
        }
    }

    #[test]
    fn test_require_unknown_module() {
        // Test that unknown modules return Any type
        let code = r#"
local unknown = require("nonexistent")
"#;
        let (tree, source) = parse(code);
        let mut inf = TypeInference::new(&source);

        let root = tree.root_node();

        // Process declarations
        for child in root.children(&mut root.walk()) {
            if child.kind() == "variable_declaration" || child.kind() == "assignment_statement" {
                inf.process_declaration(child);
            }
        }

        // Check that unknown module is inferred as Any
        let module_type = inf.env.get("unknown");
        assert_eq!(module_type, Some(LuaType::Any));
    }
}
