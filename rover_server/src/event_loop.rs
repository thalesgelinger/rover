use std::collections::HashMap;

use anyhow::Result;
use hyper::{body::Bytes, StatusCode};
use mlua::{Lua, Table, Value};
use tokio::sync::mpsc::Receiver;
use tracing::{debug, info, warn};

use crate::{HttpMethod, LuaRequest, LuaResponse, Route, ServerConfig, RoverResponse};
use crate::{fast_router::FastRouter, to_json::ToJson};
use rover_openapi;

pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, config: ServerConfig, openapi_spec: Option<serde_json::Value>) {
    std::thread::spawn(move || -> Result<()> {
        let fast_router = FastRouter::from_routes(routes)?;

        while let Some(req) = rx.blocking_recv() {
            // Methods should be only lua functions, so lua function is utf8 safe
            let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };

            let method = match HttpMethod::from_str(method_str) {
                Some(m) => m,
                None => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: Bytes::from(format!(
                            "Invalid HTTP method '{}'. Valid methods: {}",
                            method_str,
                            HttpMethod::valid_methods().join(", ")
                        )),
                    });
                    continue;
                }
            };

            // Paths should be only lua functions, so lua function is utf8 safe
            let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

            if tracing::event_enabled!(tracing::Level::DEBUG) {
                if !req.query.is_empty() {
                    debug!("  ├─ query: {:?}", req.query);
                }
                if let Some(ref body) = req.body {
                    let body_display = std::str::from_utf8(body).unwrap_or("<binary data>");
                    debug!("  └─ body: {}", body_display);
                }
            }

            // Handle /docs endpoint if enabled and spec is available
            if config.docs && path_str == "/docs" && openapi_spec.is_some() {
                let html = rover_openapi::scalar_html(openapi_spec.as_ref().unwrap());
                let elapsed = req.started_at.elapsed();
                debug!("GET /docs - 200 OK in {:.2}ms", elapsed.as_secs_f64() * 1000.0);
                let _ = req.respond_to.send(LuaResponse {
                    status: StatusCode::OK,
                    body: Bytes::from(html),
                });
                continue;
            }

            let (handler, params) = match fast_router.match_route(method, path_str) {
                Some((h, p)) => (h, p),
                None => {
                    let elapsed = req.started_at.elapsed();
                    warn!(
                        "{} {} - 404 NOT_FOUND in {:.2}ms",
                        method,
                        path_str,
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::NOT_FOUND,
                        body: Bytes::from("Route not found"),
                    });
                    continue;
                }
            };

            let ctx = match build_lua_context(&lua, &req, &params) {
                Ok(c) => c,
                Err(e) => {
                    let error_msg = e.to_string();
                    let status = if error_msg.contains("Invalid UTF-8") {
                        StatusCode::BAD_REQUEST
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    };
                    let _ = req.respond_to.send(LuaResponse {
                        status,
                        body: Bytes::from(error_msg),
                    });
                    continue;
                }
            };

            let result: Value = match handler.call(ctx) {
                Ok(r) => r,
                Err(e) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: Bytes::from(format!("Lua error: {}", e)),
                    });
                    continue;
                }
            };

            let (status, body) = match result {
                Value::UserData(ref ud) => {
                    if let Ok(response) = ud.borrow::<RoverResponse>() {
                        (
                            StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK),
                            response.body.clone(),
                        )
                    } else {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Bytes::from("Invalid userdata type"),
                        )
                    }
                }

                Value::String(ref s) => (
                    StatusCode::OK,
                    Bytes::from(s.to_str().unwrap().to_string())
                ),

                Value::Table(table) => {
                    let json = lua_table_to_json(&table).unwrap_or_else(|e| {
                        format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
                    });
                    (StatusCode::OK, Bytes::from(json))
                }

                // Fast path: integers
                Value::Integer(i) => (StatusCode::OK, Bytes::from(i.to_string())),

                // Fast path: numbers
                Value::Number(n) => (StatusCode::OK, Bytes::from(n.to_string())),

                // Fast path: booleans
                Value::Boolean(b) => (StatusCode::OK, Bytes::from(b.to_string())),

                // Fast path: nil -> 204 No Content
                Value::Nil => (StatusCode::NO_CONTENT, Bytes::new()),

                // Lua errors
                Value::Error(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Bytes::from(e.to_string())
                ),

                // Unsupported types
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Bytes::from("Unsupported return type"),
                ),
            };

            if tracing::event_enabled!(tracing::Level::DEBUG) {
                let body_preview = if body.len() > 200 {
                    format!("{}... ({} bytes)", String::from_utf8_lossy(&body[..200]), body.len())
                } else {
                    String::from_utf8_lossy(&body).to_string()
                };
                debug!("  └─ response body: {}", body_preview);
            }

            let elapsed = req.started_at.elapsed();
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

            if status.is_success() {
                if tracing::event_enabled!(tracing::Level::INFO) {
                    info!(
                        "{} {} - {} in {:.2}ms",
                        method,
                        path_str,
                        status.as_u16(),
                        elapsed_ms
                    );
                }
            } else if status.is_client_error() || status.is_server_error() {
                if tracing::event_enabled!(tracing::Level::WARN) {
                    warn!(
                        "{} {} - {} in {:.2}ms",
                        method,
                        path_str,
                        status.as_u16(),
                        elapsed_ms
                    );
                }
            }

            let _ = req.respond_to.send(LuaResponse { status, body });
        }
        Ok(())
    });
}

fn build_lua_context(
    lua: &Lua,
    req: &LuaRequest,
    params: &HashMap<String, String>,
) -> Result<Table> {
    let ctx = lua.create_table()?;

    let method_str = std::str::from_utf8(&req.method)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in HTTP method".to_string()))?;
    ctx.set("method", method_str)?;

    let path_str = std::str::from_utf8(&req.path)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in request path".to_string()))?;
    ctx.set("path", path_str)?;

    let headers_clone = req.headers.clone();
    let query_clone = req.query.clone();
    let params_clone = params.clone();
    let body_clone = req.body.clone();

    let headers_fn = lua.create_function(move |lua, ()| {
        if headers_clone.is_empty() {
            return lua.create_table();
        }
        let headers = lua.create_table_with_capacity(0, headers_clone.len())?;
        for (k, v) in &headers_clone {
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

    let query_fn = lua.create_function(move |lua, ()| {
        if query_clone.is_empty() {
            return lua.create_table();
        }
        let query = lua.create_table_with_capacity(0, query_clone.len())?;
        for (k, v) in &query_clone {
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

    let params_fn = lua.create_function(move |lua, ()| {
        if params_clone.is_empty() {
            return lua.create_table();
        }
        let params_table = lua.create_table_with_capacity(0, params_clone.len())?;
        for (k, v) in &params_clone {
            params_table.set(k.as_str(), v.as_str())?;
        }
        Ok(params_table)
    })?;
    ctx.set("params", params_fn)?;

    let body_fn = lua.create_function(move |lua, ()| {
        if let Some(body) = &body_clone {
            let body_str = std::str::from_utf8(body).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Request body contains invalid UTF-8 (binary data not supported)".to_string(),
                )
            })?;
            
            let globals = lua.globals();
            let rover: Table = globals.get("rover")?;
            let guard: Table = rover.get("guard")?;
            
            if let Ok(constructor) = guard.get::<mlua::Function>("__body_value") {
                constructor.call(body_str.to_string())
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

fn lua_table_to_json(table: &Table) -> Result<String> {
    table
        .to_json_string()
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))
}
