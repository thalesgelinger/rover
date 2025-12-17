mod event_loop;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use smallvec::SmallVec;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Instant;
use tokio::net::TcpListener;

use anyhow::{Result, anyhow};

use mlua::{
    FromLua, Function, Lua,
    Value::{self},
};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

#[derive(Clone)]
pub struct Route {
    pub method: Bytes,
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
    port: u16,
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
                    port: config.get::<u16>("port").unwrap_or(4242),
                    host: config.get::<String>("host").unwrap_or("localhost".into()),
                    debug,
                })
            }
            _ => Err(anyhow!("Server config must be a table"))?,
        }
    }
}

struct LuaRequest {
    method: Bytes,
    path: Bytes,
    headers: SmallVec<[(Bytes, Bytes); 16]>,
    query: SmallVec<[(Bytes, Bytes); 8]>,
    body: Option<Bytes>,
    respond_to: oneshot::Sender<LuaResponse>,
    started_at: Instant,
}

struct LuaResponse {
    status: StatusCode,
    body: String,
}

async fn server(lua: Lua, routes: RouteTable, config: ServerConfig) -> Result<()> {
    let (tx, rx) = mpsc::channel(1024);

    let addr = format!("{}:{}", config.host, config.port);
    info!("🚀 Rover server running at http://{}", addr);
    if config.debug {
        info!("🐛 Debug mode enabled");
    }

    let host: [u8; 4] = if config.host == "localhost" {
        [127, 0, 0, 1]
    } else {
        let parts: Vec<u8> = config
            .host
            .split('.')
            .filter_map(|s| s.parse::<u8>().ok())
            .collect();

        parts.try_into().unwrap_or([127, 0, 0, 1])
    };

    let addr = SocketAddr::from((host, config.port));

    let listener = TcpListener::bind(addr).await?;

    event_loop::run(lua, routes.routes, rx, config.clone());

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let tx = tx.clone();

        tokio::task::spawn(async move {
            if let Err(err) = auto::Builder::new(TokioExecutor::new())
                .serve_connection(io, service_fn(move |req| handler(req, tx.clone())))
                .await
            {
                eprintln!("Error serving connection: {}", err);
            }
        });
    }
}

async fn handler(
    req: Request<hyper::body::Incoming>,
    tx: mpsc::Sender<LuaRequest>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body_stream) = req.into_parts();

    let headers: SmallVec<[(Bytes, Bytes); 16]> = parts
        .headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str().ok().map(|v_str| {
                (
                    Bytes::copy_from_slice(k.as_str().as_bytes()),
                    Bytes::copy_from_slice(v_str.as_bytes()),
                )
            })
        })
        .collect();

    let query: SmallVec<[(Bytes, Bytes); 8]> = parts
        .uri
        .query()
        .map(|q| {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| {
                    (
                        Bytes::copy_from_slice(k.as_bytes()),
                        Bytes::copy_from_slice(v.as_bytes()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let body_bytes = http_body_util::BodyExt::collect(body_stream)
        .await
        .unwrap()
        .to_bytes();
    let body = if !body_bytes.is_empty() {
        Some(body_bytes)
    } else {
        None
    };

    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(LuaRequest {
        method: Bytes::copy_from_slice(parts.method.as_str().as_bytes()),
        path: Bytes::copy_from_slice(parts.uri.path().as_bytes()),
        headers,
        query,
        body,
        respond_to: resp_tx,
        started_at: Instant::now(),
    })
    .await
    .unwrap();

    let resp = resp_rx.await.unwrap();

    let mut response = Response::new(Full::new(Bytes::from(resp.body)));
    *response.status_mut() = resp.status.into();
    Ok(response)
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
    let _ = runtime.block_on(server(lua, routes, config));
}
