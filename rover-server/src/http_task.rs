use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use mlua::{Function, Lua, Table, Value, Thread, ThreadStatus, UserData, UserDataMethods, RegistryKey};
use tracing::{debug, info, warn};

use crate::{to_json::ToJson, response::RoverResponse, Bytes};
use crate::table_pool::LuaTablePool;

pub struct RequestContext {
    buf: Bytes,
    
    method_off: u16,
    method_len: u8,
    
    path_off: u16,
    path_len: u16,
    
    body_off: u32,
    body_len: u32,
    
    headers: Vec<(u16, u8, u16, u16)>,
    query: Vec<(u16, u8, u16, u16)>,
    
    params: Vec<(Bytes, Bytes)>,
}

impl UserData for RequestContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("headers", |lua, this, ()| {
            if this.headers.is_empty() {
                return lua.create_table();
            }
            let headers_table = lua.create_table_with_capacity(0, this.headers.len())?;
            for &(name_off, name_len, val_off, val_len) in &this.headers {
                let name_bytes = &this.buf[name_off as usize..(name_off + name_len as u16) as usize];
                let val_bytes = &this.buf[val_off as usize..(val_off + val_len) as usize];
                let k_str = unsafe { std::str::from_utf8_unchecked(name_bytes) };
                let v_str = unsafe { std::str::from_utf8_unchecked(val_bytes) };
                headers_table.set(k_str, v_str)?;
            }
            Ok(headers_table)
        });

        methods.add_method("query", |lua, this, ()| {
            if this.query.is_empty() {
                return lua.create_table();
            }
            let query_table = lua.create_table_with_capacity(0, this.query.len())?;
            for &(name_off, name_len, val_off, val_len) in &this.query {
                let name_bytes = &this.buf[name_off as usize..(name_off + name_len as u16) as usize];
                let val_bytes = &this.buf[val_off as usize..(val_off + val_len) as usize];
                let k_str = unsafe { std::str::from_utf8_unchecked(name_bytes) };
                let v_str = unsafe { std::str::from_utf8_unchecked(val_bytes) };
                query_table.set(k_str, v_str)?;
            }
            Ok(query_table)
        });

        methods.add_method("params", |lua, this, ()| {
            if this.params.is_empty() {
                return lua.create_table();
            }
            let params_table = lua.create_table_with_capacity(0, this.params.len())?;
            for (k, v) in &this.params {
                let k_str = unsafe { std::str::from_utf8_unchecked(k) };
                let v_str = unsafe { std::str::from_utf8_unchecked(v) };
                params_table.set(k_str, v_str)?;
            }
            Ok(params_table)
        });

        methods.add_method("body", |lua, this, ()| {
            if this.body_len > 0 {
                let body_bytes = this.buf.slice(this.body_off as usize..(this.body_off + this.body_len) as usize);
                let body_value = BodyValue::new(body_bytes);
                lua.create_userdata(body_value).map(Value::UserData)
            } else {
                Err(mlua::Error::RuntimeError(
                    "Request has no body".to_string(),
                ))
            }
        });
    }
}

pub struct BodyValue {
    bytes: Bytes,
}

impl BodyValue {
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes }
    }
}

impl UserData for BodyValue {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("json", |lua, this, ()| {
            crate::direct_json_parser::json_bytes_ref_to_lua_direct(lua, &this.bytes)
        });

        methods.add_method("raw", |lua, this, ()| {
            crate::direct_json_parser::json_bytes_ref_to_lua_direct(lua, &this.bytes)
        });

        methods.add_method("text", |lua, this, ()| {
            let text_str = unsafe { std::str::from_utf8_unchecked(&this.bytes) };
            Ok(Value::String(lua.create_string(text_str)?))
        });

        methods.add_method("as_string", |lua, this, ()| {
            let text_str = unsafe { std::str::from_utf8_unchecked(&this.bytes) };
            Ok(Value::String(lua.create_string(text_str)?))
        });

        methods.add_method("echo", |lua, this, ()| {
            let text_str = unsafe { std::str::from_utf8_unchecked(&this.bytes) };
            Ok(Value::String(lua.create_string(text_str)?))
        });

