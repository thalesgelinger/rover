use anyhow::{Result, anyhow};
use mlua::{Lua, MultiValue, ObjectLike, Table, Value};
use rover_openapi::generate_spec;
use rover_parser::analyze;
use rover_server::store::{NamespacedStore, SharedStore, StoreBackendType, StoreValue};
use rover_server::to_json::ToJson;
use rover_server::{
    Bytes, HttpMethod, MiddlewareChain, Route, RouteTable, RoverResponse, ServerConfig,
    SseResponse, WsRoute,
};
use rover_types::ValidationErrors;
use rover_ui::SharedSignalRuntime;
use rover_ui::scheduler::SharedScheduler;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::html::{get_rover_html, render_template_with_components};
use crate::{app_type::AppType, auto_table::AutoTable};

#[derive(Clone)]
struct IdempotencyEntry {
    expires_at: Instant,
    fingerprint: u64,
    response: RoverResponse,
}

#[derive(Clone)]
struct IdempotencyStoreContext {
    store: NamespacedStore,
}

fn to_rover_response(value: Value) -> mlua::Result<RoverResponse> {
    match value {
        Value::UserData(ud) => {
            if let Ok(response) = ud.borrow::<RoverResponse>() {
                Ok(response.clone())
            } else {
                Ok(RoverResponse::text(
                    500,
                    Bytes::from_static(b"Invalid userdata type"),
                    None,
                ))
            }
        }
        Value::String(s) => {
            let body = s.to_str()?.to_string();
            Ok(RoverResponse::text(200, Bytes::from(body), None))
        }
        Value::Table(table) => {
            let json = table.to_json_string().map_err(|e| {
                mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
            })?;
            Ok(RoverResponse::json(200, Bytes::from(json), None))
        }
        Value::Integer(i) => Ok(RoverResponse::text(200, Bytes::from(i.to_string()), None)),
        Value::Number(n) => Ok(RoverResponse::text(200, Bytes::from(n.to_string()), None)),
        Value::Boolean(b) => Ok(RoverResponse::text(200, Bytes::from(b.to_string()), None)),
        Value::Nil => Ok(RoverResponse::empty(204)),
        Value::Error(e) => Ok(RoverResponse::text(500, Bytes::from(e.to_string()), None)),
        _ => Ok(RoverResponse::text(
            500,
            Bytes::from_static(b"Unsupported return type"),
            None,
        )),
    }
}

fn content_type_to_static(content_type: &str) -> &'static str {
    match content_type {
        "application/json" => "application/json",
        "text/plain" => "text/plain",
        "text/html" => "text/html",
        "application/octet-stream" => "application/octet-stream",
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

fn encode_idempotency_entry(entry: &IdempotencyEntry) -> mlua::Result<Vec<u8>> {
    let json = serde_json::json!({
        "fingerprint": entry.fingerprint,
        "status": entry.response.status,
        "body": entry.response.body.as_ref(),
        "content_type": entry.response.content_type,
        "headers": entry.response.headers,
    });

    serde_json::to_vec(&json)
        .map_err(|e| mlua::Error::RuntimeError(format!("idempotency encode error: {}", e)))
}

fn decode_idempotency_entry(payload: &[u8], ttl: Duration) -> mlua::Result<IdempotencyEntry> {
    let value: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|e| mlua::Error::RuntimeError(format!("idempotency decode error: {}", e)))?;

    let fingerprint = value
        .get("fingerprint")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            mlua::Error::RuntimeError("idempotency payload missing fingerprint".to_string())
        })?;

    let status = value
        .get("status")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            mlua::Error::RuntimeError("idempotency payload missing status".to_string())
        })? as u16;

    let body: Vec<u8> = serde_json::from_value(value.get("body").cloned().ok_or_else(|| {
        mlua::Error::RuntimeError("idempotency payload missing body".to_string())
    })?)
    .map_err(|e| {
        mlua::Error::RuntimeError(format!("idempotency payload body decode error: {}", e))
    })?;

    let content_type = value
        .get("content_type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            mlua::Error::RuntimeError("idempotency payload missing content_type".to_string())
        })?;

    let headers = match value.get("headers") {
        Some(v) if !v.is_null() => Some(
            serde_json::from_value::<HashMap<String, String>>(v.clone()).map_err(|e| {
                mlua::Error::RuntimeError(format!(
                    "idempotency payload headers decode error: {}",
                    e
                ))
            })?,
        ),
        _ => None,
    };

    Ok(IdempotencyEntry {
        expires_at: Instant::now() + ttl,
        fingerprint,
        response: RoverResponse {
            status,
            body: Bytes::from(body),
            content_type: content_type_to_static(content_type),
            headers,
        },
    })
}

fn load_idempotency_entry(
    lua: &Lua,
    key: &str,
    ttl: Duration,
) -> mlua::Result<Option<IdempotencyEntry>> {
    if let Some(ctx) = lua.app_data_ref::<IdempotencyStoreContext>() {
        match ctx.store.get(key).map_err(|e| {
            mlua::Error::RuntimeError(format!("idempotency store read failed: {}", e))
        })? {
            Some(StoreValue::Bytes(payload)) => decode_idempotency_entry(&payload, ttl).map(Some),
            Some(StoreValue::String(payload)) => {
                decode_idempotency_entry(payload.as_bytes(), ttl).map(Some)
            }
            Some(_) => Ok(None),
            None => Ok(None),
        }
    } else {
        let now = Instant::now();
        let mut store = idempotency_store().lock().map_err(|_| {
            mlua::Error::RuntimeError("idempotency storage lock poisoned".to_string())
        })?;
        store.retain(|_, entry| entry.expires_at > now);
        Ok(store.get(key).cloned())
    }
}

fn save_idempotency_entry(
    lua: &Lua,
    key: &str,
    entry: IdempotencyEntry,
    ttl: Duration,
) -> mlua::Result<()> {
    if let Some(ctx) = lua.app_data_ref::<IdempotencyStoreContext>() {
        let payload = encode_idempotency_entry(&entry)?;
        ctx.store
            .set(key, StoreValue::Bytes(payload), Some(ttl))
            .map_err(|e| {
                mlua::Error::RuntimeError(format!("idempotency store write failed: {}", e))
            })
    } else {
        let mut store = idempotency_store().lock().map_err(|_| {
            mlua::Error::RuntimeError("idempotency storage lock poisoned".to_string())
        })?;
        store.insert(key.to_string(), entry);
        Ok(())
    }
}

static IDEMPOTENCY_STORE: OnceLock<Mutex<HashMap<String, IdempotencyEntry>>> = OnceLock::new();
static IDEMPOTENCY_ROUTE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn idempotency_store() -> &'static Mutex<HashMap<String, IdempotencyEntry>> {
    IDEMPOTENCY_STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub trait AppServer {
    fn create_server(&self, config: Table) -> Result<Table>;
}

