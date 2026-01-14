//! Type system for Lua with structural inference
//!
//! This module implements a type system that infers types from:
//! - Literal values
//! - Variable assignments
//! - Property access patterns (structural typing)
//! - Control flow narrowing (type guards)
//! - Assert-based constraints

use std::collections::HashMap;
use std::fmt;

/// Represents a Lua type
#[derive(Debug, Clone, PartialEq)]
pub enum LuaType {
    /// The nil type
    Nil,
    /// Boolean type
    Boolean,
    /// Number type (Lua doesn't distinguish int/float at type level)
    Number,
    /// String type
    String,
    /// Table with known structure
    Table(TableType),
    /// Function type with parameter and return types
    Function(Box<FunctionType>),
    /// Userdata (opaque C data)
    Userdata,
    /// Coroutine thread
    Thread,
    /// Type not yet determined - will be refined by usage
    Unknown,
    /// Union of multiple possible types
    Union(Vec<LuaType>),
    /// Bottom type - represents impossible/never (e.g., after exhaustive type checks)
    Never,
    /// Any type - opt out of type checking
    Any,
}

impl Default for LuaType {
    fn default() -> Self {
        LuaType::Unknown
    }
}

impl LuaType {
    /// Check if this type is assignable to another type
    pub fn is_assignable_to(&self, target: &LuaType) -> bool {
        match (self, target) {
            // Any is assignable to anything and anything is assignable to Any
            (LuaType::Any, _) | (_, LuaType::Any) => true,
            // Unknown can be assigned to anything (it will be refined)
            (LuaType::Unknown, _) => true,
            // Never is assignable to anything (unreachable code)
            (LuaType::Never, _) => true,
            // Same types are assignable
            (a, b) if a == b => true,
            // Nil is assignable to unions containing nil
            (LuaType::Nil, LuaType::Union(types)) => {
                types.iter().any(|t| matches!(t, LuaType::Nil))
            }
            // Any type is assignable to a union if it's assignable to any member
            (t, LuaType::Union(types)) => types.iter().any(|u| t.is_assignable_to(u)),
            // Union is assignable if all members are assignable
            (LuaType::Union(types), target) => types.iter().all(|t| t.is_assignable_to(target)),
            // Table structural compatibility
            (LuaType::Table(a), LuaType::Table(b)) => a.is_assignable_to(b),
            // Function compatibility
            (LuaType::Function(a), LuaType::Function(b)) => a.is_assignable_to(b),
            _ => false,
        }
    }

    /// Create a union type, flattening nested unions and deduplicating
    pub fn union(types: Vec<LuaType>) -> LuaType {
        let mut flattened = Vec::new();
        for t in types {
            match t {
                LuaType::Union(inner) => {
                    for it in inner {
                        if !flattened.contains(&it) {
                            flattened.push(it);
                        }
                    }
                }
                LuaType::Never => {} // Never disappears in unions
                other => {
                    if !flattened.contains(&other) {
                        flattened.push(other);
                    }
                }
            }
        }

        match flattened.len() {
            0 => LuaType::Never,
            1 => flattened.pop().unwrap(),
            _ => LuaType::Union(flattened),
        }
    }

    /// Exclude a type from this type (for narrowing in else branches)
    pub fn exclude(&self, excluded: &LuaType) -> LuaType {
        match self {
            LuaType::Union(types) => {
                let remaining: Vec<LuaType> = types
                    .iter()
                    .filter(|t| *t != excluded)
                    .cloned()
                    .collect();
                LuaType::union(remaining)
            }
            t if t == excluded => LuaType::Never,
            t => t.clone(),
        }
    }

    /// Narrow to a specific type (for type guards)
    pub fn narrow_to(&self, target: &LuaType) -> LuaType {
        match self {
            LuaType::Unknown | LuaType::Any => target.clone(),
            LuaType::Union(types) => {
                if types.contains(target) {
                    target.clone()
                } else {
                    LuaType::Never
                }
            }
            t if t == target => t.clone(),
            _ => LuaType::Never,
        }
    }

    /// Check if type is truthy (excludes nil and false)
    pub fn is_truthy(&self) -> bool {
        !matches!(self, LuaType::Nil | LuaType::Never)
            && match self {
                LuaType::Union(types) => !types.iter().all(|t| matches!(t, LuaType::Nil)),
                _ => true,
            }
    }

    /// Get the truthy narrowing of this type
    pub fn truthy(&self) -> LuaType {
        match self {
            LuaType::Nil => LuaType::Never,
            LuaType::Boolean => LuaType::Boolean, // Could be true
            LuaType::Union(types) => {
                let truthy: Vec<LuaType> = types
                    .iter()
                    .filter(|t| !matches!(t, LuaType::Nil))
                    .cloned()
                    .collect();
                LuaType::union(truthy)
            }
            t => t.clone(),
        }
    }

