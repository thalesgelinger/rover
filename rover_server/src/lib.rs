mod event_loop;
mod status_code;
use std::collections::HashMap;
use std::time::Instant;

use anyhow::anyhow;

use axum::{Router, body, extract::Request, response::IntoResponse, routing::any};

use mlua::{
    FromLua, Function, Lua,
    Value::{self},
};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use crate::status_code::StatusCode;

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

async fn server(lua: Lua, routes: RouteTable, config: ServerConfig) {
    let (tx, rx) = mpsc::channel(1024);
    let config_clone = config.clone();
    event_loop::run(lua, routes, rx, config.clone());

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
