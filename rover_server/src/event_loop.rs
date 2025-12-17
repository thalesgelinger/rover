use std::collections::HashMap;

use mlua::{
    Function, Lua, LuaSerdeExt, Table,
    Value::{self},
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{LuaRequest, LuaResponse, RouteTable, ServerConfig, status_code::StatusCode};

pub fn run(lua: Lua, routes: RouteTable, mut rx: mpsc::Receiver<LuaRequest>, config: ServerConfig) {
    std::thread::spawn(move || {
        while let Some(req) = rx.blocking_recv() {
            // Log incoming request
            if config.debug && !req.query.is_empty() {
                debug!("  ├─ query: {:?}", req.query);
            }
            if config.debug {
                if let Some(ref body) = req.body {
                    debug!("  └─ body: {}", body);
                }
            }

            let (handler, params) = match match_route(&routes, &req.method, &req.path) {
                Some((h, p)) => (h, p),
                None => {
                    let elapsed = req.started_at.elapsed();
                    warn!(
                        "{} {} - 404 NOT_FOUND in {:.2}ms",
                        req.method.to_uppercase(),
                        req.path,
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::NOT_FOUND,
                        body: "Route not found".to_string(),
                    });
                    continue;
                }
            };

            let ctx = match build_lua_context(&lua, &req, &params) {
                Ok(c) => c,
                Err(e) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: format!("Failed to build context: {}", e),
                    });
                    continue;
                }
            };

            let result: Value = match handler.call(ctx) {
                Ok(r) => r,
                Err(e) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: format!("Lua error: {}", e),
                    });
                    continue;
                }
            };

            let (status, body) = match result {
                Value::String(ref s) => (StatusCode::OK, s.to_str().unwrap().to_string()),

                Value::Table(table) => {
                    if let Ok(status_code) = table.get::<u16>("status") {
                        if status_code >= 400 {
                            let message = table
                                .get::<String>("message")
                                .unwrap_or_else(|_| "Error".to_string());
                            (
                                StatusCode::from_u16(status_code)
                                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                                message,
                            )
                        } else {
                            let body = lua_table_to_json(&lua, table).unwrap_or_else(|e| {
                                format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
                            });
                            (
                                StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                                body,
                            )
                        }
                    } else {
                        let json = lua_table_to_json(&lua, table).unwrap_or_else(|e| {
                            format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
                        });
                        (StatusCode::OK, json)
                    }
                }

                Value::Integer(i) => (StatusCode::OK, i.to_string()),
                Value::Number(n) => (StatusCode::OK, n.to_string()),

                Value::Boolean(b) => (StatusCode::OK, b.to_string()),

                Value::Nil => (StatusCode::NO_CONTENT, String::new()),

                Value::Error(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),

                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unsupported return type".to_string(),
                ),
            };

            // Log response
            let elapsed = req.started_at.elapsed();
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

            if status.is_success() {
                info!(
                    "{} {} - {} in {:.2}ms",
                    req.method.to_uppercase(),
                    req.path,
                    status.as_u16(),
                    elapsed_ms
                );
            } else if status.is_client_error() || status.is_server_error() {
                warn!(
                    "{} {} - {} in {:.2}ms",
                    req.method.to_uppercase(),
                    req.path,
                    status.as_u16(),
                    elapsed_ms
                );
            }

            let _ = req.respond_to.send(LuaResponse { status, body });
        }
    });
}

fn build_lua_context(
    lua: &Lua,
    req: &LuaRequest,
    params: &HashMap<String, String>,
) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("method", req.method.as_str())?;
    ctx.set("path", req.path.as_str())?;

    let headers = lua.create_table()?;
    for (k, v) in &req.headers {
        headers.set(k.as_str(), v.as_str())?;
    }
    ctx.set("headers", headers)?;

    let query = lua.create_table()?;
    for (k, v) in &req.query {
        query.set(k.as_str(), v.as_str())?;
    }
    ctx.set("query", query)?;

    let params_table = lua.create_table()?;
    for (k, v) in params {
        params_table.set(k.as_str(), v.as_str())?;
    }
    ctx.set("params", params_table)?;

    if let Some(body) = &req.body {
        ctx.set("body", body.as_str())?;
    }

    Ok(ctx)
}

fn match_route<'a>(
    routes: &'a RouteTable,
    method: &str,
    path: &str,
) -> Option<(&'a Function, HashMap<String, String>)> {
    for route in &routes.routes {
        if route.method != method {
            continue;
        }

        if let Some(params) = matches_pattern(&route.pattern, path, &route.param_names) {
            return Some((&route.handler, params));
        }
    }
    None
}

fn matches_pattern(
    pattern: &str,
    path: &str,
    _param_names: &[String],
) -> Option<HashMap<String, String>> {
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Must have same number of segments
    if pattern_parts.len() != path_parts.len() {
        return None;
    }

    let mut params = HashMap::new();

    for (pattern_seg, path_seg) in pattern_parts.iter().zip(path_parts.iter()) {
        if pattern_seg.starts_with(':') {
            // Param segment - capture value
            let param_name = pattern_seg.strip_prefix(':').unwrap();

            // URL decode the value
            let decoded = urlencoding::decode(path_seg).ok()?.into_owned();

            // Check for empty param value (from double slash)
            if decoded.is_empty() {
                return None; // Reject empty params
            }

            params.insert(param_name.to_string(), decoded);
        } else {
            // Static segment - must match exactly
            if pattern_seg != path_seg {
                return None;
            }
        }
    }

    Some(params)
}

fn lua_table_to_json(lua: &Lua, table: Table) -> mlua::Result<String> {
    let json_value: serde_json::Value = lua.from_value(Value::Table(table))?;
    Ok(serde_json::to_string(&json_value).unwrap())
}
