use mlua::prelude::*;
use curl::easy::Easy;
use serde_json::Value as JsonValue;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

static HTTP_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct HttpClient {
    base_url: Option<String>,
    default_headers: Vec<(String, String)>,
    timeout: Option<Duration>,
}

/// Marker for HTTP requests that should yield
#[derive(Clone, Debug)]
pub struct HttpRequestDescriptor {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl LuaUserData for HttpRequestDescriptor {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("__rover_http_request", |_, _| Ok(true));
        fields.add_field_method_get("id", |_, this| Ok(this.id));
        fields.add_field_method_get("method", |_, this| Ok(this.method.clone()));
        fields.add_field_method_get("url", |_, this| Ok(this.url.clone()));
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            base_url: None,
            default_headers: Vec::new(),
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
        methods.add_method("get", |lua, this, (url, config): (String, Option<LuaTable>)| {
            let client = this.clone();
            // Check if we should yield (running in coroutine)
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "GET", url, None, config)
            } else {
                make_request(&lua, &client, "GET", url, None, config)
            }
        });

        methods.add_method("post", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "POST", url, data, config)
            } else {
                make_request(&lua, &client, "POST", url, data, config)
            }
        });

        methods.add_method("put", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "PUT", url, data, config)
            } else {
                make_request(&lua, &client, "PUT", url, data, config)
            }
        });

        methods.add_method("delete", |lua, this, (url, config): (String, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "DELETE", url, None, config)
            } else {
                make_request(&lua, &client, "DELETE", url, None, config)
            }
        });

        methods.add_method("patch", |lua, this, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "PATCH", url, data, config)
            } else {
                make_request(&lua, &client, "PATCH", url, data, config)
            }
        });

        methods.add_method("head", |lua, this, (url, config): (String, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "HEAD", url, None, config)
            } else {
                make_request(&lua, &client, "HEAD", url, None, config)
            }
        });

        methods.add_method("options", |lua, this, (url, config): (String, Option<LuaTable>)| {
            let client = this.clone();
            if should_yield_for_io(lua)? {
                yield_http_request(lua, &client, "OPTIONS", url, None, config)
            } else {
                make_request(&lua, &client, "OPTIONS", url, None, config)
            }
        });
    }
}

/// Check if we should yield for I/O (currently running in a coroutine)
fn should_yield_for_io(lua: &Lua) -> LuaResult<bool> {
    // Check if there's a running coroutine
    let globals = lua.globals();
    if let Ok(coroutine) = globals.get::<LuaTable>("coroutine") {
        if let Ok(running) = coroutine.get::<LuaFunction>("running") {
            // Call coroutine.running() - returns (thread, is_main)
            if let Ok((thread, is_main)) = running.call::<(LuaValue, bool)>(()) {
                // If not main and we have a thread, we should yield
                return Ok(!is_main && matches!(thread, LuaValue::Thread(_)));
            }
        }
    }
    Ok(false)
}

/// Yield for HTTP request - performs blocking I/O then yields to simulate async
fn yield_http_request(
    lua: &Lua,
    client: &HttpClient,
    method: &str,
    url: String,
    data: Option<LuaValue>,
    config: Option<LuaTable>,
) -> LuaResult<LuaTable> {
    // For now, perform the blocking request
    // TODO: Use curl::multi for true async when we have event loop polling
    let result = make_request(lua, client, method, url, data, config)?;

    // Yield to give event loop a chance to process other requests
    // This simulates async behavior even with blocking I/O
    let globals = lua.globals();
    if let Ok(coroutine) = globals.get::<LuaTable>("coroutine") {
        if let Ok(yield_fn) = coroutine.get::<LuaFunction>("yield") {
            // Yield and immediately return the result
            // This allows other coroutines to run while we "wait"
            let _ = yield_fn.call::<()>(());
        }
    }

    Ok(result)
}

