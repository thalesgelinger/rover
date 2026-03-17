use anyhow::Result;
use jsonwebtoken::{decode, decode_header, encode, DecodingKey, EncodingKey, Header, Validation};
use mlua::{Function, Lua, ObjectLike, Table, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct KeySet {
    active: String,
    keys: HashMap<String, String>,
}

fn extract_key_set(table: &Table, kind: &str) -> mlua::Result<Option<KeySet>> {
    let key_set_table = match table.get::<Value>("keys")? {
        Value::Table(_) => table.clone(),
        _ => match table.get::<Value>(kind)? {
            Value::Table(key_table) => key_table,
            _ => return Ok(None),
        },
    };

    let active: String = key_set_table.get("active")?;
    let keys_table: Table = key_set_table.get("keys")?;

    let mut keys = HashMap::new();
    for pair in keys_table.pairs::<String, String>() {
        let (key_id, key_value) = pair?;
        keys.insert(key_id, key_value);
    }

    if !keys.contains_key(active.as_str()) {
        return Err(mlua::Error::RuntimeError(format!(
            "{} active key '{}' not found in keys",
            kind, active
        )));
    }

    Ok(Some(KeySet { active, keys }))
}

fn resolve_signing_key_for_create(secret_input: Value) -> mlua::Result<(String, Option<String>)> {
    match secret_input {
        Value::String(secret) => Ok((secret.to_str()?.to_string(), None)),
        Value::Table(table) => {
            let key_set = extract_key_set(&table, "signing")?.ok_or_else(|| {
                mlua::Error::RuntimeError(
                    "secret must be string or key set table with signing keys".to_string(),
                )
            })?;

            let key = key_set
                .keys
                .get(key_set.active.as_str())
                .cloned()
                .ok_or_else(|| {
                    mlua::Error::RuntimeError("active signing key not found".to_string())
                })?;

            Ok((key, Some(key_set.active)))
        }
        _ => Err(mlua::Error::RuntimeError(
            "secret must be string or key set table".to_string(),
        )),
    }
}

fn resolve_signing_keys_for_verify(secret_input: Value) -> mlua::Result<Vec<(String, String)>> {
    match secret_input {
        Value::String(secret) => Ok(vec![("default".to_string(), secret.to_str()?.to_string())]),
        Value::Table(table) => {
            let key_set = extract_key_set(&table, "signing")?.ok_or_else(|| {
                mlua::Error::RuntimeError(
                    "secret must be string or key set table with signing keys".to_string(),
                )
            })?;

            let mut ordered = Vec::with_capacity(key_set.keys.len());
            if let Some(active_key) = key_set.keys.get(key_set.active.as_str()) {
                ordered.push((key_set.active.clone(), active_key.clone()));
            }

            for (key_id, key_value) in key_set.keys {
                if key_id != key_set.active {
                    ordered.push((key_id, key_value));
                }
            }

            Ok(ordered)
        }
        _ => Err(mlua::Error::RuntimeError(
            "secret must be string or key set table".to_string(),
        )),
    }
}

fn ensure_secret_bucket(lua: &Lua, secrets: &Table, kind: &str) -> mlua::Result<Table> {
    match secrets.get::<Value>(kind)? {
        Value::Table(bucket) => Ok(bucket),
        _ => {
            let bucket = lua.create_table()?;
            bucket.set("keys", lua.create_table()?)?;
            secrets.set(kind, bucket.clone())?;
            Ok(bucket)
        }
    }
}

fn create_secrets_table(lua: &Lua, initial: Option<Table>) -> mlua::Result<Table> {
    let secrets = lua.create_table()?;

    if let Some(initial) = initial {
        for kind in ["signing", "encryption"] {
            if let Value::Table(key_set) = initial.get::<Value>(kind)? {
                secrets.set(kind, key_set)?;
            }
        }
    }

    for kind in ["signing", "encryption"] {
        let bucket = ensure_secret_bucket(lua, &secrets, kind)?;
        if !matches!(bucket.get::<Value>("keys")?, Value::Table(_)) {
            bucket.set("keys", lua.create_table()?)?;
        }
    }

    Ok(secrets)
}

fn rotate_secret_key(
    lua: &Lua,
    (secrets, kind, key_id, key_value): (Table, String, String, String),
) -> mlua::Result<()> {
    if kind != "signing" && kind != "encryption" {
        return Err(mlua::Error::RuntimeError(
            "kind must be 'signing' or 'encryption'".to_string(),
        ));
    }

    let bucket = ensure_secret_bucket(lua, &secrets, kind.as_str())?;
    let keys_table: Table = match bucket.get::<Value>("keys")? {
        Value::Table(keys) => keys,
        _ => {
            let keys = lua.create_table()?;
            bucket.set("keys", keys.clone())?;
            keys
        }
    };

    keys_table.set(key_id.clone(), key_value)?;
    bucket.set("active", key_id)?;
    Ok(())
}

fn active_secret_key(lua: &Lua, (secrets, kind): (Table, String)) -> mlua::Result<Value> {
    if kind != "signing" && kind != "encryption" {
        return Err(mlua::Error::RuntimeError(
            "kind must be 'signing' or 'encryption'".to_string(),
        ));
    }

    let bucket = ensure_secret_bucket(lua, &secrets, kind.as_str())?;
    let active: Option<String> = bucket.get("active")?;
    let Some(active) = active else {
        return Ok(Value::Nil);
    };

    let keys_table: Table = bucket.get("keys")?;
    let key_value: Option<String> = keys_table.get(active.as_str())?;
    let Some(key_value) = key_value else {
        return Ok(Value::Nil);
    };

    let out = lua.create_table()?;
    out.set("id", active)?;
    out.set("key", key_value)?;
    Ok(Value::Table(out))
}

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
    _lua: &Lua,
    (claims_table, secret_input, _algorithm): (Table, Value, Option<String>),
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

    let (secret, key_id) = resolve_signing_key_for_create(secret_input)?;
    let header = Header {
        kid: key_id,
        ..Header::default()
    };
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    let token = encode(&header, &claims, &encoding_key)
        .map_err(|e| mlua::Error::RuntimeError(format!("Failed to create JWT: {}", e)))?;

    Ok(token)
}

