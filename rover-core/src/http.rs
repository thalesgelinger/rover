use mlua::prelude::*;
use reqwest::{header::HeaderMap, Client, Method};
use serde_json::Value as JsonValue;
use std::time::Duration;

/// HTTP client configuration similar to axios
#[derive(Clone)]
pub struct HttpClient {
    base_url: Option<String>,
    default_headers: HeaderMap,
    timeout: Option<Duration>,
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            base_url: None,
            default_headers: HeaderMap::new(),
            timeout: Some(Duration::from_secs(30)),
        }
    }

    fn build_url(&self, path: &str) -> String {
        if let Some(ref base) = self.base_url {
            if path.starts_with("http://") || path.starts_with("https://") {
                path.to_string()
            } else {
                let base = base.trim_end_matches('/');
                let path = path.trim_start_matches('/');
                format!("{}/{}", base, path)
            }
        } else {
            path.to_string()
        }
    }
}

impl LuaUserData for HttpClient {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // GET request
        methods.add_async_method("get", |lua, this, (url, config): (String, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::GET, url, None, config).await
        });

        // POST request
        methods.add_async_method("post", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::POST, url, data, config).await
        });

        // PUT request
        methods.add_async_method("put", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::PUT, url, data, config).await
        });

        // DELETE request
        methods.add_async_method("delete", |lua, this, (url, config): (String, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::DELETE, url, None, config).await
        });

        // PATCH request
        methods.add_async_method("patch", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::PATCH, url, data, config).await
        });

        // HEAD request
        methods.add_async_method("head", |lua, this, (url, config): (String, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::HEAD, url, None, config).await
        });

        // OPTIONS request
        methods.add_async_method("options", |lua, this, (url, config): (String, Option<LuaTable>)| async move {
            let client = this.clone();
            make_request(&lua, &client, Method::OPTIONS, url, None, config).await
        });
    }
}

async fn make_request(
    lua: &Lua,
    client: &HttpClient,
    method: Method,
    url: String,
    data: Option<LuaValue>,
    config: Option<LuaTable>,
) -> LuaResult<LuaTable> {
    let full_url = client.build_url(&url);

    // Build reqwest client
    let mut req_client = Client::builder();
    if let Some(timeout) = client.timeout {
        req_client = req_client.timeout(timeout);
    }
    let req_client = req_client.build().map_err(|e| LuaError::external(e))?;

    // Build request
    let mut req = req_client.request(method.clone(), &full_url);

    // Add default headers
    for (key, value) in client.default_headers.iter() {
        req = req.header(key, value);
    }

    // Add config headers
    if let Some(ref cfg) = config {
        if let Ok(headers) = cfg.get::<LuaTable>("headers") {
            for pair in headers.pairs::<String, String>() {
                if let Ok((key, value)) = pair {
                    req = req.header(key, value);
                }
            }
        }

        // Add query params
        if let Ok(params) = cfg.get::<LuaTable>("params") {
            let mut query_params = Vec::new();
            for pair in params.pairs::<String, LuaValue>() {
                if let Ok((key, value)) = pair {
                    let value_str = match value {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        LuaValue::Integer(i) => i.to_string(),
                        LuaValue::Number(n) => n.to_string(),
                        LuaValue::Boolean(b) => b.to_string(),
                        _ => continue,
                    };
                    query_params.push((key, value_str));
                }
            }
            req = req.query(&query_params);
        }
    }

    // Add request body
    if let Some(body_data) = data {
        let json_str = lua_value_to_json(lua, &body_data)?;
        req = req.header("Content-Type", "application/json");
        req = req.body(json_str);
    }

    // Execute request
    let response = req.send().await.map_err(|e| LuaError::external(e))?;

    // Build response table
    let result = lua.create_table()?;
    result.set("status", response.status().as_u16())?;
    result.set("statusText", response.status().canonical_reason().unwrap_or(""))?;
    result.set("ok", response.status().is_success())?;

    // Response headers
    let headers_table = lua.create_table()?;
    for (key, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            headers_table.set(key.as_str(), value_str)?;
        }
    }
    result.set("headers", headers_table)?;

    // Response body
    let body_text = response.text().await.map_err(|e| LuaError::external(e))?;

    // Try to parse as JSON, fallback to text
    if let Ok(json_value) = serde_json::from_str::<JsonValue>(&body_text) {
        let lua_value = json_to_lua_value(lua, &json_value)?;
        result.set("data", lua_value)?;
    } else {
        result.set("data", body_text)?;
    }

    Ok(result)
}

fn lua_value_to_json(_lua: &Lua, value: &LuaValue) -> LuaResult<String> {
    match value {
        LuaValue::Table(table) => {
            // Convert Lua table to JSON using serde_json
            let json_value = table_to_json_value(table)?;
            serde_json::to_string(&json_value)
                .map_err(|e| LuaError::RuntimeError(format!("JSON serialization failed: {}", e)))
        }
        LuaValue::String(s) => Ok(format!("\"{}\"", s.to_str()?)),
        LuaValue::Integer(i) => Ok(i.to_string()),
        LuaValue::Number(n) => Ok(n.to_string()),
        LuaValue::Boolean(b) => Ok(b.to_string()),
        LuaValue::Nil => Ok("null".to_string()),
        _ => Err(LuaError::RuntimeError("Unsupported data type for JSON serialization".to_string())),
    }
}

