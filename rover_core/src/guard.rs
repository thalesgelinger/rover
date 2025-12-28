use mlua::{Error as LuaError, Lua, Table, UserData, UserDataMethods, Value};
use std::sync::Arc;
use serde_json;
pub use rover_types::{ValidationError, ValidationErrors};

/// Validate a single field value based on config passed from Lua
pub fn validate_field(
    lua: &Lua,
    field_name: &str,
    value: Value,
    config: &Table,
) -> Result<Value, Vec<ValidationError>> {
    let validator_type: String = match config.get("type") {
        Ok(t) => t,
        Err(_) => {
            return Err(vec![ValidationError::new(
                field_name,
                "Invalid validator configuration",
                "config",
            )]);
        }
    };
    let required: bool = config.get("required").unwrap_or(false);
    let required_msg: Option<String> = match config.raw_get("required_msg") {
        Ok(Value::String(s)) => match s.to_str() {
            Ok(s) => Some(s.to_string()),
            Err(_) => {
                return Err(vec![ValidationError::new(
                    field_name,
                    "Invalid required message",
                    "config",
                )]);
            }
        },
        _ => None,
    };
    let default_value: Option<Value> = match config.raw_get("default") {
        Ok(v @ Value::String(_))
        | Ok(v @ Value::Number(_))
        | Ok(v @ Value::Integer(_))
        | Ok(v @ Value::Boolean(_)) => Some(v),
        _ => None,
    };
    let enum_values: Option<Vec<String>> = match config.raw_get("enum") {
        Ok(Value::Table(t)) => {
            let mut values = Vec::new();
            let len = match t.len() {
                Ok(l) => l,
                Err(_) => {
                    return Err(vec![ValidationError::new(
                        field_name,
                        "Invalid enum configuration",
                        "config",
                    )]);
                }
            };
            for i in 1..=len {
                if let Ok(Value::String(s)) = t.get(i) {
                    match s.to_str() {
                        Ok(s) => values.push(s.to_string()),
                        Err(_) => {
                            return Err(vec![ValidationError::new(
                                field_name,
                                "Invalid enum value",
                                "config",
                            )]);
                        }
                    }
                }
            }
            Some(values)
        }
        _ => None,
    };

    // Handle missing/nil values
    if matches!(value, Value::Nil) {
        // Check for default first, then required
        if let Some(default) = default_value {
            return Ok(default);
        }

        if required {
            let msg = required_msg.unwrap_or_else(|| format!("Field '{}' is required", field_name));
            return Err(vec![ValidationError::new(field_name, &msg, "required")]);
        }

        return Ok(Value::Nil);
    }

    // Type validation
    match validator_type.as_str() {
        "string" => {
            if let Value::String(_) = value {
                // Enum validation
                if let Some(allowed) = enum_values {
                    let str_val = match &value {
                        Value::String(s) => s.to_str().map_err(|_| {
                            vec![ValidationError::new(
                                field_name,
                                "Invalid string value",
                                "type",
                            )]
                        })?,
                        _ => unreachable!(),
                    };
                    if !allowed.contains(&str_val.to_string()) {
                        return Err(vec![ValidationError::new(
                            field_name,
                            &format!("Must be one of: {}. Got: '{}'", allowed.join(", "), str_val),
                            "enum",
                        )]);
                    }
                }
                Ok(value)
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be a string, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        "number" => {
            if let Value::Number(_) = value {
                Ok(value)
            } else if let Value::Integer(i) = value {
                Ok(Value::Number(i as f64))
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be a number, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        "integer" => {
            if let Value::Integer(_) = value {
                Ok(value)
            } else if let Value::Number(n) = value {
                if n.fract() == 0.0 {
                    Ok(Value::Integer(n as i64))
                } else {
                    Err(vec![ValidationError::new(
                        field_name,
                        &format!("Must be an integer, got float {}", n),
                        "type",
                    )])
                }
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be an integer, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        "boolean" => {
            if let Value::Boolean(_) = value {
                Ok(value)
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be a boolean, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        "array" => {
            if let Value::Table(ref table) = value {
                let result = match lua.create_table() {
                    Ok(t) => t,
                    Err(_) => {
                        return Err(vec![ValidationError::new(
                            field_name,
                            "Failed to create result table",
                            "internal",
                        )]);
                    }
                };
                let len = match table.len() {
                    Ok(l) => l,
                    Err(_) => {
                        return Err(vec![ValidationError::new(
                            field_name,
                            "Invalid array structure",
                            "type",
                        )]);
                    }
                };
                let element_config: Table = match config.get("element") {
                    Ok(t) => t,
                    Err(_) => {
                        return Err(vec![ValidationError::new(
                            field_name,
                            "Invalid array element configuration",
                            "config",
                        )]);
                    }
                };

                let mut all_errors = Vec::new();

                for i in 1..=len {
                    let elem_result = table.get(i);
                    if let Err(_) = elem_result {
                        all_errors.push(ValidationError::new(
                            field_name,
                            "Invalid array element access",
                            "type",
                        ));
                        continue;
                    }
                    let elem = elem_result.unwrap();

                    match validate_field(
                        lua,
                        &format!("{}[{}]", field_name, i),
                        elem,
                        &element_config,
                    ) {
                        Ok(validated) => {
                            if let Err(_) = result.set(i, validated) {
                                all_errors.push(ValidationError::new(
                                    field_name,
                                    "Failed to set validated element",
                                    "internal",
                                ));
                            }
                        }
                        Err(errors) => {
                            all_errors.extend(errors);
                        }
                    }
                }

                if all_errors.is_empty() {
                    Ok(Value::Table(result))
                } else {
                    Err(all_errors)
                }
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be an array, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        "object" => {
            if let Value::Table(ref data_table) = value {
                let schema: Table = config.get("schema").map_err(|_| {
                    vec![ValidationError::new(
                        field_name,
                        "Invalid object schema configuration",
                        "config",
                    )]
                })?;
                validate_table_internal(lua, data_table, &schema, field_name)
            } else {
                Err(vec![ValidationError::new(
                    field_name,
                    &format!("Must be an object, got {}", value.type_name()),
                    "type",
                )])
            }
        }
        _ => Err(vec![ValidationError::new(
            field_name,
            &format!("Unknown validator type: {}", validator_type),
            "config",
        )]),
    }
}

/// Internal table validation helper
fn validate_table_internal(
    lua: &Lua,
    data: &Table,
    schema: &Table,
    context: &str,
) -> Result<Value, Vec<ValidationError>> {
    let result = lua.create_table().map_err(|_| {
        vec![ValidationError::new(
            context,
            "Failed to create result table",
            "internal",
        )]
    })?;

    let pairs_vec: Vec<(String, Table)> =
        schema.pairs().collect::<Result<Vec<_>, _>>().map_err(|_| {
            vec![ValidationError::new(
                context,
                "Failed to read schema pairs",
                "config",
            )]
        })?;

    let mut all_errors = Vec::new();

    for (field_name, validator_config) in pairs_vec {
        let full_field_name = if context.is_empty() {
            field_name.clone()
        } else {
            format!("{}.{}", context, field_name)
        };

        let lua_value: Value = data.get(&field_name as &str).unwrap_or(Value::Nil);
        match validate_field(lua, &full_field_name, lua_value, &validator_config) {
            Ok(validated_value) => {
                result.set(field_name, validated_value).map_err(|_| {
                    vec![ValidationError::new(
                        &full_field_name,
                        "Failed to set validated value",
                        "internal",
                    )]
                })?;
            }
            Err(errors) => {
                all_errors.extend(errors);
            }
        }
    }

    if all_errors.is_empty() {
        Ok(Value::Table(result))
    } else {
        Err(all_errors)
    }
}

/// Validate a Lua table against a schema (public API)
pub fn validate_table(
    lua: &Lua,
    data: &Table,
    schema: &Table,
    context: &str,
) -> Result<Value, Vec<ValidationError>> {
    validate_table_internal(lua, data, schema, context)
}

/// A wrapper around parsed body that can be validated with :expect()
pub struct BodyValue {
    json_string: String,
}

impl BodyValue {
    pub fn new(json_string: String) -> Self {
        Self { json_string }
    }

    pub fn json_string(&self) -> &str {
        &self.json_string
    }
}


impl UserData for BodyValue {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("expect", |lua, this, schema: Table| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let parsed_json: serde_json::Value = serde_json::from_str(&this.json_string)
                    .map_err(|e| {
                        LuaError::RuntimeError(format!("Invalid JSON in request body: {}", e))
                    })?;

                let body_object = match parsed_json {
                    serde_json::Value::Object(obj) => obj,
                    _ => {
                        return Err(LuaError::RuntimeError(
                            "Request body must be a JSON object".to_string(),
                        ));
                    }
                };

                let data_table = lua.create_table().map_err(|e| {
                    eprintln!("INTERNAL ERROR: Failed to create data table: {}", e);
                    LuaError::RuntimeError(
                        "Internal server error while processing request".to_string(),
                    )
                })?;

                for (k, v) in body_object {
                    data_table.set(
                        k.as_str(),
                        json_to_lua(lua, &v).map_err(|e| {
                            eprintln!(
                                "INTERNAL ERROR: Failed to convert JSON value for key '{}': {}",
                                k, e
                            );
                            LuaError::RuntimeError(
                                "Internal server error while processing request".to_string(),
                            )
                        })?,
                    )?;
                }

                match validate_table(lua, &data_table, &schema, "") {
                    Ok(validated) => Ok(validated),
                    Err(errors) => {
                        let validation_errors = ValidationErrors::new(errors);
                        Err(LuaError::ExternalError(Arc::new(validation_errors)))
                    }
                }
            }));

            match result {
                Ok(inner_result) => inner_result,
                Err(panic_err) => {
                    eprintln!("PANIC in validation: {:?}", panic_err);
                    Err(LuaError::RuntimeError(
                        "Internal server error occurred during validation".to_string(),
                    ))
                }
            }
        });
    }
}

/// Convert serde_json::Value to mlua::Value
fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Err(LuaError::RuntimeError("Invalid number".to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table_with_capacity(arr.len(), 0)?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(obj) => {
            let table = lua.create_table_with_capacity(0, obj.len())?;
            for (k, v) in obj {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
