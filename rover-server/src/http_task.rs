use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use mlua::{
    Function, Lua, RegistryKey, Table, Thread, ThreadStatus, UserData, UserDataFields,
    UserDataMethods, Value,
};

use tracing::{debug, info, warn};

use crate::buffer_pool::BufferPool;
use crate::table_pool::LuaTablePool;
use crate::{Bytes, MiddlewareChain, SseResponse, response::RoverResponse, to_json::ToJson};
use rover_ui::SharedSignalRuntime;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for request ID uniqueness
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique request ID (base62 encoded for compactness)
pub fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0) as u64;
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Combine timestamp (high bits) with counter (low bits) for uniqueness
    let combined = (timestamp << 20) | (counter & 0xFFFFF);
    base62_encode(combined)
}

/// Encode a u64 as base62 for compact request IDs
fn base62_encode(mut n: u64) -> String {
    const CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::with_capacity(11);
    if n == 0 {
        return "0".to_string();
    }
    while n > 0 {
        result.push(CHARSET[(n % 62) as usize]);
        n /= 62;
    }
    result.reverse();
    String::from_utf8(result).unwrap_or_else(|_| "0".to_string())
}

/// Extract request ID from headers or generate a new one
pub fn extract_or_generate_request_id(
    buf: &[u8],
    headers: &[(usize, usize, usize, usize)],
) -> String {
    for &(name_off, name_len, val_off, val_len) in headers {
        let name_bytes = &buf[name_off..name_off + name_len];
        let name = unsafe { std::str::from_utf8_unchecked(name_bytes) };
        if name.eq_ignore_ascii_case("x-request-id") {
            let val_bytes = &buf[val_off..val_off + val_len];
            let value = unsafe { std::str::from_utf8_unchecked(val_bytes) };
            // Accept non-empty IDs up to 256 chars
            if !value.is_empty() && value.len() <= 256 {
                return value.to_string();
            }
            break;
        }
    }
    generate_request_id()
}

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

    /// Request ID for correlation across request lifecycle
    request_id: String,

    client_ip: String,
    client_proto: String,

    /// Context storage for request-scoped data (used by middleware)
    context_data: std::cell::RefCell<std::collections::HashMap<String, Value>>,

    /// Middleware chain to execute (if any)
    middleware_chain: std::cell::RefCell<Option<MiddlewareChain>>,

    /// Current position in middleware chain execution
    chain_position: std::cell::Cell<usize>,
}

impl RequestContext {
    fn header_value(&self, name: &str) -> Option<String> {
        for &(name_off, name_len, val_off, val_len) in &self.headers {
            let name_bytes = &self.buf[name_off as usize..(name_off + name_len as u16) as usize];
            let val_bytes = &self.buf[val_off as usize..(val_off + val_len) as usize];
            let key = unsafe { std::str::from_utf8_unchecked(name_bytes) };
            if key.eq_ignore_ascii_case(name) {
                let value = unsafe { std::str::from_utf8_unchecked(val_bytes) };
                return Some(value.to_string());
            }
        }
        None
    }

    fn request_id(&self) -> &str {
        &self.request_id
    }
}