    /// Check if this type can have properties accessed
    pub fn can_have_properties(&self) -> bool {
        matches!(
            self,
            LuaType::Table(_) | LuaType::Unknown | LuaType::Any | LuaType::String
        )
    }

    /// Check if this type can be called
    pub fn is_callable(&self) -> bool {
        matches!(self, LuaType::Function(_) | LuaType::Unknown | LuaType::Any)
    }

    /// Convert type() result string to LuaType
    pub fn from_type_string(s: &str) -> Option<LuaType> {
        match s {
            "nil" => Some(LuaType::Nil),
            "boolean" => Some(LuaType::Boolean),
            "number" => Some(LuaType::Number),
            "string" => Some(LuaType::String),
            "table" => Some(LuaType::Table(TableType::default())),
            "function" => Some(LuaType::Function(Box::new(FunctionType::default()))),
            "userdata" => Some(LuaType::Userdata),
            "thread" => Some(LuaType::Thread),
            _ => None,
        }
    }

    /// Get the type string for type() comparison
    pub fn type_string(&self) -> Option<&'static str> {
        match self {
            LuaType::Nil => Some("nil"),
            LuaType::Boolean => Some("boolean"),
            LuaType::Number => Some("number"),
            LuaType::String => Some("string"),
            LuaType::Table(_) => Some("table"),
            LuaType::Function(_) => Some("function"),
            LuaType::Userdata => Some("userdata"),
            LuaType::Thread => Some("thread"),
            _ => None,
        }
    }
}

impl fmt::Display for LuaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaType::Nil => write!(f, "nil"),
            LuaType::Boolean => write!(f, "boolean"),
            LuaType::Number => write!(f, "number"),
            LuaType::String => write!(f, "string"),
            LuaType::Table(t) => write!(f, "{}", t),
            LuaType::Function(func) => write!(f, "{}", func),
            LuaType::Userdata => write!(f, "userdata"),
            LuaType::Thread => write!(f, "thread"),
            LuaType::Unknown => write!(f, "unknown"),
            LuaType::Union(types) => {
                let parts: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            LuaType::Never => write!(f, "never"),
            LuaType::Any => write!(f, "any"),
        }
    }
}

/// Represents a table type with known fields
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableType {
    /// Known fields and their types
    pub fields: HashMap<String, LuaType>,
    /// If true, table may have additional unknown fields
    pub open: bool,
    /// Array element type (for array-like tables)
    pub array_element: Option<Box<LuaType>>,
    /// Metatable type (if known)
    pub metatable: Option<Box<TableType>>,
}

impl TableType {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a table with specific fields
    pub fn with_fields(fields: HashMap<String, LuaType>) -> Self {
        Self {
            fields,
            open: false,
            array_element: None,
            metatable: None,
        }
    }

    /// Create an open table (can have additional fields)
    pub fn open() -> Self {
        Self {
            fields: HashMap::new(),
            open: true,
            array_element: None,
            metatable: None,
        }
    }

    /// Create an array type
    pub fn array(element_type: LuaType) -> Self {
        Self {
            fields: HashMap::new(),
            open: false,
            array_element: Some(Box::new(element_type)),
            metatable: None,
        }
    }

    /// Add or update a field
    pub fn set_field(&mut self, name: String, ty: LuaType) {
        self.fields.insert(name, ty);
    }

    /// Get a field type
    pub fn get_field(&self, name: &str) -> Option<&LuaType> {
        self.fields.get(name)
    }

    /// Merge another table type into this one (union of fields)
    pub fn merge(&mut self, other: &TableType) {
        for (name, ty) in &other.fields {
            match self.fields.get(name) {
                Some(existing) => {
                    // Merge field types
                    let merged = LuaType::union(vec![existing.clone(), ty.clone()]);
                    self.fields.insert(name.clone(), merged);
                }
                None => {
                    self.fields.insert(name.clone(), ty.clone());
                }
            }
        }
        self.open = self.open || other.open;
    }