impl AppServer for Lua {
    fn create_server(&self, config: Table) -> Result<Table> {
        let server = self.create_auto_table()?;
        let _ = server.set("__rover_app_type", Value::Integer(AppType::Server.to_i64()))?;
        let _ = server.set("config", config)?;

        let json_helper = self.create_table()?;

        let json_call = self.create_function(|lua, (_self, data): (Table, Value)| {
            // Helper to extract headers from table
            let extract_headers = |table: &Table| -> mlua::Result<
                Option<std::collections::HashMap<String, String>>,
            > {
                let headers_val: Value = table.get("headers")?;
                match headers_val {
                    Value::Nil => Ok(None),
                    Value::Table(headers_table) => {
                        let mut headers = std::collections::HashMap::new();
                        for pair in headers_table.pairs::<String, String>() {
                            let (key, value) = pair?;
                            headers.insert(key, value);
                        }
                        Ok(Some(headers))
                    }
                    _ => Err(mlua::Error::RuntimeError(
                        "headers must be a table".to_string(),
                    )),
                }
            };

            match data {
                Value::String(s) => {
                    let json_str = s.to_str()?;
                    Ok(RoverResponse::json(
                        200,
                        Bytes::copy_from_slice(json_str.as_bytes()),
                        None,
                    ))
                }
                Value::Table(table) => {
                    let json = table.to_json_string().map_err(|e| {
                        mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                    })?;
                    let headers = extract_headers(&table)?;
                    Ok(RoverResponse::json(200, Bytes::from(json), headers))
                }
                _ => Err(mlua::Error::RuntimeError(
                    "api.json() requires a table or string".to_string(),
                )),
            }
        })?;

        let json_status_fn =
            self.create_function(|lua, (_self, status_code, data): (Table, u16, Value)| {
                // Helper to extract headers from table
                let extract_headers = |table: &Table| -> mlua::Result<
                    Option<std::collections::HashMap<String, String>>,
                > {
                    let headers_val: Value = table.get("headers")?;
                    match headers_val {
                        Value::Nil => Ok(None),
                        Value::Table(headers_table) => {
                            let mut headers = std::collections::HashMap::new();
                            for pair in headers_table.pairs::<String, String>() {
                                let (key, value) = pair?;
                                headers.insert(key, value);
                            }
                            Ok(Some(headers))
                        }
                        _ => Err(mlua::Error::RuntimeError(
                            "headers must be a table".to_string(),
                        )),
                    }
                };

                match data {
                    Value::String(s) => {
                        let json_str = s.to_str()?;
                        Ok(RoverResponse::json(
                            status_code,
                            Bytes::copy_from_slice(json_str.as_bytes()),
                            None,
                        ))
                    }
                    Value::Table(table) => {
                        let json = table.to_json_string().map_err(|e| {
                            mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                        })?;
                        let headers = extract_headers(&table)?;
                        Ok(RoverResponse::json(status_code, Bytes::from(json), headers))
                    }
                    _ => Err(mlua::Error::RuntimeError(
                        "api.json.status() requires a table or string".to_string(),
                    )),
                }
            })?;
        json_helper.set("status", json_status_fn)?;

        let meta = self.create_table()?;
        meta.set("__call", json_call)?;
        let _ = json_helper.set_metatable(Some(meta));
        server.set("json", json_helper)?;

        let text_helper = self.create_table()?;

        let text_call = self.create_function(|_lua, (_self, content): (Table, String)| {
            Ok(RoverResponse::text(
                200,
                Bytes::copy_from_slice(content.as_bytes()),
                None,
            ))
        })?;

        let text_status_fn = self.create_function(
            |_lua, (_self, status_code, content): (Table, u16, String)| {
                Ok(RoverResponse::text(
                    status_code,
                    Bytes::copy_from_slice(content.as_bytes()),
                    None,
                ))
            },
        )?;
        text_helper.set("status", text_status_fn)?;

        let text_meta = self.create_table()?;
        text_meta.set("__call", text_call)?;
        let _ = text_helper.set_metatable(Some(text_meta));
        server.set("text", text_helper)?;

        let html_helper = self.create_table()?;

        // Shared function to create HTML response builder
        fn create_html_response_builder(
            lua: &Lua,
            data: Value,
            status: u16,
        ) -> mlua::Result<Table> {
            let builder = lua.create_table()?;
            builder.set("__data", data)?;
            builder.set("__status", status)?;

            let builder_meta = lua.create_table()?;
            builder_meta.set(
                "__call",
                lua.create_function(|lua, (builder, template): (Table, String)| {
                    let data: Value = builder.get("__data")?;
                    let status: u16 = builder.get("__status")?;
                    let html_table = get_rover_html(lua)?;

                    let data_table = match data {
                        Value::Table(t) => t,
                        Value::Nil => lua.create_table()?,
                        _ => {
                            return Err(mlua::Error::RuntimeError(
                                "html() data must be a table or nil".to_string(),
                            ));
                        }
                    };

                    let rendered =
                        render_template_with_components(lua, &template, &data_table, &html_table)?;
                    Ok(RoverResponse::html(
                        status,
                        Bytes::copy_from_slice(rendered.as_bytes()),
                        None,
                    ))
                })?,
            )?;
            let _ = builder.set_metatable(Some(builder_meta));
            Ok(builder)
        }

        let html_call = self.create_function(|lua, (_self, data): (Table, Value)| {
            create_html_response_builder(lua, data, 200)
        })?;

        let html_status_fn =
            self.create_function(|lua, (_self, status_code, data): (Table, u16, Value)| {
                create_html_response_builder(lua, data, status_code)
            })?;
        html_helper.set("status", html_status_fn)?;

        let html_meta = self.create_table()?;
        html_meta.set("__call", html_call)?;
        let _ = html_helper.set_metatable(Some(html_meta));
        server.set("html", html_helper)?;

        let redirect_helper = self.create_table()?;

        let redirect_call = self.create_function(|_lua, (_self, location): (Table, String)| {
            Ok(RoverResponse::redirect(302, location))
        })?;

        let redirect_permanent =
            self.create_function(|_lua, (_self, location): (Table, String)| {
                Ok(RoverResponse::redirect(301, location))
            })?;
        redirect_helper.set("permanent", redirect_permanent)?;

        let redirect_status_fn = self.create_function(
            |_lua, (_self, status_code, location): (Table, u16, String)| {
                Ok(RoverResponse::redirect(status_code, location))
            },
        )?;
        redirect_helper.set("status", redirect_status_fn)?;

        let redirect_meta = self.create_table()?;
        redirect_meta.set("__call", redirect_call)?;
        let _ = redirect_helper.set_metatable(Some(redirect_meta));
        server.set("redirect", redirect_helper)?;

        let error_fn =
            self.create_function(|lua, (_self, (status, message)): (Table, (u16, Value))| {
                // Try ValidationErrors userdata (when passed directly without pcall stringification)
                if let Value::UserData(ref ud) = message {
                    if let Ok(verr) = ud.borrow::<ValidationErrors>() {
                        return Ok(RoverResponse::json(
                            status,
                            Bytes::from(verr.to_json_string()),
                            None,
                        ));
                    }
                }

                // Convert to string
                let message_str = match message {
                    Value::String(s) => s.to_str()?.to_string(),
                    Value::UserData(ud) => {
                        let tostring: mlua::Function = lua.globals().get("tostring")?;
                        tostring.call(Value::UserData(ud))?
                    }
                    other => {
                        let tostring: mlua::Function = lua.globals().get("tostring")?;
                        tostring.call(other)?
                    }
                };

                let mut message_str = message_str
                    .trim_start_matches("runtime error: ")
                    .to_string();
                if let Some(stack_pos) = message_str.find("\nstack traceback:") {
                    message_str = message_str[..stack_pos].to_string();
                }

                // Check if this is a stringified ValidationErrors (from pcall)
                if message_str.contains("Validation failed for request body:") {
                    // Parse the formatted string back to structured JSON
                    use rover_types::ValidationError;
                    let mut errors = Vec::new();
                    let lines: Vec<&str> = message_str.lines().collect();
                    let mut i = 0;

                    while i < lines.len() {
                        let line = lines[i];
                        if let Some(start) = line.find("Field '") {
                            if let Some(end) = line[start + 7..].find('\'') {
                                let field = &line[start + 7..start + 7 + end];
                                let mut error_msg = String::new();
                                let mut error_type = String::new();

                                if i + 1 < lines.len() {
                                    let next_line = lines[i + 1].trim();
                                    if next_line.starts_with("Error:") {
                                        error_msg = next_line
                                            .strip_prefix("Error:")
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                    }
                                }

                                if i + 2 < lines.len() {
                                    let type_line = lines[i + 2].trim();
                                    if type_line.starts_with("Type:") {
                                        error_type = type_line
                                            .strip_prefix("Type:")
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                    }
                                }

                                errors.push(ValidationError::new(field, &error_msg, &error_type));
                                i += 3;
                                continue;
                            }
                        }
                        i += 1;
                    }

                    if !errors.is_empty() {
                        let validation_errors = ValidationErrors::new(errors);
                        return Ok(RoverResponse::json(
                            status,
                            Bytes::from(validation_errors.to_json_string()),
                            None,
                        ));
                    }
                }

                // Generic error
                let table = lua.create_table()?;
                table.set("error", message_str)?;
                let json = table.to_json_string().map_err(|e| {
                    mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                })?;
                Ok(RoverResponse::json(
                    status,
                    Bytes::copy_from_slice(json.as_bytes()),
                    None,
                ))
            })?;
        server.set("error", error_fn)?;

        let no_content_fn = self.create_function(|_lua, _: Table| Ok(RoverResponse::empty(204)))?;
        server.set("no_content", no_content_fn)?;

        let idempotent_fn = self.create_function(|lua, args: MultiValue| {
                fn context_headers(lua: &Lua, ctx: &Value) -> mlua::Result<Table> {
                    match ctx {
                        Value::UserData(ud) => ud.call_method("headers", ()),
                        Value::Table(table) => {
                            let headers_fn: mlua::Function = table.get("headers")?;
                            headers_fn.call(table.clone())
                        }
                        _ => lua.create_table(),
                    }
                }

                fn context_body(ctx: &Value) -> Option<String> {
                    match ctx {
                        Value::UserData(ud) => {
                            let body_value: Value = ud.call_method("body", ()).ok()?;
                            match body_value {
                                Value::UserData(body_ud) => {
                                    body_ud.call_method::<String>("as_string", ()).ok()
                                }
                                Value::Table(body_table) => {
                                    let as_string: mlua::Function =
                                        body_table.get("as_string").ok()?;
                                    as_string.call(body_table).ok()
                                }
                                _ => None,
                            }
                        }
                        Value::Table(table) => {
                            let body_fn: mlua::Function = table.get("body").ok()?;
                            let body_value: Value = body_fn.call(table.clone()).ok()?;
                            match body_value {
                                Value::UserData(body_ud) => {
                                    body_ud.call_method::<String>("as_string", ()).ok()
                                }
                                Value::Table(body_table) => {
                                    let as_string: mlua::Function =
                                        body_table.get("as_string").ok()?;
                                    as_string.call(body_table).ok()
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }

                fn context_method(ctx: &Value) -> Option<String> {
                    match ctx {
                        Value::UserData(ud) => match ud.get::<Value>("method").ok()? {
                            Value::String(s) => Some(s.to_str().ok()?.to_string()),
                            Value::Integer(i) => Some(i.to_string()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        },
                        Value::Table(table) => match table.get::<Value>("method").ok()? {
                            Value::String(s) => Some(s.to_str().ok()?.to_string()),
                            Value::Integer(i) => Some(i.to_string()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        },
                        _ => None,
                    }
                }

                fn compute_fingerprint(method: &str, route_identity: &str, body: &str) -> u64 {
                    let mut hasher = DefaultHasher::new();
                    method.hash(&mut hasher);
                    route_identity.hash(&mut hasher);
                    body.hash(&mut hasher);
                    hasher.finish()
                }

                let values = args.into_vec();
                let (config, handler) = match values.as_slice() {
                    [Value::Function(handler)] => (None, handler.clone()),
                    [Value::Table(config), Value::Function(handler)] => {
                        (Some(config.clone()), handler.clone())
                    }
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "api.idempotent() expects (handler) or (config, handler)".to_string(),
                        ));
                    }
                };

                let header_name = if let Some(config) = &config {
                    config
                        .get::<Option<String>>("header")?
                        .unwrap_or_else(|| "Idempotency-Key".to_string())
                } else {
                    "Idempotency-Key".to_string()
                };

                let ttl_ms = if let Some(config) = &config {
                    config.get::<Option<u64>>("ttl_ms")?.unwrap_or(300_000)
                } else {
                    300_000
                };
                let ttl = Duration::from_millis(ttl_ms);

                let route_scope = format!(
                    "route:{}",
                    IDEMPOTENCY_ROUTE_COUNTER.fetch_add(1, Ordering::Relaxed)
                );
                let wrapped = lua.create_function(move |lua, ctx: Value| {
                    let headers = context_headers(lua, &ctx)?;
                    let mut idempotency_key: Option<String> = None;
                    for pair in headers.pairs::<String, Value>() {
                        let (key, value) = pair?;
                        if key.eq_ignore_ascii_case(&header_name) {
                            match value {
                                Value::String(s) => {
                                    let candidate = s.to_str()?.trim().to_string();
                                    if !candidate.is_empty() {
                                        idempotency_key = Some(candidate);
                                    }
                                }
                                Value::Integer(i) => {
                                    idempotency_key = Some(i.to_string());
                                }
                                Value::Number(n) => {
                                    idempotency_key = Some(n.to_string());
                                }
                                _ => {}
                            }
                            break;
                        }
                    }

                    let Some(idempotency_key) = idempotency_key else {
                        return handler.call::<Value>(ctx);
                    };

                    let method = context_method(&ctx).unwrap_or_default();
                    let body = context_body(&ctx).unwrap_or_default();
                    let fingerprint = compute_fingerprint(&method, &route_scope, &body);
                    let store_key = format!("{}:{}", route_scope, idempotency_key);

                    if let Some(entry) = load_idempotency_entry(lua, &store_key, ttl)? {
                        if entry.fingerprint != fingerprint {
                            let conflict = RoverResponse::json(
                                409,
                                Bytes::from_static(
                                    br#"{"error":"Idempotency key already used with different payload"}"#,
                                ),
                                None,
                            );
                            return lua.create_userdata(conflict).map(Value::UserData);
                        }
                        return lua.create_userdata(entry.response).map(Value::UserData);
                    }

                    let result: Value = handler.call(ctx.clone())?;
                    let response = to_rover_response(result)?;

                    save_idempotency_entry(
                        lua,
                        &store_key,
                        IdempotencyEntry {
                            expires_at: Instant::now() + ttl,
                            fingerprint,
                            response: response.clone(),
                        },
                        ttl,
                    )?;

                    lua.create_userdata(response).map(Value::UserData)
                })?;

                Ok(wrapped)
            })?;
        server.set("idempotent", idempotent_fn)?;

        let raw_helper = self.create_table()?;

        let raw_call = self.create_function(|_lua, body: mlua::String| {
            let body_str = body.to_str()?;
            Ok(RoverResponse::raw(
                200,
                Bytes::copy_from_slice(body_str.as_bytes()),
                None,
            ))
        })?;

        let raw_status_fn = self.create_function(
            |_lua, (_self, status_code, body): (Table, u16, mlua::String)| {
                let body_str = body.to_str()?;
                Ok(RoverResponse::raw(
                    status_code,
                    Bytes::copy_from_slice(body_str.as_bytes()),
                    None,
                ))
            },
        )?;
        raw_helper.set("status", raw_status_fn)?;

        let raw_meta = self.create_table()?;
        raw_meta.set("__call", raw_call)?;
        let _ = raw_helper.set_metatable(Some(raw_meta));
        server.set("raw", raw_helper)?;

        // Streaming response API
        // api.stream(status, content_type, chunk_producer) -> StreamingResponse
        // chunk_producer is a function that returns strings (chunks) or nil (end of stream)
        let stream_fn = self.create_function(
            |lua,
             (_self, status_code, content_type, chunk_producer): (
                Table,
                u16,
                String,
                mlua::Function,
            )| {
                use rover_server::StreamingResponse;
                use std::sync::Arc;

                let producer_key = lua.create_registry_value(chunk_producer)?;
                Ok(StreamingResponse::new(
                    status_code,
                    content_type,
                    None,
                    Arc::new(producer_key),
                ))
            },
        )?;
        server.set("stream", stream_fn)?;

        // Stream with headers: api.stream_with_headers(status, content_type, headers, chunk_producer)
        let stream_with_headers_fn = self.create_function(
            |lua,
             (_self, status_code, content_type, headers, chunk_producer): (
                Table,
                u16,
                String,
                Table,
                mlua::Function,
            )| {
                use rover_server::StreamingResponse;
                use std::sync::Arc;

                let mut response_headers = std::collections::HashMap::new();
                for pair in headers.pairs::<String, String>() {
                    let (key, value) = pair?;
                    response_headers.insert(key, value);
                }

                let producer_key = lua.create_registry_value(chunk_producer)?;
                Ok(StreamingResponse::new(
                    status_code,
                    content_type,
                    Some(response_headers),
                    Arc::new(producer_key),
                ))
            },
        )?;
        server.set("stream_with_headers", stream_with_headers_fn)?;

        let sse_helper = self.create_table()?;

        let sse_call = self.create_function(
            |lua, (_self, event_producer, retry_ms): (Table, mlua::Function, Option<u32>)| {
                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    200,
                    None,
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;

        let sse_status_fn = self.create_function(
            |lua,
             (_self, status_code, event_producer, retry_ms): (
                Table,
                u16,
                mlua::Function,
                Option<u32>,
            )| {
                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    status_code,
                    None,
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;
        sse_helper.set("status", sse_status_fn)?;

        let sse_with_headers_fn = self.create_function(
            |lua,
             (_self, status_code, headers, event_producer, retry_ms): (
                Table,
                u16,
                Table,
                mlua::Function,
                Option<u32>,
            )| {
                let mut response_headers = std::collections::HashMap::new();
                for pair in headers.pairs::<String, String>() {
                    let (key, value) = pair?;
                    response_headers.insert(key, value);
                }

                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    status_code,
                    Some(response_headers),
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;
        sse_helper.set("with_headers", sse_with_headers_fn)?;

        let sse_meta = self.create_table()?;
        sse_meta.set("__call", sse_call)?;
        let _ = sse_helper.set_metatable(Some(sse_meta));
        server.set("sse", sse_helper)?;

        Ok(server)
    }
}

pub trait Server {
    fn run_server(&self, lua: &Lua, source: &str) -> Result<()>;
    fn get_routes(&self, lua: &Lua) -> Result<RouteTable>;
}

impl Server for Table {
    fn run_server(&self, lua: &Lua, source: &str) -> Result<()> {
        let routes = self.get_routes(lua)?;
        let config: ServerConfig = self.get("config")?;

        // Generate OpenAPI spec if docs enabled
        let openapi_spec = if config.docs {
            let model = analyze(source);
            Some(generate_spec(&model, "API", "1.0.0"))
        } else {
            None
        };

        let runtime = lua.app_data_ref::<SharedSignalRuntime>().map(|r| r.clone());
        let scheduler = lua.app_data_ref::<SharedScheduler>().map(|s| s.clone());
        let idempotency_store = match config.idempotency.backend {
            StoreBackendType::InMemory => SharedStore::memory(),
            StoreBackendType::Sqlite => {
                let path =
                    config.idempotency.sqlite_path.as_deref().ok_or_else(|| {
                        anyhow!("idempotency backend 'sqlite' requires sqlite_path")
                    })?;
                SharedStore::sqlite(path).map_err(|e| {
                    anyhow!(
                        "failed to initialize idempotency sqlite store at '{}': {}",
                        path,
                        e
                    )
                })?
            }
        };
        let idempotency_store_context = IdempotencyStoreContext {
            store: idempotency_store.namespace_strict("idempotency"),
        };

        let server_lua = lua.clone();
        if let Some(runtime) = runtime {
            server_lua.set_app_data(runtime);
        }
        if let Some(scheduler) = scheduler {
            server_lua.set_app_data(scheduler);
        }
        server_lua.set_app_data(idempotency_store_context);

        rover_server::run(server_lua, routes, config, openapi_spec);
        Ok(())
    }

    fn get_routes(&self, lua: &Lua) -> Result<RouteTable> {
        fn create_static_mount_handler(
            lua: &Lua,
            mount_dir: PathBuf,
            mount_param_name: String,
            cache_control: Option<String>,
        ) -> Result<mlua::Function> {
            let handler = lua.create_function(move |lua, ctx: Value| {
                let (params_table, headers_table): (Table, Table) = match ctx {
                    Value::UserData(ud) => {
                        let params: Table = ud.call_method("params", ())?;
                        let headers: Table = ud.call_method("headers", ())?;
                        (params, headers)
                    }
                    Value::Table(t) => {
                        let params_fn: mlua::Function = t.get("params")?;
                        let headers_fn: mlua::Function = t.get("headers")?;
                        let params: Table = params_fn.call(())?;
                        let headers: Table = headers_fn.call(())?;
                        (params, headers)
                    }
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "static mount handler expected request context, got {:?}",
                            other
                        )));
                    }
                };

                let mounted_path = params_table
                    .get::<Option<String>>(mount_param_name.as_str())?
                    .unwrap_or_default();

                let mut request_headers = HashMap::new();
                for pair in headers_table.pairs::<String, String>() {
                    let (key, value) = pair?;
                    request_headers.insert(key, value);
                }

                let request_headers_ref = if request_headers.is_empty() {
                    None
                } else {
                    Some(&request_headers)
                };

                let custom_headers = cache_control.as_ref().map(|cache| {
                    let mut headers = HashMap::new();
                    headers.insert("Cache-Control".to_string(), cache.clone());
                    headers
                });

                let response = rover_server::serve_static_file(
                    mount_dir.as_path(),
                    &mounted_path,
                    request_headers_ref,
                    custom_headers,
                );

                lua.create_userdata(response).map(Value::UserData)
            })?;

            Ok(handler)
        }

        fn extract_static_mount_route(
            lua: &Lua,
            table: &Table,
            current_path: &str,
            routes: &mut Vec<Route>,
        ) -> Result<()> {
            let mount_config = match table.raw_get::<Value>("__rover_static_mount")? {
                Value::Table(config) => config,
                Value::Nil => return Ok(()),
                _ => {
                    return Err(anyhow!(
                        "Invalid static mount at '{}': expected table config",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
            };

            let mount_dir = match mount_config.get::<Value>("dir")? {
                Value::String(s) => s.to_str()?.trim().to_string(),
                Value::Nil => {
                    return Err(anyhow!(
                        "Missing 'dir' in static mount at '{}'",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
                _ => {
                    return Err(anyhow!(
                        "Invalid 'dir' in static mount at '{}': expected string",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
            };

            if mount_dir.is_empty() {
                return Err(anyhow!(
                    "Invalid static mount at '{}': 'dir' cannot be empty",
                    if current_path.is_empty() {
                        "/"
                    } else {
                        current_path
                    }
                ));
            }

            let mount_base = if current_path.is_empty() {
                "/"
            } else {
                current_path
            };
            let cache_control = match mount_config.get::<Value>("cache")? {
                Value::Nil => None,
                Value::String(s) => {
                    let value = s.to_str()?.trim().to_string();
                    if value.is_empty() {
                        return Err(anyhow!(
                            "Invalid 'cache' in static mount at '{}': value cannot be empty",
                            if current_path.is_empty() {
                                "/"
                            } else {
                                current_path
                            }
                        ));
                    }
                    Some(value)
                }
                _ => {
                    return Err(anyhow!(
                        "Invalid 'cache' in static mount at '{}': expected string",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
            };
            let mount_param_name = "__rover_mount_path".to_string();
            let pattern = if mount_base == "/" {
                format!("/{{*{}}}", mount_param_name)
            } else {
                format!(
                    "{}/{{*{}}}",
                    mount_base.trim_end_matches('/'),
                    mount_param_name
                )
            };
            let handler = create_static_mount_handler(
                lua,
                PathBuf::from(mount_dir),
                mount_param_name.clone(),
                cache_control,
            )?;

            routes.push(Route {
                method: HttpMethod::Get,
                pattern: Bytes::from(pattern),
                param_names: vec![mount_param_name],
                handler,
                is_static: false,
                middlewares: MiddlewareChain::default(),
            });

            Ok(())
        }

        fn extract_recursive(
            lua: &Lua,
            table: &Table,
            current_path: &str,
            param_names: &mut Vec<String>,
            routes: &mut Vec<Route>,
            ws_routes: &mut Vec<WsRoute>,
            inherited_middlewares: &crate::middleware::MiddlewareChain,
        ) -> Result<()> {
            let local_middlewares = crate::middleware::extract_middlewares(lua, table)?;
            let scope_middlewares = crate::middleware::merge_middleware_chains(
                inherited_middlewares,
                &local_middlewares,
            );

            extract_static_mount_route(lua, table, current_path, routes)?;

            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;

                // Skip internal rover fields and API helpers at root level
                // Note: "before", "after", and "on_error" are only skipped at root level
                // At nested levels, they're route-specific and should be processed
                if let Value::String(ref key_str) = key {
                    let key_str_val = key_str.to_str()?;
                    let is_root = current_path.is_empty();
                    if key_str_val.starts_with("__rover_")
                        || key_str_val == "config"
                        || key_str_val == "json"
                        || key_str_val == "text"
                        || key_str_val == "html"
                        || key_str_val == "redirect"
                        || key_str_val == "error"
                        || key_str_val == "no_content"
                        || key_str_val == "raw"
                        || key_str_val == "stream"
                        || key_str_val == "stream_with_headers"
                        || key_str_val == "sse"
                        || key_str_val == "idempotent"
                        || (is_root
                            && (key_str_val == "before"
                                || key_str_val == "after"
                                || key_str_val == "on_error"))
                    {
                        continue;
                    }
                }

                match (key, value) {
                    (Value::String(key_str), Value::Function(func)) => {
                        let key_string = key_str.to_str()?.to_string();

                        if key_string == "static" {
                            continue;
                        }

                        let path = if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        };

                        // WebSocket endpoint: function api.chat.ws(ws) ... end
                        if key_string == "ws" {
                            let ws_route = extract_ws_endpoint(lua, &func, path, param_names)?;
                            ws_routes.push(ws_route);
                            continue;
                        }

                        let method = HttpMethod::from_str(&key_string).ok_or_else(|| {
                            anyhow!(
                                "Unknown HTTP method '{}' at path '{}'. Valid methods: {}",
                                key_string,
                                path,
                                HttpMethod::valid_methods().join(", ")
                            )
                        })?;

                        // Create wrapped handler that executes middleware chain
                        let wrapped_handler = if scope_middlewares.is_empty() {
                            func.clone()
                        } else {
                            create_middleware_wrapper(lua, &func, &scope_middlewares)?
                        };

                        let route = Route {
                            method,
                            pattern: Bytes::from(path.to_string()),
                            param_names: param_names.clone(),
                            handler: wrapped_handler,
                            is_static: param_names.is_empty(),
                            middlewares: MiddlewareChain::default(), // Chain is now in the wrapper
                        };
                        routes.push(route);
                    }
                    (Value::String(key_str), Value::Table(nested_table)) => {
                        let key_string = key_str.to_str()?.to_string();

                        // Skip middleware tables (before/after) - they're extracted separately
                        if key_string == "before" || key_string == "after" {
                            continue;
                        }

                        // Check if this is a parameter segment - convert to matchit format immediately
                        let (segment, param_name) = if key_string.starts_with("p_") {
                            let param = key_string.strip_prefix("p_").unwrap();
                            if param.is_empty() {
                                return Err(anyhow!(
                                    "Empty parameter name at path '{}'",
                                    current_path
                                ));
                            }
                            (format!("{{{}}}", param), Some(param.to_string()))
                        } else {
                            (key_string.clone(), None)
                        };

                        let new_path = if current_path.is_empty() {
                            format!("/{}", segment)
                        } else {
                            format!("{}/{}", current_path, segment)
                        };

                        // Add param name if this is a parameter segment
                        if let Some(param) = param_name {
                            param_names.push(param);
                        }

                        extract_recursive(
                            lua,
                            &nested_table,
                            &new_path,
                            param_names,
                            routes,
                            ws_routes,
                            &scope_middlewares,
                        )?;

                        // Remove param name after recursion
                        if key_string.starts_with("p_") {
                            param_names.pop();
                        }
                    }
                    (k, v) => {
                        return Err(anyhow!(
                            "Invalid server config at path '{}': expected string key with table/function value, got {:?} = {:?}",
                            if current_path.is_empty() {
                                "/"
                            } else {
                                current_path
                            },
                            k,
                            v
                        ));
                    }
                }
            }
            Ok(())
        }

        /// Create a wrapper function that executes middleware chain before/after the handler
        fn create_middleware_wrapper(
            lua: &Lua,
            handler: &mlua::Function,
            middlewares: &MiddlewareChain,
        ) -> Result<mlua::Function> {
            let before_handlers: Vec<_> = middlewares
                .before
                .iter()
                .map(|mw| lua.registry_value::<mlua::Function>(&mw.handler))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to get before middleware: {}", e))?;

            let after_handlers: Vec<_> = middlewares
                .after
                .iter()
                .map(|mw| lua.registry_value::<mlua::Function>(&mw.handler))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to get after middleware: {}", e))?;

            let handler = handler.clone();

            // Create wrapper function
            let wrapper = lua.create_function(move |lua, ctx: mlua::Value| {
                // Execute before middlewares
                for (i, mw) in before_handlers.iter().enumerate() {
                    let result: mlua::Value = mw.call(ctx.clone())?;

                    // Check if middleware returned a response (short-circuit)
                    // If it's not nil and not the context, treat it as a response
                    if !matches!(result, mlua::Value::Nil) {
                        // If the middleware wants to short-circuit, return its result
                        // We check if it's a RoverResponse by trying to borrow it
                        if let Some(ud) = result.as_userdata() {
                            if ud.borrow::<rover_server::RoverResponse>().is_ok() {
                                return Ok(result);
                            }
                        }
                    }
                }

                // Execute the actual handler
                let result: mlua::Value = handler.call(ctx.clone())?;

                // Execute after middlewares (in reverse order)
                for mw in after_handlers.iter().rev() {
                    let _after_result: mlua::Value = mw.call(ctx.clone())?;
                    // After middlewares typically don't modify the response
                }

                Ok(result)
            })?;

            Ok(wrapper)
        }

        /// Extract a WebSocket endpoint from its setup function.
        ///
        /// Calls the setup function with a fresh ws DSL table, then extracts
        /// the captured join/leave/listen handlers into a WsEndpointConfig.
        fn extract_ws_endpoint(
            lua: &Lua,
            setup_fn: &mlua::Function,
            path: &str,
            param_names: &[String],
        ) -> Result<WsRoute> {
            use ahash::AHashMap;
            use rover_server::ws_lua::create_ws_table;
            use rover_server::ws_manager::WsEndpointConfig;

            // Create the ws DSL table (captures handler assignments via metamethods)
            let ws_table = create_ws_table(lua)
                .map_err(|e| anyhow!("Failed to create WS table for '{}': {}", path, e))?;

            // Execute: function api.x.ws(ws) ... end
            setup_fn
                .call::<()>(ws_table.clone())
                .map_err(|e| anyhow!("WS setup function failed at '{}': {}", path, e))?;

            // Extract join handler (stored as __ws_join by metamethod)
            let join_handler = match ws_table.raw_get::<Value>("__ws_join") {
                Ok(Value::Function(f)) => Some(lua.create_registry_value(f)?),
                _ => None,
            };

            // Extract leave handler (stored as __ws_leave by metamethod)
            let leave_handler = match ws_table.raw_get::<Value>("__ws_leave") {
                Ok(Value::Function(f)) => Some(lua.create_registry_value(f)?),
                _ => None,
            };

            // Extract listen event handlers from ws.listen.__ws_handlers table
            let listen_table: Table = ws_table.raw_get("listen")?;
            let handlers_table: Table = listen_table.raw_get("__ws_handlers")?;

            let mut event_handlers = AHashMap::new();
            for pair in handlers_table.pairs::<String, mlua::Function>() {
                let (event_name, handler_fn) = pair?;
                let key = lua.create_registry_value(handler_fn)?;
                event_handlers.insert(event_name, key);
            }

            // Store the ws table itself in registry (needed at runtime for send context)
            let ws_table_key = lua.create_registry_value(ws_table)?;

            let config = WsEndpointConfig {
                join_handler,
                leave_handler,
                event_handlers,
                ws_table_key,
            };

            Ok(WsRoute {
                pattern: Bytes::from(path.to_string()),
                param_names: param_names.to_vec(),
                is_static: param_names.is_empty(),
                endpoint_config: config,
            })
        }

        let mut routes = Vec::new();
        let mut ws_routes = Vec::new();
        let mut param_names = Vec::new();
        let root_middlewares = MiddlewareChain::default();
        extract_recursive(
            lua,
            self,
            "",
            &mut param_names,
            &mut routes,
            &mut ws_routes,
            &root_middlewares,
        )?;

        // Sort routes: static routes first (for exact-match priority)
        routes.sort_by(|a, b| match (a.is_static, b.is_static) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        // Extract optional error handler (api.on_error)
        let error_handler = if let Ok(Value::Function(handler)) = self.get("on_error") {
            Some(Arc::new(lua.create_registry_value(handler)?))
        } else {
            None
        };

        Ok(RouteTable {
            routes,
            ws_routes,
            error_handler,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AppServer, Server};
    use mlua::{Lua, Table, Value};
    use rover_server::{HttpMethod, RoverResponse, SseResponse};
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    fn create_ctx(lua: &Lua, headers: &[(&str, &str)]) -> mlua::Result<Table> {
        let ctx = lua.create_table()?;
        let state = lua.create_table()?;
        let headers_table = lua.create_table()?;

        for (k, v) in headers {
            headers_table.set(*k, *v)?;
        }

        let headers_clone = headers_table.clone();
        ctx.set(
            "headers",
            lua.create_function(move |_lua, _self: Table| Ok(headers_clone.clone()))?,
        )?;

        let state_set = state.clone();
        ctx.set(
            "set",
            lua.create_function(move |_lua, (_self, key, value): (Table, String, Value)| {
                state_set.set(key, value)?;
                Ok(())
            })?,
        )?;

        let state_get = state.clone();
        ctx.set(
            "get",
            lua.create_function(move |_lua, (_self, key): (Table, String)| {
                state_get.get::<Value>(key)
            })?,
        )?;

        Ok(ctx)
    }

    fn create_ctx_with_body(
        lua: &Lua,
        headers: &[(&str, &str)],
        body: &str,
    ) -> mlua::Result<Table> {
        let ctx = create_ctx(lua, headers)?;
        let body_text = body.to_string();
        ctx.set(
            "body",
            lua.create_function(move |lua, _self: Table| {
                let body_ud = lua.create_table()?;
                let text = body_text.clone();
                body_ud.set(
                    "as_string",
                    lua.create_function(move |_lua, _inner: Table| Ok(text.clone()))?,
                )?;
                Ok(body_ud)
            })?,
        )?;
        Ok(ctx)
    }

    fn parse_response(value: Value) -> (u16, String) {
        let response_ud = value
            .as_userdata()
            .expect("middleware wrapper must return RoverResponse userdata");
        let response = response_ud
            .borrow::<RoverResponse>()
            .expect("response userdata must be RoverResponse");
        let body = std::str::from_utf8(&response.body)
            .expect("response body must be utf-8")
            .to_string();
        (response.status, body)
    }

    fn parse_response_headers(value: Value) -> Option<std::collections::HashMap<String, String>> {
        let response_ud = value
            .as_userdata()
            .expect("route must return RoverResponse userdata");
        let response = response_ud
            .borrow::<RoverResponse>()
            .expect("response userdata must be RoverResponse");
        response.headers.clone()
    }

    #[test]
    fn should_support_single_static_mount() {
        let temp = tempdir().expect("create temp dir");
        let static_dir = temp.path().join("public");
        fs::create_dir_all(&static_dir).expect("create static dir");
        fs::write(static_dir.join("app.js"), "console.log('ok');").expect("write static file");

        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let dir_lua = static_dir
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!(
            r#"
            local api = rover.server {{}}
            api.assets.static {{ dir = "{}" }}
            return api
        "#,
            dir_lua
        );

        let app: Table = lua.load(&script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok()
                        == Some("/assets/{*__rover_mount_path}")
            })
            .expect("static mount route must exist");

        let ctx = lua.create_table().expect("create ctx table");
        let params = lua.create_table().expect("create params");
        params
            .set("__rover_mount_path", "app.js")
            .expect("set path param");
        ctx.set(
            "params",
            lua.create_function(move |_lua, ()| Ok(params.clone()))
                .expect("create params fn"),
        )
        .expect("set params fn");
        ctx.set(
            "headers",
            lua.create_function(|lua, ()| lua.create_table())
                .expect("create headers fn"),
        )
        .expect("set headers fn");

        let value = route
            .handler
            .call::<Value>(Value::Table(ctx))
            .expect("call static mount route");
        let (status, body) = parse_response(value);

        assert_eq!(status, 200);
        assert_eq!(body, "console.log('ok');");
    }

    #[test]
    fn should_support_multiple_static_mounts() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            api.assets.static { dir = "public" }
            api.uploads.static { dir = "uploads" }
            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");

        let patterns: Vec<String> = routes
            .routes
            .iter()
            .filter(|r| r.method == HttpMethod::Get)
            .map(|r| std::str::from_utf8(r.pattern.as_ref()).unwrap().to_string())
            .collect();

        assert!(patterns.contains(&"/assets/{*__rover_mount_path}".to_string()));
        assert!(patterns.contains(&"/uploads/{*__rover_mount_path}".to_string()));
    }

    #[test]
    fn should_map_static_cache_option_to_cache_control_header() {
        let temp = tempdir().expect("create temp dir");
        let static_dir = temp.path().join("public");
        fs::create_dir_all(&static_dir).expect("create static dir");
        fs::write(static_dir.join("app.js"), "console.log('ok');").expect("write static file");

        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let dir_lua = static_dir
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!(
            r#"
            local api = rover.server {{}}
            api.assets.static {{ dir = "{}", cache = "public, max-age=60" }}
            return api
        "#,
            dir_lua
        );

        let app: Table = lua.load(&script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok()
                        == Some("/assets/{*__rover_mount_path}")
            })
            .expect("static mount route must exist");

        let ctx = lua.create_table().expect("create ctx table");
        let params = lua.create_table().expect("create params");
        params
            .set("__rover_mount_path", "app.js")
            .expect("set path param");
        ctx.set(
            "params",
            lua.create_function(move |_lua, ()| Ok(params.clone()))
                .expect("create params fn"),
        )
        .expect("set params fn");
        ctx.set(
            "headers",
            lua.create_function(|lua, ()| lua.create_table())
                .expect("create headers fn"),
        )
        .expect("set headers fn");

        let value = route
            .handler
            .call::<Value>(Value::Table(ctx))
            .expect("call static mount route");
        let headers = parse_response_headers(value).expect("response headers");
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"public, max-age=60".to_string())
        );
    }

    #[test]
    fn should_apply_group_middlewares_to_nested_routes() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            function api.before.global(ctx)
              ctx:set("request_scope", "global")
            end

            function api.admin.before.authn(ctx)
              if not ctx:headers().Authorization then
                return api:error(401, "Unauthorized: missing Authorization header")
              end
              ctx:set("role", "admin")
            end

            function api.admin.users.get(ctx)
              return api.json {
                scope = ctx:get("request_scope"),
                role = ctx:get("role"),
              }
            end

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/admin/users")
            })
            .expect("route /admin/users must exist");

        let missing_auth_ctx = create_ctx(&lua, &[]).expect("create ctx without auth");
        let deny = route
            .handler
            .call::<Value>(missing_auth_ctx)
            .expect("call route without auth");
        let (deny_status, deny_body) = parse_response(deny);
        assert_eq!(deny_status, 401);
        assert!(deny_body.contains("Unauthorized: missing Authorization header"));

        let allowed_ctx =
            create_ctx(&lua, &[("Authorization", "Bearer test")]).expect("create ctx with auth");
        let ok = route
            .handler
            .call::<Value>(allowed_ctx)
            .expect("call route with auth");
        let (ok_status, ok_body) = parse_response(ok);
        assert_eq!(ok_status, 200);
        assert!(ok_body.contains("\"scope\":\"global\""));
        assert!(ok_body.contains("\"role\":\"admin\""));
    }

    #[test]
    fn should_build_sse_response_from_helper() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            function api.events.get(ctx)
              return api.sse(function()
                return {
                  event = "tick",
                  data = { ok = true },
                }
              end, 1500)
            end

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/events")
            })
            .expect("route /events must exist");

        let ctx = create_ctx(&lua, &[]).expect("create ctx");
        let value = route.handler.call::<Value>(ctx).expect("call SSE route");
        let sse = value
            .as_userdata()
            .expect("SSE helper must return userdata")
            .borrow::<SseResponse>()
            .expect("userdata must be SseResponse");

        assert_eq!(sse.status, 200);
        assert_eq!(sse.retry_ms, Some(1500));
    }

    #[test]
    fn should_support_rover_docs_static_assets_example_dsl() {
        let temp = tempdir().expect("create temp dir");
        let assets_dir = temp.path().join("public/assets");
        fs::create_dir_all(&assets_dir).expect("create assets dir");
        fs::write(assets_dir.join("site.css"), "body { color: blue; }").expect("write css");
        fs::write(assets_dir.join("app.js"), "console.log('ok');").expect("write js");

        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let dir_lua = assets_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("public/assets")
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");

        let script = format!(
            r#"
            local api = rover.server {{}}
            
            -- Static mount with cache settings (as used in rover-docs example)
            api.assets.static {{
                dir = "{}",
                cache = "public, max-age=31536000, immutable"
            }}
            
            -- Route precedence: API over static
            function api.assets.health.get(ctx)
                return {{
                    status = "healthy",
                    timestamp = os.time()
                }}
            end
            
            -- Dynamic route also takes precedence
            function api.assets.p_id.get(ctx)
                return {{
                    file_id = ctx:params().id,
                    requested_path = ctx.path
                }}
            end
            
            return api
        "#,
            dir_lua
        );

        let app: Table = lua
            .load(&script)
            .eval()
            .expect("load static-assets example app");
        let routes = app.get_routes(&lua).expect("get routes");

        // Should have static mount route
        let static_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok()
                        == Some("/assets/{*__rover_mount_path}")
            })
            .expect("static mount route must exist");

        // Should have API routes that take precedence
        let _health_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/assets/health")
            })
            .expect("health route must exist");

        let _dynamic_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/assets/{id}")
            })
            .expect("dynamic route must exist");

        // Test static file serving
        let ctx = lua.create_table().expect("create ctx table");
        let params = lua.create_table().expect("create params");
        params
            .set("__rover_mount_path", "site.css")
            .expect("set path param");
        ctx.set(
            "params",
            lua.create_function(move |_lua, ()| Ok(params.clone()))
                .expect("create params fn"),
        )
        .expect("set params fn");
        ctx.set(
            "headers",
            lua.create_function(|lua, ()| lua.create_table())
                .expect("create headers fn"),
        )
        .expect("set headers fn");

        let value = static_route
            .handler
            .call::<Value>(Value::Table(ctx))
            .expect("call static mount route");

        let (status, body) = parse_response(value.clone());

        assert_eq!(status, 200);
        assert_eq!(body, "body { color: blue; }");

        let headers = parse_response_headers(value).expect("headers must exist");
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"public, max-age=31536000, immutable".to_string())
        );
        assert!(headers.contains_key("ETag"));
    }

    #[test]
    fn should_support_rover_docs_uploads_demo_dsl() {
        let temp = tempdir().expect("create temp dir");
        let uploads_dir = temp.path().join("uploads");
        fs::create_dir_all(&uploads_dir).expect("create uploads dir");

        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let dir_lua = uploads_dir
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");

        let script = format!(
            r#"
            local api = rover.server {{}}
            
            -- Static mount for uploaded files (no cache)
            api.uploads.static {{
                dir = "{}",
                cache = "private, max-age=0, must-revalidate"
            }}
            
            -- File metadata endpoint (takes precedence over static)
            function api.uploads.p_filename.get(ctx)
                local filename = ctx:params().filename:gsub("[\\\\/]", "_")
                return {{
                    filename = filename,
                    requested = true
                }}
            end
            
            -- File delete endpoint
            function api.uploads.p_filename.delete(ctx)
                return api.json:status(200, {{ message = "deleted" }})
            end
            
            return api
        "#,
            dir_lua
        );

        let app: Table = lua
            .load(&script)
            .eval()
            .expect("load uploads-demo example app");
        let routes = app.get_routes(&lua).expect("get routes");

        // Should have static mount route
        let static_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok()
                        == Some("/uploads/{*__rover_mount_path}")
            })
            .expect("static mount route must exist");

        // Should have GET metadata route (takes precedence over static)
        let meta_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/uploads/{filename}")
            })
            .expect("metadata route must exist");

        // Should have DELETE route
        let delete_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Delete
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/uploads/{filename}")
            })
            .expect("delete route must exist");

        // Create a test file for static serving test
        fs::write(uploads_dir.join("test.txt"), "test content").expect("write test file");

        // Test static mount serves file with correct cache headers
        let ctx = lua.create_table().expect("create ctx table");
        let params = lua.create_table().expect("create params");
        params
            .set("__rover_mount_path", "test.txt")
            .expect("set path param");
        ctx.set(
            "params",
            lua.create_function(move |_lua, ()| Ok(params.clone()))
                .expect("create params fn"),
        )
        .expect("set params fn");
        ctx.set(
            "headers",
            lua.create_function(|lua, ()| lua.create_table())
                .expect("create headers fn"),
        )
        .expect("set headers fn");

        let value = static_route
            .handler
            .call::<Value>(Value::Table(ctx))
            .expect("call static mount route");

        let headers = parse_response_headers(value).expect("headers must exist");
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"private, max-age=0, must-revalidate".to_string())
        );

        // Verify both routes exist and have correct methods
        assert_eq!(meta_route.method, HttpMethod::Get);
        assert_eq!(delete_route.method, HttpMethod::Delete);
    }

    #[test]
    fn should_replay_response_for_duplicate_idempotency_key() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            local counter = 0

            api.orders.post = api.idempotent(function(ctx)
                counter = counter + 1
                return api.json { counter = counter }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");

        let first_ctx = create_ctx(&lua, &[("Idempotency-Key", "abc-123")]).expect("first ctx");
        let first = route
            .handler
            .call::<Value>(Value::Table(first_ctx))
            .expect("first call");
        let (_, first_body) = parse_response(first);

        let second_ctx = create_ctx(&lua, &[("Idempotency-Key", "abc-123")]).expect("second ctx");
        let second = route
            .handler
            .call::<Value>(Value::Table(second_ctx))
            .expect("second call");
        let (_, second_body) = parse_response(second);

        assert_eq!(first_body, "{\"counter\":1}");
        assert_eq!(second_body, "{\"counter\":1}");
    }

    #[test]
    fn should_replay_plain_table_response_for_duplicate_idempotency_key() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            local counter = 0

            api.orders.post = api.idempotent(function(ctx)
                counter = counter + 1
                return { counter = counter }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");

        let first = route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("Idempotency-Key", "plain-table-1")]).expect("first ctx"),
            ))
            .expect("first call");
        let (_, first_body) = parse_response(first);

        let second = route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("Idempotency-Key", "plain-table-1")]).expect("second ctx"),
            ))
            .expect("second call");
        let (_, second_body) = parse_response(second);

        assert_eq!(first_body, "{\"counter\":1}");
        assert_eq!(second_body, "{\"counter\":1}");
    }

    #[test]
    fn should_reject_reused_key_with_different_payload() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            api.orders.post = api.idempotent(function(ctx)
                return api.json { ok = true }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");

        let first_ctx = create_ctx_with_body(&lua, &[("Idempotency-Key", "reuse-1")], "{\"a\":1}")
            .expect("first ctx");
        route
            .handler
            .call::<Value>(Value::Table(first_ctx))
            .expect("first call");

        let second_ctx = create_ctx_with_body(&lua, &[("Idempotency-Key", "reuse-1")], "{\"a\":2}")
            .expect("second ctx");
        let conflict = route
            .handler
            .call::<Value>(Value::Table(second_ctx))
            .expect("second call");
        let (status, body) = parse_response(conflict);

        assert_eq!(status, 409);
        assert!(body.contains("Idempotency key already used with different payload"));
    }

    #[test]
    fn should_reject_reused_key_with_different_method() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            api.orders.post = api.idempotent(function(ctx)
                return api.json { ok = true }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");

        let first_ctx =
            create_ctx_with_body(&lua, &[("Idempotency-Key", "reuse-method")], "{\"a\":1}")
                .expect("first ctx");
        first_ctx.set("method", "POST").expect("set first method");
        route
            .handler
            .call::<Value>(Value::Table(first_ctx))
            .expect("first call");

        let second_ctx =
            create_ctx_with_body(&lua, &[("Idempotency-Key", "reuse-method")], "{\"a\":1}")
                .expect("second ctx");
        second_ctx.set("method", "PUT").expect("set second method");
        let conflict = route
            .handler
            .call::<Value>(Value::Table(second_ctx))
            .expect("second call");
        let (status, body) = parse_response(conflict);

        assert_eq!(status, 409);
        assert!(body.contains("Idempotency key already used with different payload"));
    }

    #[test]
    fn should_honor_custom_idempotency_header_per_route() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            local orders_counter = 0
            local payments_counter = 0

            api.orders.post = api.idempotent({ header = "X-Orders-Key" }, function(ctx)
                orders_counter = orders_counter + 1
                return api.json { counter = orders_counter }
            end)

            api.payments.post = api.idempotent({ header = "X-Payments-Key" }, function(ctx)
                payments_counter = payments_counter + 1
                return api.json { counter = payments_counter }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let orders_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");
        let payments_route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/payments")
            })
            .expect("route /payments must exist");

        let orders_first = orders_route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("X-Orders-Key", "orders-1")]).expect("orders first ctx"),
            ))
            .expect("orders first call");
        let (_, orders_first_body) = parse_response(orders_first);

        let orders_second = orders_route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("X-Orders-Key", "orders-1")]).expect("orders second ctx"),
            ))
            .expect("orders second call");
        let (_, orders_second_body) = parse_response(orders_second);

        let payments_first = payments_route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("X-Payments-Key", "payments-1")]).expect("payments first ctx"),
            ))
            .expect("payments first call");
        let (_, payments_first_body) = parse_response(payments_first);

        let payments_second = payments_route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("X-Payments-Key", "payments-1")]).expect("payments second ctx"),
            ))
            .expect("payments second call");
        let (_, payments_second_body) = parse_response(payments_second);

        assert_eq!(orders_first_body, "{\"counter\":1}");
        assert_eq!(orders_second_body, "{\"counter\":1}");
        assert_eq!(payments_first_body, "{\"counter\":1}");
        assert_eq!(payments_second_body, "{\"counter\":1}");
    }

    #[test]
    fn should_expire_idempotency_entry_with_custom_ttl() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            local counter = 0

            api.orders.post = api.idempotent({ ttl_ms = 5 }, function(ctx)
                counter = counter + 1
                return api.json { counter = counter }
            end)

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Post
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/orders")
            })
            .expect("route /orders must exist");

        let first = route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("Idempotency-Key", "ttl-1")]).expect("first ctx"),
            ))
            .expect("first call");
        let (_, first_body) = parse_response(first);

        sleep(Duration::from_millis(15));

        let second = route
            .handler
            .call::<Value>(Value::Table(
                create_ctx(&lua, &[("Idempotency-Key", "ttl-1")]).expect("second ctx"),
            ))
            .expect("second call");
        let (_, second_body) = parse_response(second);

        assert_eq!(first_body, "{\"counter\":1}");
        assert_eq!(second_body, "{\"counter\":2}");
    }
}