impl UserData for RequestContext {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("method", |_lua, this| {
            let method = unsafe {
                std::str::from_utf8_unchecked(
                    &this.buf[this.method_off as usize
                        ..(this.method_off + this.method_len as u16) as usize],
                )
            };
            Ok(method.to_string())
        });
        fields.add_field_method_get("path", |_lua, this| {
            let path = unsafe {
                std::str::from_utf8_unchecked(
                    &this.buf[this.path_off as usize..(this.path_off + this.path_len) as usize],
                )
            };
            Ok(path.to_string())
        });
        fields.add_field_method_get("client_ip", |_lua, this| Ok(this.client_ip.clone()));
        fields.add_field_method_get("client_proto", |_lua, this| Ok(this.client_proto.clone()));
        fields.add_field_method_get("ip", |_lua, this| Ok(this.client_ip.clone()));
        fields.add_field_method_get("proto", |_lua, this| Ok(this.client_proto.clone()));
    }

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
                let content_type = this.header_value("content-type");
                let body_value = BodyValue::new(body_bytes, content_type);
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

        // ctx:request_id() - Get the unique request ID for correlation
        methods.add_method("request_id", |lua, this, ()| {
            Ok(Value::String(lua.create_string(&this.request_id)?))
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
    content_type: Option<String>,
}

impl BodyValue {
    pub fn new(bytes: Bytes, content_type: Option<String>) -> Self {
        Self {
            bytes,
            content_type,
        }
    }

    fn ensure_json_media_type(&self) -> mlua::Result<()> {
        if let Some(ct) = &self.content_type {
            let media_type = ct
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let is_json = media_type == "application/json" || media_type.ends_with("+json");
            if !is_json {
                return Err(mlua::Error::RuntimeError(format!(
                    "Unsupported Media Type (415): expected application/json, got {}",
                    media_type
                )));
            }
        }
        Ok(())
    }
}

impl BodyValue {
    fn ensure_multipart_media_type(&self) -> mlua::Result<()> {
        if let Some(ct) = &self.content_type {
            let media_type = ct
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if !media_type.starts_with("multipart/form-data") {
                return Err(mlua::Error::RuntimeError(format!(
                    "Unsupported Media Type (415): expected multipart/form-data, got {}",
                    media_type
                )));
            }
        } else {
            return Err(mlua::Error::RuntimeError(
                "Missing Content-Type header for multipart parsing".to_string(),
            ));
        }
        Ok(())
    }

    fn parse_multipart(&self) -> mlua::Result<crate::multipart::MultipartData> {
        self.ensure_multipart_media_type()?;
        let content_type = self.content_type.as_ref().unwrap();
        let limits = crate::multipart::MultipartLimits::default();

        match crate::multipart::parse_multipart_data(&self.bytes, content_type, &limits) {
            Ok(data) => Ok(data),
            Err(e) => Err(mlua::Error::RuntimeError(format!(
                "Failed to parse multipart data: {}",
                e
            ))),
        }
    }
}

impl UserData for BodyValue {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("json", |lua, this, ()| {
            this.ensure_json_media_type()?;
            parse_json_with_expect(lua, &this.bytes)
        });

        methods.add_method("raw", |lua, this, ()| {
            this.ensure_json_media_type()?;
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
            this.ensure_json_media_type()?;
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

        // Multipart form-data methods
        methods.add_method("file", |lua, this, field_name: String| {
            let data = this.parse_multipart()?;

            if let Some(file) = data.get_file(&field_name) {
                let file_table = lua.create_table()?;
                file_table.set("name", file.filename.clone())?;
                file_table.set("size", file.size)?;
                if let Some(ref ct) = file.content_type {
                    file_table.set("type", ct.clone())?;
                }
                file_table.set("data", lua.create_string(&file.data)?)?;
                Ok(Value::Table(file_table))
            } else {
                Ok(Value::Nil)
            }
        });

        methods.add_method("files", |lua, this, field_name: String| {
            let data = this.parse_multipart()?;

            let files_table = lua.create_table()?;
            if let Some(files) = data.get_files(&field_name) {
                for (i, file) in files.iter().enumerate() {
                    let file_table = lua.create_table()?;
                    file_table.set("name", file.filename.clone())?;
                    file_table.set("size", file.size)?;
                    if let Some(ref ct) = file.content_type {
                        file_table.set("type", ct.clone())?;
                    }
                    file_table.set("data", lua.create_string(&file.data)?)?;
                    files_table.set(i + 1, file_table)?;
                }
            }
            Ok(Value::Table(files_table))
        });

        methods.add_method("form", |lua, this, ()| {
            let data = this.parse_multipart()?;

            let form_table = lua.create_table()?;
            for (key, value) in &data.fields {
                form_table.set(key.clone(), value.clone())?;
            }
            Ok(Value::Table(form_table))
        });

        methods.add_method("multipart", |lua, this, ()| {
            let data = this.parse_multipart()?;

            let result_table = lua.create_table()?;

            // Add fields
            let fields_table = lua.create_table()?;
            for (key, value) in &data.fields {
                fields_table.set(key.clone(), value.clone())?;
            }
            result_table.set("fields", fields_table)?;

            // Add files
            let files_table = lua.create_table()?;
            for (field_name, uploads) in &data.files {
                let field_files = lua.create_table()?;
                for (i, file) in uploads.iter().enumerate() {
                    let file_table = lua.create_table()?;
                    file_table.set("name", file.filename.clone())?;
                    file_table.set("size", file.size)?;
                    if let Some(ref ct) = file.content_type {
                        file_table.set("type", ct.clone())?;
                    }
                    file_table.set("data", lua.create_string(&file.data)?)?;
                    field_files.set(i + 1, file_table)?;
                }
                files_table.set(field_name.clone(), field_files)?;
            }
            result_table.set("files", files_table)?;

            Ok(Value::Table(result_table))
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
                request_id: String::new(),
                client_ip: String::new(),
                client_proto: String::new(),
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
        request_id: String,
        client_ip: String,
        client_proto: String,
    ) -> mlua::Result<(Value, usize)> {
        let idx = self
            .available
            .pop()
            .ok_or_else(|| mlua::Error::RuntimeError("RequestContextPool exhausted".to_string()))?;

        let key = &self.pool[idx];
        let userdata: mlua::AnyUserData = lua.registry_value(key)?;

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
        ctx.request_id = request_id;
        ctx.client_ip = client_ip;
        ctx.client_proto = client_proto;
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
        headers: Option<std::collections::HashMap<String, String>>,
    },
    Yielded {
        thread: Thread,
        ctx_idx: usize,
    },
    Streaming {
        status: u16,
        content_type: String,
        headers: Option<std::collections::HashMap<String, String>>,
        chunk_producer: Arc<RegistryKey>,
    },
    Sse {
        status: u16,
        headers: Option<std::collections::HashMap<String, String>>,
        event_producer: Arc<RegistryKey>,
        retry_ms: Option<u32>,
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
    request_id: String,
    client_ip: String,
    client_proto: String,
    started_at: Instant,
    thread_pool: &mut ThreadPool,
    request_pool: &mut RequestContextPool,
    _table_pool: &LuaTablePool,
    buffer_pool: &mut BufferPool,
    error_handler: Option<&Arc<RegistryKey>>,
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

    let request_id_for_log = request_id.clone();
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
        request_id,
        client_ip,
        client_proto,
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
                headers: None,
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
                    // Fast path: check for RoverResponse or StreamingResponse
                    if let Value::UserData(ref ud) = result {
                        if let Ok(response) = ud.borrow::<RoverResponse>() {
                            let status = response.status;
                            let body = response.body.clone();
                            let content_type = Some(response.content_type);
                            let headers = response.headers.clone();

                            let elapsed = started_at.elapsed();
                            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

                            if (200..300).contains(&status) {
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
                                        "[{}] {} {} - {} in {:.2}ms",
                                        request_id_for_log,
                                        method_str,
                                        path_str,
                                        status,
                                        elapsed_ms
                                    );
                                }
                            } else if status >= 400 && tracing::event_enabled!(tracing::Level::WARN)
                            {
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
                                    "[{}] {} {} - {} in {:.2}ms",
                                    request_id_for_log, method_str, path_str, status, elapsed_ms
                                );
                            }

                            thread_pool.release(thread);
                            request_pool.release(ctx_idx);

                            return Ok(CoroutineResponse::Ready {
                                status,
                                body,
                                content_type,
                                headers,
                            });
                        }

                        // Check for StreamingResponse
                        if let Ok(streaming) = ud.borrow::<crate::StreamingResponse>() {
                            let status = streaming.status;
                            let content_type = streaming.content_type.clone();
                            let headers = streaming.headers.clone();
                            let chunk_producer = Arc::clone(&streaming.chunk_producer);

                            thread_pool.release(thread);
                            request_pool.release(ctx_idx);

                            return Ok(CoroutineResponse::Streaming {
                                status,
                                content_type,
                                headers,
                                chunk_producer,
                            });
                        }

                        if let Ok(sse) = ud.borrow::<SseResponse>() {
                            let status = sse.status;
                            let headers = sse.headers.clone();
                            let event_producer = Arc::clone(&sse.event_producer);
                            let retry_ms = sse.retry_ms;

                            thread_pool.release(thread);
                            request_pool.release(ctx_idx);

                            return Ok(CoroutineResponse::Sse {
                                status,
                                headers,
                                event_producer,
                                retry_ms,
                            });
                        }
                    }

                    // Convert other Lua values to response
                    let (status, body, content_type) =
                        convert_lua_response(lua, result, buffer_pool);

                    let elapsed = started_at.elapsed();
                    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

                    if (200..300).contains(&status) {
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
                                "[{}] {} {} - {} in {:.2}ms",
                                request_id_for_log, method_str, path_str, status, elapsed_ms
                            );
                        }
                    } else if status >= 400 && tracing::event_enabled!(tracing::Level::WARN) {
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
                            "[{}] {} {} - {} in {:.2}ms",
                            request_id_for_log, method_str, path_str, status, elapsed_ms
                        );
                    }

                    thread_pool.release(thread);
                    request_pool.release(ctx_idx);

                    Ok(CoroutineResponse::Ready {
                        status,
                        body,
                        content_type,
                        headers: None,
                    })
                }
            }
        }
        Err(e) => {
            request_pool.release(ctx_idx);
            convert_error_to_response(
                lua,
                e,
                &buf,
                method_off,
                method_len,
                path_off,
                path_len,
                started_at,
                buffer_pool,
                error_handler,
                &request_id_for_log,
            )
        }
    }
}

