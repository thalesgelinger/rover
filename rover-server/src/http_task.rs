use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use mlua::{
    Function, Lua, RegistryKey, Table, Thread, ThreadStatus, UserData, UserDataMethods, Value,
};

use tracing::{debug, info, warn};

use crate::buffer_pool::BufferPool;
use crate::table_pool::LuaTablePool;
use crate::{Bytes, MiddlewareChain, response::RoverResponse, to_json::ToJson};
use rover_ui::SharedSignalRuntime;

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

    /// Context storage for request-scoped data (used by middleware)
    context_data: std::cell::RefCell<std::collections::HashMap<String, Value>>,

    /// Middleware chain to execute (if any)
    middleware_chain: std::cell::RefCell<Option<MiddlewareChain>>,

    /// Current position in middleware chain execution
    chain_position: std::cell::Cell<usize>,
}

impl UserData for RequestContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("headers", |lua, this, ()| {
            if this.headers.is_empty() {
                return lua.create_table();
            }
            let headers_table = lua.create_table_with_capacity(0, this.headers.len())?;
            for &(name_off, name_len, val_off, val_len) in &this.headers {
                let name_bytes =
                    &this.buf[name_off as usize..(name_off + name_len as u16) as usize];
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
                let name_bytes =
                    &this.buf[name_off as usize..(name_off + name_len as u16) as usize];
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
                let body_bytes = this
                    .buf
                    .slice(this.body_off as usize..(this.body_off + this.body_len) as usize);
                let body_value = BodyValue::new(body_bytes);
                lua.create_userdata(body_value).map(Value::UserData)
            } else {
                Err(mlua::Error::RuntimeError("Request has no body".to_string()))
            }
        });

        // ctx:set(key, value) - Store request-scoped data
        methods.add_method_mut("set", |_lua, this, (key, value): (String, Value)| {
            this.context_data.borrow_mut().insert(key, value);
            Ok(())
        });

        // ctx:get(key) - Retrieve request-scoped data
        methods.add_method("get", |_lua, this, key: String| {
            let data = this.context_data.borrow();
            match data.get(&key) {
                Some(value) => Ok(value.clone()),
                None => Ok(Value::Nil),
            }
        });

        // ctx:next() - Continue to next middleware/handler
        // For now, this returns nil. The middleware chain is executed linearly
        // by the request handler, not through Lua callbacks.
        methods.add_method("next", |_lua, _this, ()| {
            // The actual chain execution is handled by the request handler
            // This method exists for API compatibility
            Ok(Value::Nil)
        });
    }
}

impl RequestContext {
    /// Set the middleware chain for this request
    pub fn set_middleware_chain(&self, chain: MiddlewareChain) {
        *self.middleware_chain.borrow_mut() = Some(chain);
        self.chain_position.set(0);
    }

    /// Get the next middleware handler to execute
    /// Returns None when the chain is complete
    pub fn get_next_middleware(&self) -> Option<(String, Arc<RegistryKey>)> {
        if let Some(ref chain) = *self.middleware_chain.borrow() {
            let position = self.chain_position.get();
            let total_before = chain.before.len();
            let total_after = chain.after.len();

            if position < total_before {
                // Execute before middleware
                let mw = &chain.before[position];
                self.chain_position.set(position + 1);
                Some((mw.name.clone(), Arc::clone(&mw.handler)))
            } else if position == total_before && total_after > 0 {
                // Execute after middleware (in reverse order)
                let after_idx = position - total_before;
                let rev_idx = total_after - 1 - after_idx;
                let mw = &chain.after[rev_idx];
                self.chain_position.set(position + 1);
                Some((mw.name.clone(), Arc::clone(&mw.handler)))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if there are more after middlewares to execute
    pub fn has_after_middlewares(&self) -> bool {
        if let Some(ref chain) = *self.middleware_chain.borrow() {
            let position = self.chain_position.get();
            let total_before = chain.before.len();
            let total_after = chain.after.len();
            position > total_before && position < total_before + total_after
        } else {
            false
        }
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
            parse_json_with_expect(lua, &this.bytes)
        });

        methods.add_method("raw", |lua, this, ()| {
            parse_json_with_expect(lua, &this.bytes)
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

        methods.add_method("expect", |lua, this, schema: Table| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let parsed =
                    crate::direct_json_parser::json_bytes_ref_to_lua_direct(lua, &this.bytes)?;
                validate_body_table(lua, &parsed, &schema)
            }));

            match result {
                Ok(inner_result) => inner_result,
                Err(panic_err) => {
                    eprintln!("PANIC in validation: {:?}", panic_err);
                    Err(mlua::Error::RuntimeError(
                        "Internal server error occurred during validation".to_string(),
                    ))
                }
            }
        });
    }
}