/// Verify JWT token
fn verify_token(
    lua: &Lua,
    (token, secret_input, _algorithm): (String, Value, Option<String>),
) -> mlua::Result<Value> {
    let validation = Validation::default();
    let key_candidates = resolve_signing_keys_for_verify(secret_input)?;
    let token_kid = decode_header(token.as_str())
        .ok()
        .and_then(|header| header.kid);

    let mut ordered_candidates = Vec::with_capacity(key_candidates.len());
    if let Some(kid) = token_kid {
        if let Some(candidate) = key_candidates
            .iter()
            .find(|(candidate_id, _)| candidate_id == kid.as_str())
        {
            ordered_candidates.push(candidate.clone());
        }
    }

    for candidate in key_candidates {
        if !ordered_candidates
            .iter()
            .any(|(candidate_id, _)| candidate_id == &candidate.0)
        {
            ordered_candidates.push(candidate);
        }
    }

    let mut last_error = "signature validation failed".to_string();

    for (_key_id, secret) in ordered_candidates {
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
                return Ok(Value::Table(table));
            }
            Err(e) => {
                last_error = format!("{}", e);
            }
        }
    }

    let table = lua.create_table()?;
    table.set("valid", false)?;
    table.set("error", last_error)?;
    Ok(Value::Table(table))
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

fn call_ctx_headers(ctx: &Value) -> mlua::Result<Table> {
    match ctx {
        Value::UserData(ud) => ud.call_method::<Table>("headers", ()),
        Value::Table(table) => {
            let headers_fn: Function = table.get("headers")?;
            headers_fn.call((table.clone(),))
        }
        _ => Err(mlua::Error::RuntimeError(
            "middleware ctx must be table or userdata".to_string(),
        )),
    }
}

fn call_ctx_set(ctx: &Value, key: &str, value: Value) -> mlua::Result<()> {
    match ctx {
        Value::UserData(ud) => ud.call_method("set", (key.to_string(), value)),
        Value::Table(table) => {
            let set_fn: Function = table.get("set")?;
            set_fn.call((table.clone(), key.to_string(), value))
        }
        _ => Err(mlua::Error::RuntimeError(
            "middleware ctx must be table or userdata".to_string(),
        )),
    }
}

fn call_ctx_get(ctx: &Value, key: &str) -> mlua::Result<Value> {
    match ctx {
        Value::UserData(ud) => ud.call_method("get", key.to_string()),
        Value::Table(table) => {
            let get_fn: Function = table.get("get")?;
            get_fn.call((table.clone(), key.to_string()))
        }
        _ => Err(mlua::Error::RuntimeError(
            "middleware ctx must be table or userdata".to_string(),
        )),
    }
}

