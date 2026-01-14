use mlua::{MetaMethod, UserData, UserDataMethods};
use serde_json::json;
use std::fmt;

mod task;
pub use task::*;

/// Represents a validation error with path and message
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
    pub error_type: String,
}

impl ValidationError {
    pub fn new(path: &str, message: &str, error_type: &str) -> Self {
        Self {
            path: path.to_string(),
            message: message.to_string(),
            error_type: error_type.to_string(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Field '{}': {}", self.path, self.message)
    }
}

/// A collection of validation errors that formats nicely when displayed
#[derive(Debug, Clone)]
pub struct ValidationErrors {
    pub errors: Vec<ValidationError>,
}

impl ValidationErrors {
    pub fn new(errors: Vec<ValidationError>) -> Self {
        Self { errors }
    }

    /// Convert ValidationErrors directly to JSON string (no Lua, no string parsing)
    pub fn to_json_string(&self) -> String {
        let errors: Vec<_> = self
            .errors
            .iter()
            .map(|err| {
                json!({
                    "field": err.path,
                    "message": err.message,
                    "type": err.error_type
                })
            })
            .collect();
        json!({ "errors": errors }).to_string()
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Validation failed for request body:\n")?;

        for (i, err) in self.errors.iter().enumerate() {
            writeln!(f, "  {}. Field '{}'", i + 1, err.path)?;
            writeln!(f, "     Error: {}", err.message)?;
            writeln!(f, "     Type: {}", err.error_type)?;
            if i < self.errors.len() - 1 {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

impl UserData for ValidationErrors {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(format!("{}", this)));

        // Return structured error data for API responses
        methods.add_method("to_json", |lua, this, ()| {
            let errors_array = lua.create_table_with_capacity(this.errors.len(), 0)?;
            for (i, err) in this.errors.iter().enumerate() {
                let error_obj = lua.create_table()?;
                error_obj.set("field", err.path.clone())?;
                error_obj.set("message", err.message.clone())?;
                error_obj.set("type", err.error_type.clone())?;
                errors_array.set(i + 1, error_obj)?;
            }

            let response = lua.create_table()?;
            response.set("errors", errors_array)?;
            Ok(response)
        });

        // Legacy method - kept for backward compatibility
        methods.add_method("errors", |lua, this, ()| {
            let errors_table = lua.create_table_with_capacity(this.errors.len(), 0)?;
            for (i, err) in this.errors.iter().enumerate() {
                let error_table = lua.create_table()?;
                error_table.set("path", err.path.clone())?;
                error_table.set("message", err.message.clone())?;
                error_table.set("type", err.error_type.clone())?;
                errors_table.set(i + 1, error_table)?;
            }
            Ok(errors_table)
        });
    }
}
