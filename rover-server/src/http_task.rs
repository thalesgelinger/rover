use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use mlua::{Function, Lua, Table, Value, Thread, ThreadStatus};
use tracing::{debug, info, warn};

use crate::{to_json::ToJson, response::RoverResponse, Bytes};

pub struct HttpResponse {
    pub status: u16,
    pub body: Bytes,
    pub content_type: Option<String>,
}

pub struct ThreadPool {
    available: Vec<Thread>,
    max_size: usize,
}

impl ThreadPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            available: Vec::with_capacity(max_size),
            max_size,
        }
    }

    #[allow(unused_mut)]
    pub fn acquire(&mut self, lua: &Lua, handler: &Function) -> Result<Thread> {
        if let Some(mut thread) = self.available.pop() {
            thread.reset(handler.clone())?;
            Ok(thread)
        } else {
            Ok(lua.create_thread(handler.clone())?)
        }
    }

    pub fn release(&mut self, thread: Thread) {
        if thread.status() == ThreadStatus::Finished && self.available.len() < self.max_size {
            self.available.push(thread);
        }
    }
}

#[deprecated(note = "Use execute_handler_coroutine instead for non-blocking execution")]
pub fn execute_handler(
    lua: &Lua,
    handler: &Function,
    method: &str,
    path: &str,
    headers: &[(Bytes, Bytes)],
    query: &[(Bytes, Bytes)],
    params: &HashMap<String, String>,
    body: Option<&[u8]>,
    _started_at: Instant,
) -> Result<HttpResponse> {
    let ctx = match build_lua_context(lua, method, path, headers, query, params, body) {
        Ok(c) => c,
        Err(e) => {
            return Ok(HttpResponse {
                status: 500,
                body: Bytes::from(e.to_string()),
                content_type: None,
            });
        }
    };

    let result: Value = match handler.call(ctx) {
        Ok(r) => r,
        Err(e) => {
            return Ok(HttpResponse {
                status: 500,
                body: Bytes::from(e.to_string()),
                content_type: None,
            });
        }
    };

    let (status, body, content_type) = convert_lua_response(lua, result);
    Ok(HttpResponse { status, body, content_type })
}

pub enum CoroutineResponse {
    Ready { status: u16, body: Bytes, content_type: Option<String> },
    Yielded { thread: Thread },
}

pub fn execute_handler_coroutine(
    lua: &Lua,
    handler: &Function,
    method: &str,
    path: &str,
    headers: &[(Bytes, Bytes)],
    query: &[(Bytes, Bytes)],
    params: &HashMap<String, String>,
    body: Option<&[u8]>,
    started_at: Instant,
    pool: &mut ThreadPool,
) -> Result<CoroutineResponse> {
    if tracing::event_enabled!(tracing::Level::DEBUG) {
        if !query.is_empty() {
            debug!("  ├─ query: {:?}", query);
        }
        if let Some(body) = body {
            let body_display = std::str::from_utf8(body).unwrap_or("<binary data>");
            debug!("  └─ body: {}", body_display);
        }
    }

    let ctx = match build_lua_context(lua, method, path, headers, query, params, body) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = e.to_string();
            let status = if error_msg.contains("Invalid UTF-8") {
                400
            } else {
                500
            };
            return Ok(CoroutineResponse::Ready {
                status,
                body: Bytes::from(error_msg),
                content_type: None,
            });
        }
    };

    // Always use coroutines to support yielding I/O operations
    // The coroutine may complete immediately (fast path) or yield for I/O (slow path)
    let thread = pool.acquire(lua, handler)?;

    match thread.resume::<Value>(ctx) {
        Ok(result) => {
            // Check if the coroutine yielded or completed
            use mlua::ThreadStatus;
            match thread.status() {
                ThreadStatus::Resumable => {
                    // Handler yielded - return the thread to be resumed later
                    Ok(CoroutineResponse::Yielded { thread })
                }
                _ => {
                    // Handler completed without yielding (or died)
                    let (status, body, content_type) = convert_lua_response(lua, result);
                    let elapsed = started_at.elapsed();
                    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

                    if status >= 200 && status < 300 {
                        if tracing::event_enabled!(tracing::Level::INFO) {
                            info!("{} {} - {} in {:.2}ms", method, path, status, elapsed_ms);
                        }
                    } else if status >= 400 {
                        if tracing::event_enabled!(tracing::Level::WARN) {
                            warn!("{} {} - {} in {:.2}ms", method, path, status, elapsed_ms);
                        }
                    }

                    // Return finished thread to pool
                    pool.release(thread);

                    Ok(CoroutineResponse::Ready { status, body, content_type })
                }
            }
        }
        Err(e) => {
            // Error during execution
            convert_error_to_response(e, method, path, started_at)
        }
    }
}