fn deny_response(lua: &Lua, api: &Table, status: u16, message: &str) -> mlua::Result<Value> {
    let error_fn: Function = api.get("error")?;
    error_fn.call((
        api.clone(),
        (status, Value::String(lua.create_string(message)?)),
    ))
}

fn parse_allowed_roles(value: Value) -> mlua::Result<Vec<String>> {
    match value {
        Value::String(role) => Ok(vec![role.to_str()?.to_string()]),
        Value::Table(table) => {
            let mut roles = Vec::new();
            for value in table.sequence_values::<String>() {
                roles.push(value?);
            }
            if roles.is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "allow_roles requires at least one role".to_string(),
                ));
            }
            Ok(roles)
        }
        _ => Err(mlua::Error::RuntimeError(
            "allow_roles roles must be a string or string array".to_string(),
        )),
    }
}

fn create_require_middleware(
    lua: &Lua,
    (api, secret, opts): (Table, Value, Option<Table>),
) -> mlua::Result<Function> {
    let claims_key = opts
        .as_ref()
        .and_then(|o| o.get::<Option<String>>("claims_key").ok())
        .flatten()
        .unwrap_or_else(|| "auth".to_string());
    let header_name = opts
        .as_ref()
        .and_then(|o| o.get::<Option<String>>("header").ok())
        .flatten()
        .unwrap_or_else(|| "Authorization".to_string());

    let middleware = lua.create_function(move |lua, ctx: Value| {
        let headers = call_ctx_headers(&ctx)?;

        let header_value = headers
            .get::<Option<String>>(header_name.as_str())?
            .or_else(|| {
                headers
                    .get::<Option<String>>("Authorization")
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                headers
                    .get::<Option<String>>("authorization")
                    .ok()
                    .flatten()
            });

        let Some(header_value) = header_value else {
            return deny_response(
                lua,
                &api,
                401,
                "Unauthorized: missing Authorization bearer token",
            );
        };

        let token = header_value
            .strip_prefix("Bearer ")
            .or_else(|| header_value.strip_prefix("bearer "));

        let Some(token) = token else {
            return deny_response(
                lua,
                &api,
                401,
                "Unauthorized: Authorization must use Bearer token",
            );
        };

        let verify_result = verify_token(lua, (token.to_string(), secret.clone(), None))?;
        let claims = match verify_result {
            Value::Table(table) => table,
            _ => {
                return deny_response(lua, &api, 401, "Unauthorized: invalid token response");
            }
        };

        let valid = claims.get::<bool>("valid").unwrap_or(false);
        if !valid {
            return deny_response(lua, &api, 401, "Unauthorized: invalid or expired token");
        }

        call_ctx_set(&ctx, &claims_key, Value::Table(claims))?;
        Ok(Value::Nil)
    })?;

    Ok(middleware)
}