fn parse_json_with_expect(lua: &Lua, bytes: &Bytes) -> mlua::Result<Value> {
    let value = crate::direct_json_parser::json_bytes_ref_to_lua_direct(lua, bytes)?;
    if let Value::Table(table) = &value {
        attach_expect_metatable(lua, table.clone())?;
    }
    Ok(value)
}

fn attach_expect_metatable(lua: &Lua, table: Table) -> mlua::Result<()> {
    if table.metatable().is_some() {
        return Ok(());
    }

    let meta = lua.create_table()?;
    let methods = lua.create_table()?;
    let expect_fn = lua.create_function(|lua, (data, schema): (Table, Table)| {
        let body_value = Value::Table(data.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            validate_body_table(lua, &body_value, &schema)
        }));

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_err) => {
                eprintln!("PANIC in validation: {:?}", panic_err);
                Err(mlua::Error::RuntimeError(
                    "Internal server error occurred during validation".to_string(),
                ))
            }
        }
    })?;

    methods.set("expect", expect_fn)?;
    meta.set("__index", methods)?;
    table.set_metatable(Some(meta))?;
    Ok(())
}

fn validate_body_table(lua: &Lua, value: &Value, schema: &Table) -> mlua::Result<Value> {
    let body_object = match value {
        Value::Table(table) => table.clone(),
        _ => {
            return Err(mlua::Error::RuntimeError(
                "Request body must be a JSON object".to_string(),
            ));
        }
    };

    match rover_types::validate_table(lua, &body_object, schema, "") {
        Ok(validated) => Ok(validated),
        Err(errors) => {
            let validation_errors = rover_types::ValidationErrors::new(errors);
            Err(mlua::Error::ExternalError(Arc::new(validation_errors)))
        }
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
                context_data: std::cell::RefCell::new(std::collections::HashMap::new()),
                middleware_chain: std::cell::RefCell::new(None),
                chain_position: std::cell::Cell::new(0),
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
        let idx = self
            .available
            .pop()
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
        // Clear context data and chain state from previous request
        ctx.context_data.borrow_mut().clear();
        ctx.middleware_chain.borrow_mut().take();
        ctx.chain_position.set(0);
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

pub enum CoroutineResponse {
    Ready {
        status: u16,
        body: Bytes,
        content_type: Option<&'static str>,
    },
    Yielded {
        thread: Thread,
        ctx_idx: usize,
    },
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
    buffer_pool: &mut BufferPool,
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

    let (ctx, ctx_idx) = match request_pool.acquire(
        lua,
        buf.clone(),
        method_off,
        method_len,
        path_off,
        path_len,
        body_off,
        body_len,
        headers,
        query,
        params,
    ) {
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

    // Get signal runtime if available and begin batch
    let runtime_opt = lua.app_data_ref::<SharedSignalRuntime>();
    if let Some(ref runtime) = runtime_opt {
        runtime.begin_batch();
    }

    let resume_result = thread.resume::<Value>(ctx);

    // End batch and run pending effects
    if let Some(runtime) = runtime_opt {
        let _ = runtime.end_batch(lua);
    }

    match resume_result {
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
                            (
                                response.status,
                                response.body.clone(),
                                Some(response.content_type),
                            )
                        } else {
                            convert_lua_response(lua, result, buffer_pool)
                        }
                    } else {
                        convert_lua_response(lua, result, buffer_pool)
                    };

                    let elapsed = started_at.elapsed();
                    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

                    if status >= 200 && status < 300 {
                        if tracing::event_enabled!(tracing::Level::INFO) {
                            let method_str = unsafe {
                                std::str::from_utf8_unchecked(
                                    &buf[method_off as usize
                                        ..(method_off + method_len as u16) as usize],
                                )
                            };
                            let path_str = unsafe {
                                std::str::from_utf8_unchecked(
                                    &buf[path_off as usize..(path_off + path_len) as usize],
                                )
                            };
                            info!(
                                "{} {} - {} in {:.2}ms",
                                method_str, path_str, status, elapsed_ms
                            );
                        }
                    } else if status >= 400 {
                        if tracing::event_enabled!(tracing::Level::WARN) {
                            let method_str = unsafe {
                                std::str::from_utf8_unchecked(
                                    &buf[method_off as usize
                                        ..(method_off + method_len as u16) as usize],
                                )
                            };
                            let path_str = unsafe {
                                std::str::from_utf8_unchecked(
                                    &buf[path_off as usize..(path_off + path_len) as usize],
                                )
                            };
                            warn!(
                                "{} {} - {} in {:.2}ms",
                                method_str, path_str, status, elapsed_ms
                            );
                        }
                    }

                    // Return thread and context to pools
                    thread_pool.release(thread);
                    request_pool.release(ctx_idx);

                    Ok(CoroutineResponse::Ready {
                        status,
                        body,
                        content_type,
                    })
                }
            }
        }
        Err(e) => {
            request_pool.release(ctx_idx);
            convert_error_to_response(
                e,
                &buf,
                method_off,
                method_len,
                path_off,
                path_len,
                started_at,
                buffer_pool,
            )
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
    _buffer_pool: &mut BufferPool,
) -> Result<CoroutineResponse> {
    let validation_err = match &e {
        mlua::Error::ExternalError(arc_err) => {
            arc_err.downcast_ref::<rover_types::ValidationErrors>()
        }
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
        (
            500,
            Bytes::from(format!(
                "{{\"error\": \"{}\"}}",
                error_str.replace("\"", "\\\"").replace("\n", "\\n")
            )),
        )
    };

    let elapsed = started_at.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    if status >= 400 {
        if tracing::event_enabled!(tracing::Level::WARN) {
            let method_str = unsafe {
                std::str::from_utf8_unchecked(
                    &buf[method_off as usize..(method_off + method_len as u16) as usize],
                )
            };
            let path_str = unsafe {
                std::str::from_utf8_unchecked(
                    &buf[path_off as usize..(path_off + path_len) as usize],
                )
            };
            warn!(
                "{} {} - {} in {:.2}ms",
                method_str, path_str, status, elapsed_ms
            );
        }
    }

    Ok(CoroutineResponse::Ready {
        status,
        body,
        content_type: None,
    })
}

