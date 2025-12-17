use std::collections::HashMap;

use hyper::StatusCode;
use matchit::Router;
use mlua::{
    Function, Lua, LuaSerdeExt, Table,
    Value::{self},
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{LuaRequest, LuaResponse, Route, ServerConfig};
use crate::request_context::RequestContext;

pub struct FastRouter {
    get_router: Router<usize>,
    post_router: Router<usize>,
    put_router: Router<usize>,
    patch_router: Router<usize>,
    delete_router: Router<usize>,
    handlers: Vec<Function>,
}

impl FastRouter {
    pub fn from_routes(routes: Vec<Route>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut get_router = Router::new();
        let mut post_router = Router::new();
        let mut put_router = Router::new();
        let mut patch_router = Router::new();
        let mut delete_router = Router::new();
        let mut handlers = Vec::new();

        for (idx, route) in routes.into_iter().enumerate() {
            let method_str = std::str::from_utf8(&route.method)?;
            // Convert :param to {param} for matchit syntax
            let matchit_pattern = route.pattern
                .split('/')
                .map(|seg| {
                    if let Some(param) = seg.strip_prefix(':') {
                        format!("{{{}}}", param)
                    } else {
                        seg.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("/");
            
            let router = match method_str.to_lowercase().as_str() {
                "get" => &mut get_router,
                "post" => &mut post_router,
                "put" => &mut put_router,
                "patch" => &mut patch_router,
                "delete" => &mut delete_router,
                _ => return Err(format!("Unknown HTTP method: {}", method_str).into()),
            };
            
            router.insert(&matchit_pattern, idx)?;
            handlers.push(route.handler);
        }

        Ok(Self { 
            get_router,
            post_router,
            put_router,
            patch_router,
            delete_router,
            handlers 
        })
    }

    pub fn match_route(&self, method: &str, path: &str) -> Option<(&Function, HashMap<String, String>)> {
        let router = match method.to_lowercase().as_str() {
            "get" => &self.get_router,
            "post" => &self.post_router,
            "put" => &self.put_router,
            "patch" => &self.patch_router,
            "delete" => &self.delete_router,
            _ => return None,
        };
        
        let matched = router.at(path).ok()?;
        let handler = &self.handlers[*matched.value];
        
        let mut params = HashMap::new();
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
            // Validate UTF-8 in method and path
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

            let (handler, params) = match fast_router.match_route(method_str, path_str) {
                Some((h, p)) => (h, p),
                None => {
                    let elapsed = req.started_at.elapsed();
                    warn!(
                        "{} {} - 404 NOT_FOUND in {:.2}ms",
                        method_str,
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

            let ctx = build_lua_context(&req, params);

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
                    method_str,
                    path_str,
                    status.as_u16(),
                    elapsed_ms
                );
            } else if status.is_client_error() || status.is_server_error() {
                warn!(
                    "{} {} - {} in {:.2}ms",
                    method_str,
                    path_str,
                    status.as_u16(),
                    elapsed_ms
                );
            }

            let _ = req.respond_to.send(LuaResponse { status, body });
        }
    });
}

fn build_lua_context(req: &LuaRequest, params: HashMap<String, String>) -> RequestContext {
    RequestContext {
        method: req.method.clone(),
        path: req.path.clone(),
        headers: req.headers.clone(),
        query: req.query.clone(),
        params,
        body: req.body.clone(),
    }
}



fn lua_table_to_json(lua: &Lua, table: Table) -> mlua::Result<String> {
    let json_value: serde_json::Value = lua.from_value(Value::Table(table))?;
    Ok(serde_json::to_string(&json_value).unwrap())
}
