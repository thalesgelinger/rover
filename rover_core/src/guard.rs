use mlua::{Error as LuaError, FromLua, Lua, Table, UserData, UserDataMethods, Value};
use serde_json;

#[derive(Debug, Clone)]
pub enum ValidatorType {
    String,
    Number,
    Integer,
    Boolean,
    Array(Box<Validator>),  
    Object(Table),         
}

#[derive(Debug, Clone)]
pub struct Validator {
    validator_type: ValidatorType,
    required: bool,
    required_msg: Option<String>,
    default_value: Option<DefaultValue>,
    enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum DefaultValue {
    String(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Nil,
}

impl Validator {
    fn new(validator_type: ValidatorType) -> Self {
        Self {
            validator_type,
            required: false,
            required_msg: None,
            default_value: None,
            enum_values: None,
        }
    }

    fn validate(&self, lua: &Lua, field_name: &str, value: Option<Value>) -> mlua::Result<Value> {
        let value = match value {
            Some(Value::Nil) | None => {
                if self.required {
                    let msg = if let Some(ref custom_msg) = self.required_msg {
                        custom_msg.clone()
                    } else {
                        format!("Missing required field: {}", field_name)
                    };
                    return Err(LuaError::RuntimeError(msg));
                }
                
                if let Some(ref default) = self.default_value {
                    return Ok(match default {
                        DefaultValue::String(s) => Value::String(lua.create_string(s)?),
                        DefaultValue::Number(n) => Value::Number(*n),
                        DefaultValue::Integer(i) => Value::Integer(*i),
                        DefaultValue::Boolean(b) => Value::Boolean(*b),
                        DefaultValue::Nil => Value::Nil,
                    });
                }
                
                return Ok(Value::Nil);
            }
            Some(v) => v,
        };

        match &self.validator_type {
            ValidatorType::String => {
                if let Value::String(_) = value {
                    if let Some(ref allowed) = self.enum_values {
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
            ValidatorType::Number => {
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
            ValidatorType::Integer => {
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
            ValidatorType::Boolean => {
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
            ValidatorType::Array(element_validator) => {
                if let Value::Table(ref table) = value {
                    let result = lua.create_table()?;
                    let len = table.len()?;
                    
                    for i in 1..=len {
                        let elem: Value = table.get(i)?;
                        let validated = element_validator.validate(
                            lua,
                            &format!("{}[{}]", field_name, i),
                            Some(elem)
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
            ValidatorType::Object(schema) => {
                if let Value::Table(ref data_table) = value {
                    validate_table(lua, data_table, schema, field_name)
                } else {
                    Err(LuaError::RuntimeError(format!(
                        "Field '{}' must be an object, got {}",
                        field_name,
                        value.type_name()
                    )))
                }
            }
        }
    }
}

impl FromLua for Validator {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::UserData(ud) => {
                Ok(ud.borrow::<Validator>()?.clone())
            }
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Validator".to_string(),
                message: Some("Expected a validator (e.g., g:string())".to_string()),
            }),
        }
    }
}

impl UserData for Validator {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("required", |_lua, this, msg: Option<String>| {
            let mut validator = this.clone();
            validator.required = true;
            validator.required_msg = msg;
            Ok(validator)
        });

        methods.add_method("default", |_lua, this, value: Value| {
            let mut validator = this.clone();
            let default_value = match value {
                Value::String(s) => DefaultValue::String(s.to_str()?.to_string()),
                Value::Number(n) => DefaultValue::Number(n),
                Value::Integer(i) => DefaultValue::Integer(i),
                Value::Boolean(b) => DefaultValue::Boolean(b),
                Value::Nil => DefaultValue::Nil,
                _ => return Err(LuaError::RuntimeError(
                    "Default value must be string, number, integer, boolean, or nil".to_string()
                )),
            };
            validator.default_value = Some(default_value);
            Ok(validator)
        });

        methods.add_method("enum", |_lua, this, values: Vec<String>| {
            if values.is_empty() {
                return Err(LuaError::RuntimeError("Enum must have at least one value".to_string()));
            }
            let mut validator = this.clone();
            validator.enum_values = Some(values);
            Ok(validator)
        });
    }
}

/// Guard table that provides validator constructors
pub struct Guard;

impl UserData for Guard {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("string", |_lua, _this, ()| {
            Ok(Validator::new(ValidatorType::String))
        });

        methods.add_method("number", |_lua, _this, ()| {
            Ok(Validator::new(ValidatorType::Number))
        });

        methods.add_method("integer", |_lua, _this, ()| {
            Ok(Validator::new(ValidatorType::Integer))
        });

        methods.add_method("boolean", |_lua, _this, ()| {
            Ok(Validator::new(ValidatorType::Boolean))
        });

        methods.add_method("array", |_lua, _this, element_validator: Validator| {
            Ok(Validator::new(ValidatorType::Array(Box::new(element_validator))))
        });

        methods.add_method("object", |_lua, _this, schema: Table| {
            Ok(Validator::new(ValidatorType::Object(schema)))
        });
    }
}

/// Validate a Lua table against a schema (public for use in lib.rs)
pub fn validate_table(lua: &Lua, data: &Table, schema: &Table, context: &str) -> mlua::Result<Value> {
    let result = lua.create_table()?;

    for pair in schema.pairs::<String, Value>() {
        let (field_name, validator_value) = pair?;

        // Get validator from userdata
        let validator = match validator_value {
            Value::UserData(ref ud) => {
                ud.borrow::<Validator>()
                    .map_err(|_| LuaError::RuntimeError(format!(
                        "Field '{}' has invalid validator type",
                        field_name
                    )))?
                    .clone()
            }
            _ => {
                return Err(LuaError::RuntimeError(format!(
                    "Field '{}' must have a validator (e.g., g:string())",
                    field_name
                )))
            }
        };

        // Get value from data table
        let lua_value: Value = data.get(&field_name as &str)?;
        let lua_value = if matches!(lua_value, Value::Nil) {
            None
        } else {
            Some(lua_value)
        };

        // Validate and set in result table
        let full_field_name = if context.is_empty() {
            field_name.clone()
        } else {
            format!("{}.{}", context, field_name)
        };
        let validated_value = validator.validate(lua, &full_field_name, lua_value)?;
        result.set(field_name, validated_value)?;
    }

    Ok(Value::Table(result))
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