        methods.add_method("bytes", |lua, this, ()| {
            let table = lua.create_table_with_capacity(this.bytes.len(), 0)?;
            for (i, byte) in this.bytes.iter().enumerate() {
                table.set(i + 1, *byte)?;
            }
            Ok(Value::Table(table))
        });
    }
}

pub struct RequestContextPool {
    pool: Vec<RegistryKey>,
    available: Vec<usize>,
    capacity: usize,
}

impl RequestContextPool {
    pub fn new(lua: &Lua, capacity: usize) -> mlua::Result<Self> {
        let mut pool = Vec::with_capacity(capacity);
        let mut available = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            let ctx = RequestContext {
                buf: Bytes::new(),
                method_off: 0,
                method_len: 0,
                path_off: 0,
                path_len: 0,
                body_off: 0,
                body_len: 0,
                headers: Vec::new(),
                query: Vec::new(),
                params: Vec::new(),
            };

            let userdata = lua.create_userdata(ctx)?;
            let key = lua.create_registry_value(userdata)?;
            pool.push(key);
            available.push(pool.len() - 1);
        }

        Ok(Self {
            pool,
            available,
            capacity,
        })
    }

    pub fn acquire(
        &mut self,
        lua: &Lua,
        buf: Bytes,
        method_off: u16,
        method_len: u8,
        path_off: u16,
        path_len: u16,
        body_off: u32,
        body_len: u32,
        headers: Vec<(u16, u8, u16, u16)>,
        query: Vec<(u16, u8, u16, u16)>,
        params: &[(Bytes, Bytes)],
    ) -> mlua::Result<(Value, usize)> {
        let idx = self.available.pop()
            .ok_or_else(|| mlua::Error::RuntimeError("RequestContextPool exhausted".to_string()))?;

        let key = &self.pool[idx];
        let userdata: mlua::AnyUserData = lua.registry_value(&key)?;

        let mut ctx = userdata.borrow_mut::<RequestContext>()?;
        ctx.buf = buf;
        ctx.method_off = method_off;
        ctx.method_len = method_len;
        ctx.path_off = path_off;
        ctx.path_len = path_len;
        ctx.body_off = body_off;
        ctx.body_len = body_len;
        ctx.headers.clear();
        ctx.headers.extend(headers);
        ctx.query.clear();
        ctx.query.extend(query);
        ctx.params.clear();
        ctx.params.extend_from_slice(params);
        drop(ctx);

        Ok((Value::UserData(userdata), idx))
    }

    pub fn release(&mut self, idx: usize) {
        if idx < self.capacity {
            self.available.push(idx);
        }
    }
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Bytes,
    pub content_type: Option<&'static str>,
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
    Ready { status: u16, body: Bytes, content_type: Option<&'static str> },
    Yielded { thread: Thread, ctx_idx: usize },
}