    /// Check structural compatibility
    pub fn is_assignable_to(&self, target: &TableType) -> bool {
        // Check all required fields in target are present with compatible types
        for (name, target_type) in &target.fields {
            match self.fields.get(name) {
                Some(our_type) => {
                    if !our_type.is_assignable_to(target_type) {
                        return false;
                    }
                }
                None => {
                    // Field missing - only OK if target field is optional (union with nil)
                    if !matches!(target_type, LuaType::Union(types) if types.contains(&LuaType::Nil))
                    {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl fmt::Display for TableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref elem) = self.array_element {
            return write!(f, "{}[]", elem);
        }

        if self.fields.is_empty() {
            if self.open {
                return write!(f, "table");
            } else {
                return write!(f, "{{}}");
            }
        }

        let mut parts: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        parts.sort(); // Consistent ordering

        if self.open {
            parts.push("...".to_string());
        }

        write!(f, "{{{}}}", parts.join(", "))
    }
}

/// Represents a function type
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionType {
    /// Parameter names and types
    pub params: Vec<(String, LuaType)>,
    /// Return types (Lua functions can return multiple values)
    pub returns: Vec<LuaType>,
    /// Whether this function accepts varargs
    pub vararg: bool,
    /// Whether this is a method (first param is self)
    pub is_method: bool,
}

impl Default for FunctionType {
    fn default() -> Self {
        Self {
            params: Vec::new(),
            returns: vec![LuaType::Unknown],
            vararg: false,
            is_method: false,
        }
    }
}

impl FunctionType {
    pub fn new(params: Vec<(String, LuaType)>, returns: Vec<LuaType>) -> Self {
        Self {
            params,
            returns,
            vararg: false,
            is_method: false,
        }
    }

    /// Create a simple function type with unnamed params
    pub fn simple(param_types: Vec<LuaType>, return_type: LuaType) -> Self {
        let params = param_types
            .into_iter()
            .enumerate()
            .map(|(i, t)| (format!("arg{}", i), t))
            .collect();
        Self {
            params,
            returns: vec![return_type],
            vararg: false,
            is_method: false,
        }
    }

    /// Get parameter type by index
    pub fn param_type(&self, index: usize) -> Option<&LuaType> {
        self.params.get(index).map(|(_, t)| t)
    }

    /// Get parameter type by name
    pub fn param_type_by_name(&self, name: &str) -> Option<&LuaType> {
        self.params.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    /// Set parameter type by name
    pub fn set_param_type(&mut self, name: &str, ty: LuaType) {
        for (n, t) in &mut self.params {
            if n == name {
                *t = ty;
                return;
            }
        }
    }

    /// Get the first return type
    pub fn return_type(&self) -> &LuaType {
        self.returns.first().unwrap_or(&LuaType::Nil)
    }

    /// Check function compatibility (contravariant params, covariant returns)
    pub fn is_assignable_to(&self, target: &FunctionType) -> bool {
        // Check param count (allow varargs)
        if !self.vararg && self.params.len() < target.params.len() {
            return false;
        }

        // Params are contravariant: target params must be assignable to ours
        for (i, (_, target_type)) in target.params.iter().enumerate() {
            if let Some((_, our_type)) = self.params.get(i) {
                if !target_type.is_assignable_to(our_type) {
                    return false;
                }
            }
        }

        // Returns are covariant: our returns must be assignable to target
        for (i, our_return) in self.returns.iter().enumerate() {
            if let Some(target_return) = target.returns.get(i) {
                if !our_return.is_assignable_to(target_return) {
                    return false;
                }
            }
        }

        true
    }
}

impl fmt::Display for FunctionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(name, ty)| {
                if matches!(ty, LuaType::Unknown) {
                    name.clone()
                } else {
                    format!("{}: {}", name, ty)
                }
            })
            .collect();

        let mut param_str = params.join(", ");
        if self.vararg {
            if !param_str.is_empty() {
                param_str.push_str(", ");
            }
            param_str.push_str("...");
        }

        let returns: Vec<String> = self.returns.iter().map(|t| t.to_string()).collect();
        let return_str = if returns.is_empty() {
            "()".to_string()
        } else if returns.len() == 1 {
            returns[0].clone()
        } else {
            format!("({})", returns.join(", "))
        };

        write!(f, "({}) -> {}", param_str, return_str)
    }
}

/// Type constraint for inference
#[derive(Debug, Clone)]
pub enum TypeConstraint {
    /// Must be exactly this type
    Exact(LuaType),
    /// Must have this field with this type
    HasField(String, LuaType),
    /// Must have this method
    HasMethod(String, FunctionType),
    /// Must be callable with these args
    Callable(Vec<LuaType>),
    /// Must support numeric operations
    Numeric,
    /// Must support string operations
    Stringable,
    /// Must support indexing
    Indexable,
}

/// Result of type checking
#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub expected: LuaType,
    pub actual: LuaType,
    pub line: usize,
    pub column: usize,
}

impl TypeError {
    pub fn new(message: String, expected: LuaType, actual: LuaType, line: usize, column: usize) -> Self {
        Self {
            message,
            expected,
            actual,
            line,
            column,
        }
    }

    pub fn type_mismatch(expected: &LuaType, actual: &LuaType, line: usize, column: usize) -> Self {
        Self {
            message: format!("Type '{}' is not assignable to type '{}'", actual, expected),
            expected: expected.clone(),
            actual: actual.clone(),
            line,
            column,
        }
    }

    pub fn missing_field(field: &str, on_type: &LuaType, line: usize, column: usize) -> Self {
        Self {
            message: format!("Property '{}' does not exist on type '{}'", field, on_type),
            expected: LuaType::Unknown,
            actual: on_type.clone(),
            line,
            column,
        }
    }

