use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use hyper::{body::Bytes, StatusCode};
use mlua::{Function, Lua, Table, Value};
use smallvec::SmallVec;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

use crate::{to_json::ToJson, response::RoverResponse};

/// Shared request data - wrapped in Arc for zero-cost sharing across closures
struct RequestData {
    headers: SmallVec<[(Bytes, Bytes); 8]>,
    query: SmallVec<[(Bytes, Bytes); 8]>,
    params: HashMap<String, String>,
    body: Option<Bytes>,
}

/// HTTP-specific task that can be executed by the event loop
pub struct HttpTask {
    pub method: Bytes,
    pub path: Bytes,
    pub headers: SmallVec<[(Bytes, Bytes); 8]>,
    pub query: SmallVec<[(Bytes, Bytes); 8]>,
    pub params: HashMap<String, String>,
    pub body: Option<Bytes>,
    pub handler: Function,
    pub respond_to: oneshot::Sender<HttpResponse>,
    pub started_at: Instant,
}

/// HTTP response that will be sent back to the server
pub struct HttpResponse {
    pub status: StatusCode,
    pub body: Bytes,
    pub content_type: Option<String>,
}

impl HttpTask {
    /// Execute the HTTP task by calling the Lua handler
    pub async fn execute(self, lua: &Lua) -> Result<()> {
        let method_str = unsafe { std::str::from_utf8_unchecked(&self.method) };
        let path_str = unsafe { std::str::from_utf8_unchecked(&self.path) };

        if tracing::event_enabled!(tracing::Level::DEBUG) {
            if !self.query.is_empty() {
                debug!("  ├─ query: {:?}", self.query);
            }
            if let Some(ref body) = self.body {
                let body_display = std::str::from_utf8(body).unwrap_or("<binary data>");
                debug!("  └─ body: {}", body_display);
            }
        }

        let ctx = match build_lua_context(lua, &self) {
            Ok(c) => c,
            Err(e) => {
                let error_msg = e.to_string();
                let status = if error_msg.contains("Invalid UTF-8") {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                let _ = self.respond_to.send(HttpResponse {
                    status,
                    body: Bytes::from(error_msg),
                    content_type: None,
                });
                return Ok(());
            }
        };

        let result: Value = match self.handler.call_async(ctx).await {
            Ok(r) => r,
            Err(e) => {
                // Try to extract ValidationErrors from error (handles both direct ExternalError and CallbackError wrapping)
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
                    // Direct struct→JSON conversion (zero string parsing)
                    (StatusCode::BAD_REQUEST, Bytes::from(validation_errors.to_json_string()))
                } else {
                    // Generic Lua error
                    let mut error_str = e.to_string();
                    if let Some(stack_pos) = error_str.find("\nstack traceback:") {
                        error_str = error_str[..stack_pos].to_string();
                    }
                    error_str = error_str.trim_start_matches("runtime error: ").to_string();
                    (StatusCode::INTERNAL_SERVER_ERROR, Bytes::from(format!("{{\"error\": \"{}\"}}", error_str.replace("\"", "\\\"").replace("\n", "\\n"))))
                };
                
                let _ = self.respond_to.send(HttpResponse {
                    status,
                    body,
                    content_type: None,
                });
                return Ok(());
            }
        };

        let (status, body, content_type) = convert_lua_response(lua, result);

        if tracing::event_enabled!(tracing::Level::DEBUG) {
            let body_preview = if body.len() > 200 {
                format!(
                    "{}... ({} bytes)",
                    String::from_utf8_lossy(&body[..200]),
                    body.len()
                )
            } else {
                String::from_utf8_lossy(&body).to_string()
            };
            debug!("  └─ response body: {}", body_preview);
        }

        let elapsed = self.started_at.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

        if status.is_success() {
            if tracing::event_enabled!(tracing::Level::INFO) {
                info!(
                    "{} {} - {} in {:.2}ms",
                    method_str,
                    path_str,
                    status.as_u16(),
                    elapsed_ms
                );
            }
        } else if status.is_client_error() || status.is_server_error() {
            if tracing::event_enabled!(tracing::Level::WARN) {
                warn!(
                    "{} {} - {} in {:.2}ms",
                    method_str,
                    path_str,
                    status.as_u16(),
                    elapsed_ms
                );
            }
        }

        let _ = self.respond_to.send(HttpResponse { status, body, content_type });
        Ok(())
    }
}

fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    let ctx = lua.create_table()?;

    let method_str = std::str::from_utf8(&task.method)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in HTTP method".to_string()))?;
    ctx.set("method", method_str)?;

    let path_str = std::str::from_utf8(&task.path)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in request path".to_string()))?;
    ctx.set("path", path_str)?;

    // Arc-based shared ownership - single clone (ref-count bump) instead of 4x data clones
    let req_data = Arc::new(RequestData {
        headers: task.headers.clone(),
        query: task.query.clone(),
        params: task.params.clone(),
        body: task.body.clone(),
    });

    let req_data_headers = req_data.clone();
    let headers_fn = lua.create_function(move |lua, ()| {
        if req_data_headers.headers.is_empty() {
            return lua.create_table();
        }
        let headers = lua.create_table_with_capacity(0, req_data_headers.headers.len())?;
        for (k, v) in &req_data_headers.headers {
            let k_str = std::str::from_utf8(k).map_err(|_| {
                mlua::Error::RuntimeError("Invalid UTF-8 in header name".to_string())
            })?;
            let v_str = std::str::from_utf8(v).map_err(|_| {
                mlua::Error::RuntimeError(format!("Invalid UTF-8 in header value for '{}'", k_str))
            })?;
            headers.set(k_str, v_str)?;
        }
        Ok(headers)
    })?;
    ctx.set("headers", headers_fn)?;

    let req_data_query = req_data.clone();
    let query_fn = lua.create_function(move |lua, ()| {
        if req_data_query.query.is_empty() {
            return lua.create_table();
        }
        let query = lua.create_table_with_capacity(0, req_data_query.query.len())?;
        for (k, v) in &req_data_query.query {
            let k_str = std::str::from_utf8(k).map_err(|_| {
                mlua::Error::RuntimeError("Invalid UTF-8 in query parameter name".to_string())
            })?;
            let v_str = std::str::from_utf8(v).map_err(|_| {
                mlua::Error::RuntimeError(format!("Invalid UTF-8 in query parameter '{}'", k_str))
            })?;
            query.set(k_str, v_str)?;
        }
        Ok(query)
    })?;
    ctx.set("query", query_fn)?;

    let req_data_params = req_data.clone();
    let params_fn = lua.create_function(move |lua, ()| {
        if req_data_params.params.is_empty() {
            return lua.create_table();
        }
        let params_table = lua.create_table_with_capacity(0, req_data_params.params.len())?;
        for (k, v) in &req_data_params.params {
            params_table.set(k.as_str(), v.as_str())?;
        }
        Ok(params_table)
    })?;
    ctx.set("params", params_fn)?;

    let req_data_body = req_data.clone();
    let body_fn = lua.create_function(move |lua, ()| {
        if let Some(body) = &req_data_body.body {
            let body_str = std::str::from_utf8(body).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Request body contains invalid UTF-8 (binary data not supported)".to_string(),
                )
            })?;

            let globals = lua.globals();
            let rover: Table = globals.get("rover")?;
            let guard: Table = rover.get("guard")?;

            if let Ok(constructor) = guard.get::<mlua::Function>("__body_value") {
                constructor.call((body_str.to_string(), body.to_vec()))
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

fn convert_lua_response(_lua: &Lua, result: Value) -> (StatusCode, Bytes, Option<String>) {
    match result {
        Value::UserData(ref ud) => {
            if let Ok(response) = ud.borrow::<RoverResponse>() {
                (
                    StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK),
                    response.body.clone(),
                    Some(response.content_type.clone()),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Bytes::from("Invalid userdata type"),
                    Some("text/plain".to_string()),
                )
            }
        }

        Value::String(ref s) => (
            StatusCode::OK,
            Bytes::from(s.to_str().unwrap().to_string()),
            Some("text/plain".to_string()),
        ),

        Value::Table(table) => {
            let json = lua_table_to_json(&table).unwrap_or_else(|e| {
                format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
            });
            (StatusCode::OK, Bytes::from(json), Some("application/json".to_string()))
        }

        Value::Integer(i) => (StatusCode::OK, Bytes::from(i.to_string()), Some("text/plain".to_string())),
        Value::Number(n) => (StatusCode::OK, Bytes::from(n.to_string()), Some("text/plain".to_string())),
        Value::Boolean(b) => (StatusCode::OK, Bytes::from(b.to_string()), Some("text/plain".to_string())),
        Value::Nil => (StatusCode::NO_CONTENT, Bytes::new(), None),

        Value::Error(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Bytes::from(e.to_string()),
            Some("text/plain".to_string()),
        ),

        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
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
