use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use mlua::{Function, Lua, Table, Value};
use smallvec::SmallVec;
use tracing::{debug, info, warn};

use crate::{to_json::ToJson, response::RoverResponse, Bytes};

struct RequestData {
    headers: SmallVec<[(Bytes, Bytes); 8]>,
    query: SmallVec<[(Bytes, Bytes); 8]>,
    params: HashMap<String, String>,
    body: Option<Bytes>,
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Bytes,
    pub content_type: Option<String>,
}

pub fn execute_handler(
    lua: &Lua,
    handler: &Function,
    method: Bytes,
    path: Bytes,
    headers: SmallVec<[(Bytes, Bytes); 8]>,
    query: SmallVec<[(Bytes, Bytes); 8]>,
    params: HashMap<String, String>,
    body: Option<Bytes>,
    started_at: Instant,
) -> Result<HttpResponse> {
    let method_str = unsafe { std::str::from_utf8_unchecked(&method) };
    let path_str = unsafe { std::str::from_utf8_unchecked(&path) };

    if tracing::event_enabled!(tracing::Level::DEBUG) {
        if !query.is_empty() {
            debug!("  ├─ query: {:?}", query);
        }
        if let Some(ref body) = body {
            let body_display = std::str::from_utf8(body).unwrap_or("<binary data>");
            debug!("  └─ body: {}", body_display);
        }
    }

    let ctx = match build_lua_context(lua, &method, &path, &headers, &query, &params, &body) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = e.to_string();
            let status = if error_msg.contains("Invalid UTF-8") {
                400
            } else {
                500
            };
            return Ok(HttpResponse {
                status,
                body: Bytes::from(error_msg),
                content_type: None,
            });
        }
    };

    let result: Value = match handler.call(ctx) {
        Ok(r) => r,
        Err(e) => {
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
            
            return Ok(HttpResponse {
                status,
                body,
                content_type: None,
            });
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

    let elapsed = started_at.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    if status >= 200 && status < 300 {
        if tracing::event_enabled!(tracing::Level::INFO) {
            info!("{} {} - {} in {:.2}ms", method_str, path_str, status, elapsed_ms);
        }
    } else if status >= 400 {
        if tracing::event_enabled!(tracing::Level::WARN) {
            warn!("{} {} - {} in {:.2}ms", method_str, path_str, status, elapsed_ms);
        }
    }

    Ok(HttpResponse { status, body, content_type })
}

fn build_lua_context(
    lua: &Lua,
    method: &Bytes,
    path: &Bytes,
    headers: &SmallVec<[(Bytes, Bytes); 8]>,
    query: &SmallVec<[(Bytes, Bytes); 8]>,
    params: &HashMap<String, String>,
    body: &Option<Bytes>,
) -> Result<Table> {
    let ctx = lua.create_table()?;

    let method_str = std::str::from_utf8(method)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in HTTP method".to_string()))?;
    ctx.set("method", method_str)?;

    let path_str = std::str::from_utf8(path)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in request path".to_string()))?;
    ctx.set("path", path_str)?;

    let req_data = Arc::new(RequestData {
        headers: headers.clone(),
        query: query.clone(),
        params: params.clone(),
        body: body.clone(),
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