    pub fn not_callable(ty: &LuaType, line: usize, column: usize) -> Self {
        Self {
            message: format!("Type '{}' is not callable", ty),
            expected: LuaType::Function(Box::new(FunctionType::default())),
            actual: ty.clone(),
            line,
            column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_display() {
        assert_eq!(LuaType::Nil.to_string(), "nil");
        assert_eq!(LuaType::Number.to_string(), "number");
        assert_eq!(LuaType::String.to_string(), "string");
        assert_eq!(
            LuaType::Union(vec![LuaType::String, LuaType::Nil]).to_string(),
            "string | nil"
        );
    }

    #[test]
    fn test_table_type_display() {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), LuaType::String);
        fields.insert("age".to_string(), LuaType::Number);
        let table = TableType::with_fields(fields);
        let s = table.to_string();
        assert!(s.contains("name: string"));
        assert!(s.contains("age: number"));
    }

    #[test]
    fn test_function_type_display() {
        let func = FunctionType::new(
            vec![
                ("name".to_string(), LuaType::String),
                ("age".to_string(), LuaType::Number),
            ],
            vec![LuaType::Boolean],
        );
        assert_eq!(func.to_string(), "(name: string, age: number) -> boolean");
    }

    #[test]
    fn test_type_assignability() {
        assert!(LuaType::String.is_assignable_to(&LuaType::String));
        assert!(LuaType::Unknown.is_assignable_to(&LuaType::String));
        assert!(LuaType::Never.is_assignable_to(&LuaType::String));
        assert!(!LuaType::Number.is_assignable_to(&LuaType::String));

        let union = LuaType::Union(vec![LuaType::String, LuaType::Nil]);
        assert!(LuaType::String.is_assignable_to(&union));
        assert!(LuaType::Nil.is_assignable_to(&union));
        assert!(!LuaType::Number.is_assignable_to(&union));
    }

    #[test]
    fn test_union_creation() {
        // Flatten nested unions
        let inner = LuaType::Union(vec![LuaType::String, LuaType::Number]);
        let outer = LuaType::union(vec![inner, LuaType::Boolean]);
        match outer {
            LuaType::Union(types) => {
                assert_eq!(types.len(), 3);
                assert!(types.contains(&LuaType::String));
                assert!(types.contains(&LuaType::Number));
                assert!(types.contains(&LuaType::Boolean));
            }
            _ => panic!("Expected union"),
        }

        // Single type unwraps
        let single = LuaType::union(vec![LuaType::String]);
        assert_eq!(single, LuaType::String);

        // Empty becomes never
        let empty = LuaType::union(vec![]);
        assert_eq!(empty, LuaType::Never);
    }

    #[test]
    fn test_type_narrowing() {
        let union = LuaType::Union(vec![LuaType::String, LuaType::Number, LuaType::Nil]);

        // Narrow to string
        let narrowed = union.narrow_to(&LuaType::String);
        assert_eq!(narrowed, LuaType::String);

        // Exclude nil
        let no_nil = union.exclude(&LuaType::Nil);
        match no_nil {
            LuaType::Union(types) => {
                assert_eq!(types.len(), 2);
                assert!(!types.contains(&LuaType::Nil));
            }
            _ => panic!("Expected union"),
        }
    }

    #[test]
    fn test_truthy_narrowing() {
        let union = LuaType::Union(vec![LuaType::String, LuaType::Nil]);
        let truthy = union.truthy();
        assert_eq!(truthy, LuaType::String);

        let nil = LuaType::Nil;
        assert_eq!(nil.truthy(), LuaType::Never);
    }

    #[test]
    fn test_table_structural_compatibility() {
        let mut required_fields = HashMap::new();
        required_fields.insert("name".to_string(), LuaType::String);
        let required = TableType::with_fields(required_fields);

        let mut actual_fields = HashMap::new();
        actual_fields.insert("name".to_string(), LuaType::String);
        actual_fields.insert("age".to_string(), LuaType::Number);
        let actual = TableType::with_fields(actual_fields);

        // Actual has more fields than required - OK
        assert!(actual.is_assignable_to(&required));

        // Required has more fields - not OK
        let mut missing_fields = HashMap::new();
        missing_fields.insert("foo".to_string(), LuaType::String);
        let missing = TableType::with_fields(missing_fields);
        assert!(!missing.is_assignable_to(&required));
    }

    #[test]
    fn test_from_type_string() {
        assert_eq!(LuaType::from_type_string("string"), Some(LuaType::String));
        assert_eq!(LuaType::from_type_string("number"), Some(LuaType::Number));
        assert_eq!(LuaType::from_type_string("nil"), Some(LuaType::Nil));
        assert_eq!(LuaType::from_type_string("invalid"), None);
    }
}