fn make_request(
    lua: &Lua,
    client: &HttpClient,
    method: &str,
    url: String,
    data: Option<LuaValue>,
    config: Option<LuaTable>,
) -> LuaResult<LuaTable> {
    let full_url = client.build_url(&url);
    let mut easy = Easy::new();
    
    easy.url(&full_url).map_err(|e| LuaError::external(e))?;
    
    if let Some(timeout) = client.timeout {
        easy.timeout(timeout).map_err(|e| LuaError::external(e))?;
    }

    let mut headers = curl::easy::List::new();
    for (k, v) in &client.default_headers {
        headers.append(&format!("{}: {}", k, v)).map_err(|e| LuaError::external(e))?;
    }

    if let Some(ref cfg) = config {
        if let Ok(hdrs) = cfg.get::<LuaTable>("headers") {
            for pair in hdrs.pairs::<String, String>() {
                if let Ok((key, value)) = pair {
                    headers.append(&format!("{}: {}", key, value)).map_err(|e| LuaError::external(e))?;
                }
            }
        }

        if let Ok(params) = cfg.get::<LuaTable>("params") {
            let mut query_parts = Vec::new();
            for pair in params.pairs::<String, LuaValue>() {
                if let Ok((key, value)) = pair {
                    let value_str = match value {
                        LuaValue::String(s) => s.to_str()?.to_string(),
                        LuaValue::Integer(i) => i.to_string(),
                        LuaValue::Number(n) => n.to_string(),
                        LuaValue::Boolean(b) => b.to_string(),
                        _ => continue,
                    };
                    query_parts.push(format!("{}={}", 
                        urlencoding::encode(&key),
                        urlencoding::encode(&value_str)
                    ));
                }
            }
            if !query_parts.is_empty() {
                let new_url = if full_url.contains('?') {
                    format!("{}&{}", full_url, query_parts.join("&"))
                } else {
                    format!("{}?{}", full_url, query_parts.join("&"))
                };
                easy.url(&new_url).map_err(|e| LuaError::external(e))?;
            }
        }
    }

    match method {
        "GET" => easy.get(true).map_err(|e| LuaError::external(e))?,
        "POST" => easy.post(true).map_err(|e| LuaError::external(e))?,
        "PUT" => easy.put(true).map_err(|e| LuaError::external(e))?,
        "DELETE" => easy.custom_request("DELETE").map_err(|e| LuaError::external(e))?,
        "PATCH" => easy.custom_request("PATCH").map_err(|e| LuaError::external(e))?,
        "HEAD" => easy.nobody(true).map_err(|e| LuaError::external(e))?,
        "OPTIONS" => easy.custom_request("OPTIONS").map_err(|e| LuaError::external(e))?,
        _ => {}
    }

    if let Some(body_data) = data {
        let json_str = lua_value_to_json(lua, &body_data)?;
        let body_bytes = json_str.as_bytes();
        headers.append("Content-Type: application/json").map_err(|e| LuaError::external(e))?;
        easy.post_field_size(body_bytes.len() as u64).map_err(|e| LuaError::external(e))?;
        
        let mut body_data = body_bytes.to_vec();
        easy.read_function(move |buf| {
            let to_read = buf.len().min(body_data.len());
            buf[..to_read].copy_from_slice(&body_data[..to_read]);
            body_data.drain(..to_read);
            Ok(to_read)
        }).map_err(|e| LuaError::external(e))?;
    }

    easy.http_headers(headers).map_err(|e| LuaError::external(e))?;

    let mut response_data = Vec::new();
    let mut response_headers = Vec::new();
    
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            response_data.extend_from_slice(data);
            Ok(data.len())
        }).map_err(|e| LuaError::external(e))?;

        transfer.header_function(|header| {
            if let Ok(header_str) = std::str::from_utf8(header) {
                response_headers.push(header_str.to_string());
            }
            true
        }).map_err(|e| LuaError::external(e))?;

        transfer.perform().map_err(|e| LuaError::external(e))?;
    }

    let status_code = easy.response_code().map_err(|e| LuaError::external(e))?;
    
    let result = lua.create_table()?;
    result.set("status", status_code)?;
    result.set("ok", status_code >= 200 && status_code < 300)?;

    let headers_table = lua.create_table()?;
    for header in response_headers {
        if let Some((key, value)) = header.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                headers_table.set(key, value)?;
            }
        }
    }
    result.set("headers", headers_table)?;

    let body_text = String::from_utf8_lossy(&response_data).to_string();
    
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
        let mut arr = Vec::new();
        for i in 1..=max_index {
            if let Ok(value) = table.get::<LuaValue>(i) {
                arr.push(lua_value_to_json_value(&value)?);
            }
        }
        Ok(JsonValue::Array(arr))
    } else {
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

pub fn create_http_module(lua: &Lua) -> LuaResult<LuaTable> {
    let http = lua.create_table()?;
    let client = HttpClient::new();

    let client_get = client.clone();
    http.set("get", lua.create_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_get.clone();
        make_request(&lua, &client, "GET", url, None, config)
    })?)?;

    let client_post = client.clone();
    http.set("post", lua.create_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_post.clone();
        make_request(&lua, &client, "POST", url, data, config)
    })?)?;

    let client_put = client.clone();
    http.set("put", lua.create_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_put.clone();
        make_request(&lua, &client, "PUT", url, data, config)
    })?)?;

    let client_delete = client.clone();
    http.set("delete", lua.create_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_delete.clone();
        make_request(&lua, &client, "DELETE", url, None, config)
    })?)?;

    let client_patch = client.clone();
    http.set("patch", lua.create_function(move |lua, (url, data, config): (String, Option<LuaValue>, Option<LuaTable>)| {
        let client = client_patch.clone();
        make_request(&lua, &client, "PATCH", url, data, config)
    })?)?;

    let client_head = client.clone();
    http.set("head", lua.create_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_head.clone();
        make_request(&lua, &client, "HEAD", url, None, config)
    })?)?;

    let client_options = client.clone();
    http.set("options", lua.create_function(move |lua, (url, config): (String, Option<LuaTable>)| {
        let client = client_options.clone();
        make_request(&lua, &client, "OPTIONS", url, None, config)
    })?)?;

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
                    client.default_headers.push((key, value));
                }
            }
        }

        Ok(client)
    })?)?;

    Ok(http)
}
