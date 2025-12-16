use std::collections::HashMap;
use std::time::Instant;

use anyhow::anyhow;
use axum::{
    Router, body, extract::Request, http::StatusCode, response::IntoResponse, routing::any,
};
use mlua::{
    FromLua, Function, Lua, LuaSerdeExt, Table,
    Value::{self},
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct Route {
    pub method: String,
    pub pattern: String,
    pub param_names: Vec<String>,
    pub handler: Function,
    pub is_static: bool,
}

pub struct RouteTable {
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    port: i32,
    host: String,
    debug: bool,
}

impl FromLua for ServerConfig {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Table(config) => {
                let debug = config.get::<Value>("debug")?;
                let debug = match debug {
                    Value::Nil => true,
                    Value::Boolean(val) => val,
                    _ => Err(anyhow!("Debug should be boolean"))?,
                };

                Ok(ServerConfig {
                    port: config.get::<i32>("port").unwrap_or(4242),
                    host: config.get::<String>("host").unwrap_or("localhost".into()),
                    debug,
                })
            }
            _ => Err(anyhow!("Server config must be a table"))?,
        }
    }
}

struct LuaRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Option<String>,
    respond_to: oneshot::Sender<LuaResponse>,
    started_at: Instant,
}

struct LuaResponse {
    status: StatusCode,
    body: String,
}

fn lua_table_to_json(lua: &Lua, table: Table) -> mlua::Result<String> {
    let json_value: serde_json::Value = lua.from_value(Value::Table(table))?;
    Ok(serde_json::to_string(&json_value).unwrap())
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

async fn server(lua: Lua, routes: RouteTable, config: ServerConfig) {
    let (tx, rx) = mpsc::channel(1024);
    let config_clone = config.clone();
    event_loop(lua, routes, rx, config.clone());

    let addr = format!("{}:{}", config.host, config.port);
    let app = Router::new().fallback(any(move |req| {
        handle_all(req, tx.clone(), config_clone.clone())
    }));

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    info!("🚀 Rover server running at http://{}", addr);
    if config.debug {
        info!("🐛 Debug mode enabled");
    }

    axum::serve(listener, app).await.unwrap();
}

fn event_loop(
    lua: Lua,
    routes: RouteTable,
    mut rx: mpsc::Receiver<LuaRequest>,
    config: ServerConfig,
) {
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

async fn handle_all(
    req: Request,
    tx: mpsc::Sender<LuaRequest>,
    _config: ServerConfig,
) -> impl IntoResponse {
    let (parts, body_stream) = req.into_parts();

    let headers: HashMap<String, String> = parts
        .headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    let query: HashMap<String, String> = parts
        .uri
        .query()
        .map(|q| {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();

    let body_bytes = body::to_bytes(body_stream, usize::MAX).await.unwrap();
    let body_str = if !body_bytes.is_empty() {
        Some(String::from_utf8_lossy(&body_bytes).to_string())
    } else {
        None
    };

    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(LuaRequest {
        method: parts.method.to_string().to_lowercase(),
        path: parts.uri.path().to_string(),
        headers,
        query,
        body: body_str,
        respond_to: resp_tx,
        started_at: Instant::now(),
    })
    .await
    .unwrap();

    let resp = resp_rx.await.unwrap();
    (resp.status, resp.body)
}

pub fn run(lua: Lua, routes: RouteTable, config: ServerConfig) {
    let log_level = if config.debug { "debug" } else { "info" };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(server(lua, routes, config));
}