fn create_allow_roles_middleware(
    lua: &Lua,
    (api, roles, opts): (Table, Value, Option<Table>),
) -> mlua::Result<Function> {
    let allowed_roles = parse_allowed_roles(roles)?;
    let claims_key = opts
        .as_ref()
        .and_then(|o| o.get::<Option<String>>("claims_key").ok())
        .flatten()
        .unwrap_or_else(|| "auth".to_string());
    let role_key = opts
        .as_ref()
        .and_then(|o| o.get::<Option<String>>("role_key").ok())
        .flatten()
        .unwrap_or_else(|| "role".to_string());

    let middleware = lua.create_function(move |lua, ctx: Value| {
        let claims = call_ctx_get(&ctx, &claims_key)?;
        let claims_table = match claims {
            Value::Table(table) => table,
            Value::Nil => {
                return deny_response(lua, &api, 401, "Unauthorized: missing auth context");
            }
            _ => {
                return deny_response(lua, &api, 401, "Unauthorized: invalid auth context");
            }
        };

        let role = claims_table.get::<Option<String>>(role_key.as_str())?;
        let Some(role) = role else {
            return deny_response(lua, &api, 403, "Forbidden: missing role claim");
        };

        if !allowed_roles.iter().any(|allowed| allowed == &role) {
            return deny_response(
                lua,
                &api,
                403,
                &format!("Forbidden: role '{}' not allowed", role),
            );
        }

        Ok(Value::Nil)
    })?;

    Ok(middleware)
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

    // rover.auth.require(api, secret, opts?) - Authentication middleware helper
    auth_module.set("require", lua.create_function(create_require_middleware)?)?;

    // rover.auth.allow_roles(api, roles, opts?) - Authorization middleware helper
    auth_module.set(
        "allow_roles",
        lua.create_function(create_allow_roles_middleware)?,
    )?;

    // rover.auth.secrets(opts?) - Create signing/encryption key manager table
    auth_module.set("secrets", lua.create_function(create_secrets_table)?)?;

    // rover.auth.rotate(secrets, kind, key_id, key) - Rotate active signing/encryption key
    auth_module.set("rotate", lua.create_function(rotate_secret_key)?)?;

    // rover.auth.active(secrets, kind) - Return current active signing/encryption key
    auth_module.set("active", lua.create_function(active_secret_key)?)?;

    Ok(auth_module)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_api(lua: &Lua) -> Table {
        let api = lua.create_table().unwrap();
        api.set(
            "error",
            lua.create_function(|lua, (_self, (status, message)): (Table, (u16, Value))| {
                let message = match message {
                    Value::String(s) => s.to_str()?.to_string(),
                    _ => "unknown".to_string(),
                };
                let out = lua.create_table()?;
                out.set("status", status)?;
                out.set("message", message)?;
                Ok(Value::Table(out))
            })
            .unwrap(),
        )
        .unwrap();
        api
    }

    fn create_mock_ctx(lua: &Lua) -> Table {
        let ctx = lua.create_table().unwrap();
        let state = lua.create_table().unwrap();
        let headers = lua.create_table().unwrap();
        let headers_for_method = headers.clone();
        ctx.set(
            "headers",
            lua.create_function(move |_lua, _self: Table| Ok(headers_for_method.clone()))
                .unwrap(),
        )
        .unwrap();
        let state_set = state.clone();
        ctx.set(
            "set",
            lua.create_function(move |_lua, (_self, key, value): (Table, String, Value)| {
                state_set.set(key, value)?;
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();
        let state_get = state.clone();
        ctx.set(
            "get",
            lua.create_function(move |_lua, (_self, key): (Table, String)| {
                state_get.get::<Value>(key)
            })
            .unwrap(),
        )
        .unwrap();
        ctx.set("__headers", headers).unwrap();
        ctx
    }

    #[test]
    fn test_create_and_verify_token() {
        let lua = Lua::new();
        let claims_table = lua.create_table().unwrap();
        claims_table.set("sub", "user123").unwrap();
        claims_table.set("role", "admin").unwrap();
        // Set expiration far in the future to avoid "expired" errors
        claims_table.set("exp", 9999999999i64).unwrap();

        let secret = "my-secret-key".to_string();
        let token = create_token(
            &lua,
            (
                claims_table,
                Value::String(lua.create_string(secret.as_str()).unwrap()),
                None,
            ),
        )
        .unwrap();

        // Verify the token
        let result = verify_token(
            &lua,
            (
                token,
                Value::String(lua.create_string(secret.as_str()).unwrap()),
                None,
            ),
        )
        .unwrap();

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

        let result = verify_token(
            &lua,
            (
                token,
                Value::String(lua.create_string(secret.as_str()).unwrap()),
                None,
            ),
        )
        .unwrap();

        if let Value::Table(t) = result {
            let valid: bool = t.get("valid").unwrap();
            assert!(!valid);
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_require_middleware_returns_clear_unauthorized_messages() {
        let lua = Lua::new();
        let api = create_mock_api(&lua);
        let ctx = create_mock_ctx(&lua);
        let headers: Table = ctx.get("__headers").unwrap();

        let middleware = create_require_middleware(
            &lua,
            (
                api,
                Value::String(lua.create_string("secret").unwrap()),
                None,
            ),
        )
        .unwrap();

        let deny = middleware.call::<Value>(ctx.clone()).unwrap();
        let deny_table = match deny {
            Value::Table(t) => t,
            _ => panic!("Expected deny table"),
        };
        let status: u16 = deny_table.get("status").unwrap();
        let message: String = deny_table.get("message").unwrap();
        assert_eq!(status, 401);
        assert!(message.contains("missing Authorization bearer token"));

        headers.set("Authorization", "Token abc").unwrap();
        let bad_format = middleware.call::<Value>(ctx.clone()).unwrap();
        let bad_format_table = match bad_format {
            Value::Table(t) => t,
            _ => panic!("Expected deny table"),
        };
        let bad_status: u16 = bad_format_table.get("status").unwrap();
        let bad_message: String = bad_format_table.get("message").unwrap();
        assert_eq!(bad_status, 401);
        assert!(bad_message.contains("Authorization must use Bearer token"));
    }

    #[test]
    fn test_allow_roles_middleware_returns_clear_forbidden_messages() {
        let lua = Lua::new();
        let api = create_mock_api(&lua);
        let ctx = create_mock_ctx(&lua);

        let roles = lua.create_table().unwrap();
        roles.set(1, "admin").unwrap();
        let middleware =
            create_allow_roles_middleware(&lua, (api, Value::Table(roles), None)).unwrap();

        let unauthorized = middleware.call::<Value>(ctx.clone()).unwrap();
        let unauthorized_table = match unauthorized {
            Value::Table(t) => t,
            _ => panic!("Expected deny table"),
        };
        let status: u16 = unauthorized_table.get("status").unwrap();
        assert_eq!(status, 401);

        let claims = lua.create_table().unwrap();
        claims.set("role", "viewer").unwrap();
        let set_fn: Function = ctx.get("set").unwrap();
        set_fn
            .call::<()>((ctx.clone(), "auth".to_string(), Value::Table(claims)))
            .unwrap();

        let forbidden = middleware.call::<Value>(ctx).unwrap();
        let forbidden_table = match forbidden {
            Value::Table(t) => t,
            _ => panic!("Expected deny table"),
        };
        let forbidden_status: u16 = forbidden_table.get("status").unwrap();
        let forbidden_message: String = forbidden_table.get("message").unwrap();
        assert_eq!(forbidden_status, 403);
        assert!(forbidden_message.contains("not allowed"));
    }

    #[test]
    fn test_signing_key_rotation_verifies_old_tokens_after_rotate() {
        let lua = Lua::new();
        let secrets = create_secrets_table(&lua, None).unwrap();
        rotate_secret_key(
            &lua,
            (
                secrets.clone(),
                "signing".to_string(),
                "v1".to_string(),
                "secret-v1".to_string(),
            ),
        )
        .unwrap();

        let claims = lua.create_table().unwrap();
        claims.set("sub", "user123").unwrap();
        claims.set("exp", 9999999999i64).unwrap();

        let token_v1 =
            create_token(&lua, (claims.clone(), Value::Table(secrets.clone()), None)).unwrap();

        rotate_secret_key(
            &lua,
            (
                secrets.clone(),
                "signing".to_string(),
                "v2".to_string(),
                "secret-v2".to_string(),
            ),
        )
        .unwrap();

        let token_v2 = create_token(&lua, (claims, Value::Table(secrets.clone()), None)).unwrap();

        let verify_v1 =
            verify_token(&lua, (token_v1, Value::Table(secrets.clone()), None)).unwrap();
        let verify_v2 = verify_token(&lua, (token_v2, Value::Table(secrets), None)).unwrap();

        let table_v1 = match verify_v1 {
            Value::Table(t) => t,
            _ => panic!("Expected table"),
        };
        let valid_v1: bool = table_v1.get("valid").unwrap();
        assert!(valid_v1, "token signed with old key should remain valid");

        let table_v2 = match verify_v2 {
            Value::Table(t) => t,
            _ => panic!("Expected table"),
        };
        let valid_v2: bool = table_v2.get("valid").unwrap();
        assert!(valid_v2, "token signed with new key should be valid");
    }

    #[test]
    fn test_encryption_key_rotation_tracks_active_key() {
        let lua = Lua::new();
        let secrets = create_secrets_table(&lua, None).unwrap();

        rotate_secret_key(
            &lua,
            (
                secrets.clone(),
                "encryption".to_string(),
                "enc-v1".to_string(),
                "enc-key-v1".to_string(),
            ),
        )
        .unwrap();

        rotate_secret_key(
            &lua,
            (
                secrets.clone(),
                "encryption".to_string(),
                "enc-v2".to_string(),
                "enc-key-v2".to_string(),
            ),
        )
        .unwrap();

        let active = active_secret_key(&lua, (secrets, "encryption".to_string())).unwrap();
        let active_table = match active {
            Value::Table(t) => t,
            _ => panic!("Expected active key table"),
        };

        let active_id: String = active_table.get("id").unwrap();
        let active_key: String = active_table.get("key").unwrap();
        assert_eq!(active_id, "enc-v2");
        assert_eq!(active_key, "enc-key-v2");
    }
}