fn table_to_json_value(table: &LuaTable) -> LuaResult<JsonValue> {
    // Check if it's an array (sequential numeric keys starting from 1)
    let mut is_array = true;
    let mut max_index = 0;

    for pair in table.clone().pairs::<LuaValue, LuaValue>() {
        if let Ok((key, _)) = pair {
            match key {
                LuaValue::Integer(i) if i > 0 => {
                    max_index = max_index.max(i as usize);
                }
                _ => {
                    is_array = false;
                    break;
                }
            }
        }
    }

    if is_array && max_index > 0 {
        // It's an array
        let mut arr = Vec::new();
        for i in 1..=max_index {
            if let Ok(value) = table.get::<LuaValue>(i) {
                arr.push(lua_value_to_json_value(&value)?);
            }
        }
        Ok(JsonValue::Array(arr))
    } else {
        // It's an object
        let mut obj = serde_json::Map::new();
        for pair in table.clone().pairs::<String, LuaValue>() {
            if let Ok((key, value)) = pair {
                obj.insert(key, lua_value_to_json_value(&value)?);
            }
        }
        Ok(JsonValue::Object(obj))
    }
}

fn lua_value_to_json_value(value: &LuaValue) -> LuaResult<JsonValue> {
    match value {
        LuaValue::Nil => Ok(JsonValue::Null),
        LuaValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
        LuaValue::Integer(i) => Ok(JsonValue::Number((*i).into())),
        LuaValue::Number(n) => {
            if let Some(num) = serde_json::Number::from_f64(*n) {
                Ok(JsonValue::Number(num))
            } else {
                Ok(JsonValue::Null)
            }
        }
        LuaValue::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        LuaValue::Table(table) => table_to_json_value(table),
        _ => Err(LuaError::RuntimeError("Unsupported Lua type for JSON".to_string())),
    }
}

fn json_to_lua_value(lua: &Lua, value: &JsonValue) -> LuaResult<LuaValue> {
    match value {
        JsonValue::Null => Ok(LuaValue::Nil),
        JsonValue::Bool(b) => Ok(LuaValue::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(LuaValue::Number(f))
            } else {
                Ok(LuaValue::Nil)
            }
        }
        JsonValue::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, item) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua_value(lua, item)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (key, val) in obj {
                table.set(key.as_str(), json_to_lua_value(lua, val)?)?;
            }
            Ok(LuaValue::Table(table))
        }
    }
}

/// Create the HTTP module for Lua
pub fn create_http_module(lua: &Lua) -> LuaResult<LuaTable> {
    let http = lua.create_table()?;
    let client = HttpClient::new();

    // Create convenience methods that use default client
    let client_get = client.clone();
    http.set("get", lua.create_async_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_get.clone();
        async move {
            make_request(&lua, &client, Method::GET, url, None, config).await
        }
    })?)?;

    let client_post = client.clone();
    http.set("post", lua.create_async_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_post.clone();
        async move {
            make_request(&lua, &client, Method::POST, url, data, config).await
        }
    })?)?;

    let client_put = client.clone();
    http.set("put", lua.create_async_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_put.clone();
        async move {
            make_request(&lua, &client, Method::PUT, url, data, config).await
        }
    })?)?;

    let client_delete = client.clone();
    http.set("delete", lua.create_async_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_delete.clone();
        async move {
            make_request(&lua, &client, Method::DELETE, url, None, config).await
        }
    })?)?;

    let client_patch = client.clone();
    http.set("patch", lua.create_async_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_patch.clone();
        async move {
            make_request(&lua, &client, Method::PATCH, url, data, config).await
        }
    })?)?;

    let client_head = client.clone();
    http.set("head", lua.create_async_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_head.clone();
        async move {
            make_request(&lua, &client, Method::HEAD, url, None, config).await
        }
    })?)?;

    let client_options = client.clone();
    http.set("options", lua.create_async_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_options.clone();
        async move {
            make_request(&lua, &client, Method::OPTIONS, url, None, config).await
        }
    })?)?;

    // Create method - creates a new HTTP client instance with custom config
    http.set("create", lua.create_function(|_lua, config: LuaTable| {
        let mut client = HttpClient::new();

        if let Ok(base_url) = config.get::<String>("baseURL") {
            client.base_url = Some(base_url);
        }

        if let Ok(timeout_ms) = config.get::<u64>("timeout") {
            client.timeout = Some(Duration::from_millis(timeout_ms));
        }

        if let Ok(headers) = config.get::<LuaTable>("headers") {
            for pair in headers.pairs::<String, String>() {
                if let Ok((key, value)) = pair {
                    if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                        if let Ok(header_value) = reqwest::header::HeaderValue::from_str(&value) {
                            client.default_headers.insert(header_name, header_value);
                        }
                    }
                }
            }
        }

        Ok(client)
    })?)?;

    Ok(http)
}