fn convert_lua_response(
    _lua: &Lua,
    result: Value,
    buffer_pool: &mut BufferPool,
) -> (u16, Bytes, Option<&'static str>) {
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
            let json = lua_table_to_json(&table, buffer_pool)
                .unwrap_or_else(|e| format!("{{\"error\":\"Failed to serialize: {}\"}}", e));
            (200, Bytes::from(json), Some("application/json"))
        }

        Value::Integer(i) => (200, Bytes::from(i.to_string()), Some("text/plain")),
        Value::Number(n) => (200, Bytes::from(n.to_string()), Some("text/plain")),
        Value::Boolean(b) => (200, Bytes::from(b.to_string()), Some("text/plain")),
        Value::Nil => (204, Bytes::new(), None),

        Value::Error(e) => (500, Bytes::from(e.to_string()), Some("text/plain")),

        _ => (
            500,
            Bytes::from("Unsupported return type"),
            Some("text/plain"),
        ),
    }
}

fn lua_table_to_json(table: &Table, buffer_pool: &mut BufferPool) -> Result<String> {
    let mut buf = buffer_pool.get_json_buf();
    table
        .to_json(&mut buf)
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))?;
    let json = String::from_utf8_lossy(&buf).to_string();
    buffer_pool.return_json_buf(buf);
    Ok(json)
}