fn convert_error_to_response(
    lua: &Lua,
    e: mlua::Error,
    buf: &Bytes,
    method_off: u16,
    method_len: u8,
    path_off: u16,
    path_len: u16,
    started_at: Instant,
    _buffer_pool: &mut BufferPool,
    error_handler: Option<&Arc<RegistryKey>>,
    request_id: &str,
) -> Result<CoroutineResponse> {
    // Try to use custom error handler if available
    if let Some(handler_key) = error_handler {
        let path_str = unsafe {
            std::str::from_utf8_unchecked(&buf[path_off as usize..(path_off + path_len) as usize])
        };

        // Extract error message and try to parse as error table
        let mut error_message = e.to_string();
        if let Some(stack_pos) = error_message.find("\nstack traceback:") {
            error_message = error_message[..stack_pos].to_string();
        }
        error_message = error_message
            .trim_start_matches("runtime error: ")
            .to_string();

        // Create error table for the handler
        let error_table = lua.create_table()?;
        error_table.set("message", error_message.clone())?;
        error_table.set("path", path_str)?;

        // Try to parse status and code from the error if it was thrown as a table
        // Default to 500 for server errors, 400 for validation
        let default_status = if error_message.contains("not found") {
            404
        } else if error_message.contains("validation") || error_message.contains("required") {
            400
        } else {
            500
        };
        error_table.set("status", default_status)?;
        error_table.set("code", "ERROR")?;

        // Call the error handler
        let handler: Function = lua.registry_value(handler_key)?;
        match handler.call::<Value>(error_table) {
            Ok(result) => {
                // Handler returned a response - extract it
                if let Value::UserData(ud) = result
                    && let Ok(response) = ud.borrow::<RoverResponse>()
                {
                    return Ok(CoroutineResponse::Ready {
                        status: response.status,
                        body: response.body.clone(),
                        content_type: Some(response.content_type),
                        headers: response.headers.clone(),
                    });
                }
                // Handler returned something else - fall through to default
            }
            Err(_) => {
                // Handler failed - fall through to default error handling
            }
        }
    }

    // Default error handling
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
        let status = if error_str.contains("Unsupported Media Type (415)") {
            415
        } else {
            500
        };
        (
            status,
            Bytes::from(format!(
                "{{\"error\": \"{}\"}}",
                error_str.replace("\"", "\\\"").replace("\n", "\\n")
            )),
        )
    };

    let elapsed = started_at.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    if status >= 400 && tracing::event_enabled!(tracing::Level::WARN) {
        let method_str = unsafe {
            std::str::from_utf8_unchecked(
                &buf[method_off as usize..(method_off + method_len as u16) as usize],
            )
        };
        let path_str = unsafe {
            std::str::from_utf8_unchecked(&buf[path_off as usize..(path_off + path_len) as usize])
        };
        warn!(
            "[{}] {} {} - {} in {:.2}ms",
            request_id, method_str, path_str, status, elapsed_ms
        );
    }

    Ok(CoroutineResponse::Ready {
        status,
        body,
        content_type: None,
        headers: None,
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

#[cfg(test)]
mod tests {
    use super::{
        BodyValue, RequestContextPool, extract_or_generate_request_id, generate_request_id,
    };
    use bytes::Bytes;
    use mlua::{Lua, MultiValue, Value};

    #[test]
    fn should_allow_application_json_media_type() {
        let body = BodyValue::new(
            Bytes::from_static(br#"{"ok":true}"#),
            Some("application/json".to_string()),
        );
        assert!(body.ensure_json_media_type().is_ok());
    }

    #[test]
    fn should_allow_json_suffix_media_type() {
        let body = BodyValue::new(
            Bytes::from_static(br#"{"ok":true}"#),
            Some("application/merge-patch+json".to_string()),
        );
        assert!(body.ensure_json_media_type().is_ok());
    }

    #[test]
    fn should_reject_non_json_media_type() {
        let body = BodyValue::new(Bytes::from_static(b"hello"), Some("text/plain".to_string()));
        assert!(body.ensure_json_media_type().is_err());
    }

    #[test]
    fn should_generate_unique_request_ids() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert_ne!(id1, id2, "Request IDs should be unique");
    }

    #[test]
    fn should_generate_base62_request_id() {
        let id = generate_request_id();
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric()),
            "Request ID should be base62 encoded: {}",
            id
        );
    }

    #[test]
    fn should_extract_request_id_from_header() {
        // Simpler test buffer with just X-Request-ID header
        let buf = b"X-Request-ID: my-request-123\r\n";
        // Header name "X-Request-ID" at offset 0, length 12
        // ": " is at offset 12-13
        // Header value "my-request-123" at offset 14, length 14
        let headers: Vec<(usize, usize, usize, usize)> = vec![(0, 12, 14, 14)];
        let id = extract_or_generate_request_id(buf, &headers);
        assert_eq!(id, "my-request-123");
    }

    #[test]
    fn should_extract_request_id_case_insensitive() {
        let buf = b"x-request-id: lower-case-id\r\n";
        // Header name "x-request-id" at offset 0, length 12
        // ": " is at offset 12-13
        // Header value "lower-case-id" at offset 14, length 13
        let headers: Vec<(usize, usize, usize, usize)> = vec![(0, 12, 14, 13)];
        let id = extract_or_generate_request_id(buf, &headers);
        assert_eq!(id, "lower-case-id");
    }

    #[test]
    fn should_generate_id_when_no_header_present() {
        let buf = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> = vec![];
        let id = extract_or_generate_request_id(buf, &headers);
        assert!(
            !id.is_empty(),
            "Should generate a request ID when no header is present"
        );
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric()),
            "Generated ID should be base62 encoded: {}",
            id
        );
    }

    #[test]
    fn should_generate_id_for_empty_header_value() {
        let buf = b"GET / HTTP/1.1\r\nX-Request-ID: \r\nHost: example.com\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> = vec![
            (16, 12, 30, 0), // Empty value
        ];
        let id = extract_or_generate_request_id(buf, &headers);
        assert!(
            !id.is_empty(),
            "Should generate a request ID when header value is empty"
        );
    }

    #[test]
    fn should_expose_request_and_network_fields_on_request_context() {
        let lua = Lua::new();
        let mut pool = RequestContextPool::new(&lua, 1).expect("pool");
        let buf = Bytes::from_static(b"GET /hello HTTP/1.1\r\nHost: example.com\r\n\r\n");

        let (ctx, _idx) = pool
            .acquire(
                &lua,
                buf,
                0,
                3,
                4,
                6,
                0,
                0,
                Vec::new(),
                Vec::new(),
                &[],
                "req-1".to_string(),
                "203.0.113.7".to_string(),
                "https".to_string(),
            )
            .expect("acquire");

        lua.globals().set("ctx", ctx).expect("set ctx");
        let values: MultiValue = lua
            .load("return ctx.method, ctx.path, ctx.client_ip, ctx.client_proto, ctx.ip, ctx.proto")
            .eval()
            .expect("eval");

        let collected = values
            .into_iter()
            .map(|value| match value {
                Value::String(s) => s.to_str().unwrap().to_string(),
                other => panic!("expected string, got {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            collected,
            vec![
                "GET",
                "/hello",
                "203.0.113.7",
                "https",
                "203.0.113.7",
                "https",
            ]
        );
    }
}