fn convert_error_to_response(
    e: mlua::Error,
    method: &str,
    path: &str,
    started_at: Instant,
) -> Result<CoroutineResponse> {
    let validation_err = match &e {
        mlua::Error::ExternalError(arc_err) => arc_err.downcast_ref::<rover_types::ValidationErrors>(),
        mlua::Error::CallbackError { cause, .. } => {
            if let mlua::Error::ExternalError(arc_err) = cause.as_ref() {
                arc_err.downcast_ref::<rover_types::ValidationErrors>()
            } else {
                None
            }
        }
        _ => None,
    };

    let (status, body) = if let Some(validation_errors) = validation_err {
        (400, Bytes::from(validation_errors.to_json_string()))
    } else {
        let mut error_str = e.to_string();
        if let Some(stack_pos) = error_str.find("\nstack traceback:") {
            error_str = error_str[..stack_pos].to_string();
        }
        error_str = error_str.trim_start_matches("runtime error: ").to_string();
        (500, Bytes::from(format!("{{\"error\": \"{}\"}}", error_str.replace("\"", "\\\"").replace("\n", "\\n"))))
    };

    let elapsed = started_at.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    if status >= 400 {
        if tracing::event_enabled!(tracing::Level::WARN) {
            warn!("{} {} - {} in {:.2}ms", method, path, status, elapsed_ms);
        }
    }

    Ok(CoroutineResponse::Ready { status, body, content_type: None })
}

fn build_lua_context(
    lua: &Lua,
    method: &str,
    path: &str,
    headers: &[(Bytes, Bytes)],
    query: &[(Bytes, Bytes)],
    params: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> Result<Table> {
    let ctx = lua.create_table()?;

    ctx.set("method", method)?;
    ctx.set("path", path)?;

    // Phase 4: Remove Arc wrapping - clone data directly for better perf on small data
    let headers_data: Vec<(Bytes, Bytes)> = headers.to_vec();
    let headers_fn = lua.create_function(move |lua, ()| {
        if headers_data.is_empty() {
            return lua.create_table();
        }
        let headers_table = lua.create_table_with_capacity(0, headers_data.len())?;
        for (k, v) in &headers_data {
            // Phase 3: Skip UTF-8 validation for ASCII headers
            let k_str = unsafe { std::str::from_utf8_unchecked(k) };
            let v_str = unsafe { std::str::from_utf8_unchecked(v) };
            headers_table.set(k_str, v_str)?;
        }
        Ok(headers_table)
    })?;
    ctx.set("headers", headers_fn)?;

    let query_data: Vec<(Bytes, Bytes)> = query.to_vec();
    let query_fn = lua.create_function(move |lua, ()| {
        if query_data.is_empty() {
            return lua.create_table();
        }
        let query_table = lua.create_table_with_capacity(0, query_data.len())?;
        for (k, v) in &query_data {
            // Phase 3: Skip UTF-8 validation for URL-encoded ASCII params
            let k_str = unsafe { std::str::from_utf8_unchecked(k) };
            let v_str = unsafe { std::str::from_utf8_unchecked(v) };
            query_table.set(k_str, v_str)?;
        }
        Ok(query_table)
    })?;
    ctx.set("query", query_fn)?;

    let params_data: HashMap<String, String> = params.clone();
    let params_fn = lua.create_function(move |lua, ()| {
        if params_data.is_empty() {
            return lua.create_table();
        }
        let params_table = lua.create_table_with_capacity(0, params_data.len())?;
        for (k, v) in &params_data {
            params_table.set(k.as_str(), v.as_str())?;
        }
        Ok(params_table)
    })?;
    ctx.set("params", params_fn)?;

    let body_bytes = body.map(|b| b.to_vec());
    let body_fn = lua.create_function(move |lua, ()| {
        if let Some(ref body) = body_bytes {
            // Phase 3: Skip UTF-8 validation for performance
            let body_str = unsafe { std::str::from_utf8_unchecked(body) };

            let globals = lua.globals();
            let rover: Table = globals.get("rover")?;
            let guard: Table = rover.get("guard")?;

            if let Ok(constructor) = guard.get::<mlua::Function>("__body_value") {
                constructor.call((body_str.to_string(), body.clone()))
            } else {
                Ok(Value::String(lua.create_string(body_str)?))
            }
        } else {
            Err(mlua::Error::RuntimeError(
                "Request has no body".to_string(),
            ))
        }
    })?;
    ctx.set("body", body_fn)?;

    Ok(ctx)
}

fn convert_lua_response(_lua: &Lua, result: Value) -> (u16, Bytes, Option<String>) {
    match result {
        Value::UserData(ref ud) => {
            if let Ok(response) = ud.borrow::<RoverResponse>() {
                (
                    response.status,
                    response.body.clone(),
                    Some(response.content_type.clone()),
                )
            } else {
                (
                    500,
                    Bytes::from("Invalid userdata type"),
                    Some("text/plain".to_string()),
                )
            }
        }

        Value::String(ref s) => (
            200,
            Bytes::from(s.to_str().unwrap().to_string()),
            Some("text/plain".to_string()),
        ),

        Value::Table(table) => {
            let json = lua_table_to_json(&table).unwrap_or_else(|e| {
                format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
            });
            (200, Bytes::from(json), Some("application/json".to_string()))
        }

        Value::Integer(i) => (200, Bytes::from(i.to_string()), Some("text/plain".to_string())),
        Value::Number(n) => (200, Bytes::from(n.to_string()), Some("text/plain".to_string())),
        Value::Boolean(b) => (200, Bytes::from(b.to_string()), Some("text/plain".to_string())),
        Value::Nil => (204, Bytes::new(), None),

        Value::Error(e) => (
            500,
            Bytes::from(e.to_string()),
            Some("text/plain".to_string()),
        ),

        _ => (
            500,
            Bytes::from("Unsupported return type"),
            Some("text/plain".to_string()),
        ),
    }
}

fn lua_table_to_json(table: &Table) -> Result<String> {
    table
        .to_json_string()
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))
}
