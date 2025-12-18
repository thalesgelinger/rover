use std::collections::HashMap;

use anyhow::Result;
use hyper::StatusCode;
use matchit::Router;
use mlua::{
    Function, Lua, LuaSerdeExt, Table,
    Value::{self},
};
use smallvec::SmallVec;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{HttpMethod, LuaRequest, LuaResponse, Route, ServerConfig};

pub struct FastRouter {
    router: Router<SmallVec<[(HttpMethod, usize); 2]>>,
    handlers: Vec<Function>,
}

impl FastRouter {
    pub fn from_routes(routes: Vec<Route>) -> Result<Self> {
        let mut router = Router::new();
        let mut handlers = Vec::new();
        let mut pattern_map: HashMap<Vec<u8>, SmallVec<[(HttpMethod, usize); 2]>> = HashMap::new();

        // Group routes by pattern
        for route in routes {
            let handler_idx = handlers.len();
            handlers.push(route.handler);
            
            pattern_map
                .entry(route.pattern.to_vec())
                .or_insert_with(SmallVec::new)
                .push((route.method, handler_idx));
        }

        // Insert into router
        for (pattern_bytes, methods) in pattern_map {
            let pattern_str = std::str::from_utf8(&pattern_bytes)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
            router.insert(pattern_str, methods)?;
        }

        Ok(Self { router, handlers })
    }

    pub fn match_route(
        &self,
        method: HttpMethod,
        path: &str,
    ) -> Option<(&Function, HashMap<String, String>)> {
        let matched = self.router.at(path).ok()?;
        
        // Find handler for this method
        let handler_idx = matched.value
            .iter()
            .find(|(m, _)| *m == method)
            .map(|(_, idx)| *idx)?;
        
        let handler = &self.handlers[handler_idx];
        
        // Build params with capacity hint
        let mut params = HashMap::with_capacity(matched.params.len());
        for (name, value) in matched.params.iter() {
            let decoded = urlencoding::decode(value).ok()?.into_owned();
            if decoded.is_empty() {
                return None;
            }
            params.insert(name.to_string(), decoded);
        }
        
        Some((handler, params))
    }
}

pub fn run(lua: Lua, routes: Vec<Route>, mut rx: mpsc::Receiver<LuaRequest>, config: ServerConfig) {
    std::thread::spawn(move || {
        let fast_router = match FastRouter::from_routes(routes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to initialize router: {}", e);
                return;
            }
        };
        
        while let Some(req) = rx.blocking_recv() {
            // Validate UTF-8 in method
            let method_str = match std::str::from_utf8(&req.method) {
                Ok(s) => s,
                Err(_) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: "Invalid UTF-8 encoding in HTTP method".to_string(),
                    });
                    continue;
                }
            };

            // Convert to HttpMethod
            let method = match HttpMethod::from_str(method_str) {
                Some(m) => m,
                None => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: format!(
                            "Invalid HTTP method '{}'. Valid methods: {}",
                            method_str,
                            HttpMethod::valid_methods().join(", ")
                        ),
                    });
                    continue;
                }
            };

            let path_str = match std::str::from_utf8(&req.path) {
                Ok(s) => s,
                Err(_) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: "Invalid UTF-8 encoding in request path".to_string(),
                    });
                    continue;
                }
            };

            // Log incoming request
            if config.debug && !req.query.is_empty() {
                debug!("  ├─ query: {:?}", req.query);
            }
            if config.debug {
                if let Some(ref body) = req.body {
                    let body_display = std::str::from_utf8(body).unwrap_or("<binary data>");
                    debug!("  └─ body: {}", body_display);
                }
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
                        body: "Route not found".to_string(),
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
                        body: error_msg,
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
                    method,
                    path_str,
                    status.as_u16(),
                    elapsed_ms
                );
            } else if status.is_client_error() || status.is_server_error() {
                warn!(
                    "{} {} - {} in {:.2}ms",
                    method,
                    path_str,
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
    
    let method_str = std::str::from_utf8(&req.method)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in HTTP method".to_string()))?;
    ctx.set("method", method_str)?;
    
    let path_str = std::str::from_utf8(&req.path)
        .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in request path".to_string()))?;
    ctx.set("path", path_str)?;

    let headers = lua.create_table()?;
    for (k, v) in &req.headers {
        let k_str = std::str::from_utf8(k)
            .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in header name".to_string()))?;
        let v_str = std::str::from_utf8(v)
            .map_err(|_| mlua::Error::RuntimeError(format!("Invalid UTF-8 in header value for '{}'", k_str)))?;
        headers.set(k_str, v_str)?;
    }
    ctx.set("headers", headers)?;

    let query = lua.create_table()?;
    for (k, v) in &req.query {
        let k_str = std::str::from_utf8(k)
            .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in query parameter name".to_string()))?;
        let v_str = std::str::from_utf8(v)
            .map_err(|_| mlua::Error::RuntimeError(format!("Invalid UTF-8 in query parameter '{}'", k_str)))?;
        query.set(k_str, v_str)?;
    }
    ctx.set("query", query)?;

    let params_table = lua.create_table()?;
    for (k, v) in params {
        params_table.set(k.as_str(), v.as_str())?;
    }
    ctx.set("params", params_table)?;

    if let Some(body) = &req.body {
        let body_str = std::str::from_utf8(body)
            .map_err(|_| mlua::Error::RuntimeError("Request body contains invalid UTF-8 (binary data not supported)".to_string()))?;
        ctx.set("body", body_str)?;
    }

    Ok(ctx)
}



fn lua_table_to_json(lua: &Lua, table: Table) -> mlua::Result<String> {
    let json_value: serde_json::Value = lua.from_value(Value::Table(table))?;
    Ok(serde_json::to_string(&json_value).unwrap())
}