pub fn execute_handler_coroutine(
    lua: &Lua,
    handler: &Function,
    buf: Bytes,
    method_off: u16,
    method_len: u8,
    path_off: u16,
    path_len: u16,
    body_off: u32,
    body_len: u32,
    headers: Vec<(u16, u8, u16, u16)>,
    query: Vec<(u16, u8, u16, u16)>,
    params: &[(Bytes, Bytes)],
    started_at: Instant,
    thread_pool: &mut ThreadPool,
    request_pool: &mut RequestContextPool,
    _table_pool: &LuaTablePool,
) -> Result<CoroutineResponse> {
    if tracing::event_enabled!(tracing::Level::DEBUG) {
        if !query.is_empty() {
            debug!("  ├─ query: {:?}", query);
        }
        if body_len > 0 {
            let body_bytes = &buf[body_off as usize..(body_off + body_len) as usize];
            let body_display = std::str::from_utf8(body_bytes).unwrap_or("<binary data>");
            debug!("  └─ body: {}", body_display);
        }
    }

    let (ctx, ctx_idx) = match request_pool.acquire(lua, buf.clone(), method_off, method_len, path_off, path_len, body_off, body_len, headers, query, params) {
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
    let thread = thread_pool.acquire(lua, handler)?;

    match thread.resume::<Value>(ctx) {
        Ok(result) => {
            // Check if the coroutine yielded or completed
            use mlua::ThreadStatus;
            match thread.status() {
                ThreadStatus::Resumable => {
                    // Handler yielded - return the thread to be resumed later
                    Ok(CoroutineResponse::Yielded { thread, ctx_idx })
                }
                _ => {
                    // Handler completed without yielding (or died)
                    // Fast path: check for RoverResponse directly to avoid function call overhead
                    let (status, body, content_type) = if let Value::UserData(ref ud) = result {
                        if let Ok(response) = ud.borrow::<RoverResponse>() {
                            (response.status, response.body.clone(), Some(response.content_type))
                        } else {
                            convert_lua_response(lua, result)
                        }
                    } else {
                        convert_lua_response(lua, result)
                    };

                    let elapsed = started_at.elapsed();
                    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

                    if status >= 200 && status < 300 {
                        if tracing::event_enabled!(tracing::Level::INFO) {
                            let method_str = unsafe { std::str::from_utf8_unchecked(&buf[method_off as usize..(method_off + method_len as u16) as usize]) };
                            let path_str = unsafe { std::str::from_utf8_unchecked(&buf[path_off as usize..(path_off + path_len) as usize]) };
                            info!("{} {} - {} in {:.2}ms", method_str, path_str, status, elapsed_ms);
                        }
                    } else if status >= 400 {
                        if tracing::event_enabled!(tracing::Level::WARN) {
                            let method_str = unsafe { std::str::from_utf8_unchecked(&buf[method_off as usize..(method_off + method_len as u16) as usize]) };
                            let path_str = unsafe { std::str::from_utf8_unchecked(&buf[path_off as usize..(path_off + path_len) as usize]) };
                            warn!("{} {} - {} in {:.2}ms", method_str, path_str, status, elapsed_ms);
                        }
                    }

                    // Return thread and context to pools
                    thread_pool.release(thread);
                    request_pool.release(ctx_idx);

                    Ok(CoroutineResponse::Ready { status, body, content_type })
                }
            }
        }
        Err(e) => {
            request_pool.release(ctx_idx);
            convert_error_to_response(e, &buf, method_off, method_len, path_off, path_len, started_at)
        }
    }
}

fn convert_error_to_response(
    e: mlua::Error,
    buf: &Bytes,
    method_off: u16,
    method_len: u8,
    path_off: u16,
    path_len: u16,
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
            let method_str = unsafe { std::str::from_utf8_unchecked(&buf[method_off as usize..(method_off + method_len as u16) as usize]) };
            let path_str = unsafe { std::str::from_utf8_unchecked(&buf[path_off as usize..(path_off + path_len) as usize]) };
            warn!("{} {} - {} in {:.2}ms", method_str, path_str, status, elapsed_ms);
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

fn convert_lua_response(_lua: &Lua, result: Value) -> (u16, Bytes, Option<&'static str>) {
    match result {
        Value::UserData(ref ud) => {
            if let Ok(response) = ud.borrow::<RoverResponse>() {
                (
                    response.status,
                    response.body.clone(),
                    Some(response.content_type),
                )
            } else {
                (
                    500,
                    Bytes::from("Invalid userdata type"),
                    Some("text/plain"),
                )
            }
        }

        Value::String(ref s) => (
            200,
            Bytes::from(s.to_str().unwrap().to_string()),
            Some("text/plain"),
        ),

        Value::Table(table) => {
            let json = lua_table_to_json(&table).unwrap_or_else(|e| {
                format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
            });
            (200, Bytes::from(json), Some("application/json"))
        }

        Value::Integer(i) => (200, Bytes::from(i.to_string()), Some("text/plain")),
        Value::Number(n) => (200, Bytes::from(n.to_string()), Some("text/plain")),
        Value::Boolean(b) => (200, Bytes::from(b.to_string()), Some("text/plain")),
        Value::Nil => (204, Bytes::new(), None),

        Value::Error(e) => (
            500,
            Bytes::from(e.to_string()),
            Some("text/plain"),
        ),

        _ => (
            500,
            Bytes::from("Unsupported return type"),
            Some("text/plain"),
        ),
    }
}

fn lua_table_to_json(table: &Table) -> Result<String> {
    table
        .to_json_string()
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))
}
