use mlua::{Error as LuaError, Lua, Table, UserData, UserDataMethods, Value};
use serde_json;

/// Validate a single field value based on config passed from Lua
pub fn validate_field(
    lua: &Lua,
    field_name: &str,
    value: Value,
    config: &Table,
) -> mlua::Result<Value> {
    let validator_type: String = config.get("type")?;
    let required: bool = config.get("required").unwrap_or(false);
    let required_msg: Option<String> = match config.raw_get("required_msg") {
        Ok(Value::String(s)) => Some(s.to_str()?.to_string()),
        _ => None,
    };
    let default_value: Option<Value> = match config.raw_get("default") {
        Ok(v@Value::String(_)) | Ok(v@Value::Number(_)) | Ok(v@Value::Integer(_)) | Ok(v@Value::Boolean(_)) => Some(v),
        _ => None,
    };
    let enum_values: Option<Vec<String>> = match config.raw_get("enum") {
        Ok(Value::Table(t)) => {
            let mut values = Vec::new();
            for i in 1..=t.len()? {
                if let Ok(Value::String(s)) = t.get(i) {
                    values.push(s.to_str()?.to_string());
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
            let msg = required_msg.unwrap_or_else(|| format!("Missing required field: {}", field_name));
            return Err(LuaError::RuntimeError(msg));
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
                        Value::String(s) => s.to_str()?,
                        _ => unreachable!(),
                    };
                    if !allowed.contains(&str_val.to_string()) {
                        return Err(LuaError::RuntimeError(format!(
                            "Field '{}' must be one of: {}. Got: '{}'",
                            field_name,
                            allowed.join(", "),
                            str_val
                        )));
                    }
                }
                Ok(value)
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be a string, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        "number" => {
            if let Value::Number(_) = value {
                Ok(value)
            } else if let Value::Integer(i) = value {
                Ok(Value::Number(i as f64))
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be a number, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        "integer" => {
            if let Value::Integer(_) = value {
                Ok(value)
            } else if let Value::Number(n) = value {
                if n.fract() == 0.0 {
                    Ok(Value::Integer(n as i64))
                } else {
                    Err(LuaError::RuntimeError(format!(
                        "Field '{}' must be an integer, got float {}",
                        field_name, n
                    )))
                }
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be an integer, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        "boolean" => {
            if let Value::Boolean(_) = value {
                Ok(value)
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be a boolean, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        "array" => {
            if let Value::Table(ref table) = value {
                let result = lua.create_table()?;
                let len = table.len()?;
                let element_config: Table = config.get("element")?;
                
                for i in 1..=len {
                    let elem: Value = table.get(i)?;
                    let validated = validate_field(
                        lua,
                        &format!("{}[{}]", field_name, i),
                        elem,
                        &element_config
                    )?;
                    result.set(i, validated)?;
                }
                
                Ok(Value::Table(result))
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be an array, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        "object" => {
            if let Value::Table(ref data_table) = value {
                let schema: Table = config.get("schema")?;
                validate_table_internal(lua, data_table, &schema, field_name)
            } else {
                Err(LuaError::RuntimeError(format!(
                    "Field '{}' must be an object, got {}",
                    field_name,
                    value.type_name()
                )))
            }
        }
        _ => Err(LuaError::RuntimeError(format!(
            "Unknown validator type: {}",
            validator_type
        ))),
    }
}

/// Internal table validation helper
fn validate_table_internal(
    lua: &Lua,
    data: &Table,
    schema: &Table,
    context: &str,
) -> mlua::Result<Value> {
    let result = lua.create_table()?;

    // Use clone_from_pairs to avoid iterator issues across threads
    let pairs_vec: Vec<(String, Table)> = schema
        .pairs()
        .collect::<Result<Vec<_>, _>>()?;

    for (field_name, validator_config) in pairs_vec {
        let full_field_name = if context.is_empty() {
            field_name.clone()
        } else {
            format!("{}.{}", context, field_name)
        };
        
        let lua_value: Value = data.get(&field_name as &str)?;
        let validated_value = validate_field(lua, &full_field_name, lua_value, &validator_config)?;
        result.set(field_name, validated_value)?;
    }

    Ok(Value::Table(result))
}

/// Validate a Lua table against a schema (public API)
pub fn validate_table(lua: &Lua, data: &Table, schema: &Table, context: &str) -> mlua::Result<Value> {
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
}

impl UserData for BodyValue {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("expect", |lua, this, schema: Table| {
            // Parse JSON into a Lua table
            let parsed_json: serde_json::Value = serde_json::from_str(&this.json_string)
                .map_err(|e| LuaError::RuntimeError(format!("Invalid JSON in request body: {}", e)))?;

            let body_object = match parsed_json {
                serde_json::Value::Object(obj) => obj,
                _ => {
                    return Err(LuaError::RuntimeError(
                        "Request body must be a JSON object".to_string(),
                    ))
                }
            };

            // Convert JSON to Lua table
            let data_table = lua.create_table()?;
            for (k, v) in body_object {
                data_table.set(k.as_str(), json_to_lua(lua, &v)?)?;
            }

            // Validate using the common validation logic
            validate_table(lua, &data_table, &schema, "")
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
