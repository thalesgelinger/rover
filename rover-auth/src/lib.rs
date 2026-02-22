use anyhow::{anyhow, Result};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use mlua::{Lua, Table, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user identifier)
    pub sub: String,
    /// Issued at (timestamp)
    pub iat: Option<i64>,
    /// Expiration (timestamp)
    pub exp: Option<i64>,
    /// Issuer
    pub iss: Option<String>,
    /// Audience
    pub aud: Option<String>,
    /// Custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Create JWT token
fn create_token(
    lua: &Lua,
    (claims_table, secret, algorithm): (Table, String, Option<String>),
) -> mlua::Result<String> {
    let sub: String = claims_table.get("sub")?;
    let iat: Option<i64> = claims_table.get("iat")?;
    let exp: Option<i64> = claims_table.get("exp")?;
    let iss: Option<String> = claims_table.get("iss")?;
    let aud: Option<String> = claims_table.get("aud")?;

    // Extract custom claims
    let mut custom = HashMap::new();
    for pair in claims_table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        if let Value::String(key_str) = key {
            let key = key_str.to_str()?.to_string();
            // Skip standard claims
            if key != "sub" && key != "iat" && key != "exp" && key != "iss" && key != "aud" {
                let json_value = lua_value_to_json(&value)?;
                custom.insert(key, json_value);
            }
        }
    }

    let claims = Claims {
        sub,
        iat,
        exp,
        iss,
        aud,
        custom,
    };

    let header = Header::default();
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    let token = encode(&header, &claims, &encoding_key)
        .map_err(|e| mlua::Error::RuntimeError(format!("Failed to create JWT: {}", e)))?;

    Ok(token)
}

/// Verify JWT token
fn verify_token(
    lua: &Lua,
    (token, secret, algorithm): (String, String, Option<String>),
) -> mlua::Result<Value> {
    let validation = Validation::default();
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());

    match decode::<Claims>(&token, &decoding_key, &validation) {
        Ok(token_data) => {
            let claims = token_data.claims;

            // Convert to Lua table
            let table = lua.create_table()?;
            table.set("sub", claims.sub)?;
            if let Some(iat) = claims.iat {
                table.set("iat", iat)?;
            }
            if let Some(exp) = claims.exp {
                table.set("exp", exp)?;
            }
            if let Some(iss) = claims.iss {
                table.set("iss", iss)?;
            }
            if let Some(aud) = claims.aud {
                table.set("aud", aud)?;
            }

            // Add custom claims
            for (key, value) in claims.custom {
                let lua_value = json_value_to_lua(lua, &value)?;
                table.set(key, lua_value)?;
            }

            table.set("valid", true)?;
            Ok(Value::Table(table))
        }
        Err(e) => {
            let table = lua.create_table()?;
            table.set("valid", false)?;
            table.set("error", format!("{}", e))?;
            Ok(Value::Table(table))
        }
    }
}

/// Decode JWT token without verification (for inspection)
fn decode_token(lua: &Lua, token: String) -> mlua::Result<Value> {
    match decode::<Claims>(
        &token,
        &DecodingKey::from_secret(&[]),
        &Validation::default(),
    ) {
        Ok(token_data) => {
            let claims = token_data.claims;

            let table = lua.create_table()?;
            table.set("sub", claims.sub)?;
            if let Some(iat) = claims.iat {
                table.set("iat", iat)?;
            }
            if let Some(exp) = claims.exp {
                table.set("exp", exp)?;
            }
            if let Some(iss) = claims.iss {
                table.set("iss", iss)?;
            }
            if let Some(aud) = claims.aud {
                table.set("aud", aud)?;
            }

            // Add custom claims
            for (key, value) in claims.custom {
                let lua_value = json_value_to_lua(lua, &value)?;
                table.set(key, lua_value)?;
            }

            table.set("valid", true)?;
            Ok(Value::Table(table))
        }
        Err(e) => {
            let table = lua.create_table()?;
            table.set("valid", false)?;
            table.set("error", format!("{}", e))?;
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON value
fn lua_value_to_json(value: &Value) -> mlua::Result<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Number(n) => Ok(serde_json::json!(n)),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array
            let mut is_array = true;
            let mut array_values = Vec::new();
            let mut object_values = HashMap::new();

            for pair in t.clone().pairs::<Value, Value>() {
                let (key, val) = pair?;
                match key {
                    Value::Integer(i) if i > 0 => {
                        array_values.push((i as usize - 1, lua_value_to_json(&val)?));
                    }
                    Value::String(s) => {
                        is_array = false;
                        object_values.insert(s.to_str()?.to_string(), lua_value_to_json(&val)?);
                    }
                    _ => {
                        is_array = false;
                    }
                }
            }

            if is_array && !array_values.is_empty() {
                array_values.sort_by_key(|(i, _)| *i);
                let arr: Vec<_> = array_values.into_iter().map(|(_, v)| v).collect();
                Ok(serde_json::Value::Array(arr))
            } else {
                Ok(serde_json::Value::Object(
                    object_values.into_iter().collect(),
                ))
            }
        }
        _ => Err(mlua::Error::RuntimeError(
            "Cannot convert Lua value to JSON".to_string(),
        )),
    }
}

/// Convert JSON value to Lua value
fn json_value_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, val) in arr.iter().enumerate() {
                table.set(i + 1, json_value_to_lua(lua, val)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(obj) => {
            let table = lua.create_table()?;
            for (key, val) in obj.iter() {
                table.set(key.as_str(), json_value_to_lua(lua, val)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Create the rover.auth module
pub fn create_auth_module(lua: &Lua) -> Result<Table> {
    let auth_module = lua.create_table()?;

    // rover.auth.create(claims, secret) - Create JWT token
    auth_module.set("create", lua.create_function(create_token)?)?;

    // rover.auth.verify(token, secret) - Verify JWT token
    auth_module.set("verify", lua.create_function(verify_token)?)?;

    // rover.auth.decode(token) - Decode JWT without verification
    auth_module.set("decode", lua.create_function(decode_token)?)?;

    Ok(auth_module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_verify_token() {
        let lua = Lua::new();
        let claims_table = lua.create_table().unwrap();
        claims_table.set("sub", "user123").unwrap();
        claims_table.set("role", "admin").unwrap();
        // Set expiration far in the future to avoid "expired" errors
        claims_table.set("exp", 9999999999i64).unwrap();

        let secret = "my-secret-key".to_string();
        let token = create_token(&lua, (claims_table, secret.clone(), None)).unwrap();

        // Verify the token
        let result = verify_token(&lua, (token, secret, None)).unwrap();

        if let Value::Table(t) = result {
            let valid: bool = t.get("valid").unwrap();
            assert!(valid, "Token should be valid");
            let sub: String = t.get("sub").unwrap();
            assert_eq!(sub, "user123");
            let role: String = t.get("role").unwrap();
            assert_eq!(role, "admin");
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_verify_invalid_token() {
        let lua = Lua::new();
        let token = "invalid.token.here".to_string();
        let secret = "my-secret-key".to_string();

        let result = verify_token(&lua, (token, secret, None)).unwrap();

        if let Value::Table(t) = result {
            let valid: bool = t.get("valid").unwrap();
            assert!(!valid);
        } else {
            panic!("Expected table");
        }
    }
}
