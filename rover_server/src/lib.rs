mod event_loop;
mod to_json;
mod fast_router;
use http_body_util::Full;
pub use hyper::body::Bytes;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HttpMethod {
    Get = 1,
    Post = 2,
    Put = 4,
    Patch = 8,
    Delete = 16,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        match bytes.len() {
            3 => {
                if bytes.eq_ignore_ascii_case(b"get") {
                    Some(Self::Get)
                } else if bytes.eq_ignore_ascii_case(b"put") {
                    Some(Self::Put)
                } else {
                    None
                }
            }
            4 => {
                if bytes.eq_ignore_ascii_case(b"post") {
                    Some(Self::Post)
                } else {
                    None
                }
            }
            5 => {
                if bytes.eq_ignore_ascii_case(b"patch") {
                    Some(Self::Patch)
                } else {
                    None
                }
            }
            6 => {
                if bytes.eq_ignore_ascii_case(b"delete") {
                    Some(Self::Delete)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    pub fn valid_methods() -> &'static [&'static str] {
        &["get", "post", "put", "patch", "delete"]
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone)]
pub struct Route {
    pub method: HttpMethod,
    pub pattern: Bytes,
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
    log_level: String,
}

impl FromLua for ServerConfig {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Table(config) => {
                let log_level = config.get::<Value>("log_level")?;
                let log_level = match log_level {
                    Value::Nil => "debug".to_string(),
                    Value::String(s) => {
                        let level = s.to_str()?.to_lowercase();
                        match level.as_str() {
                            "debug" | "info" | "warn" | "error" | "nope" => level,
                            _ => Err(anyhow!(
                                "log_level must be one of: debug, info, warn, error, nope"
                            ))?,
                        }
                    }
                    _ => Err(anyhow!("log_level should be a string"))?,
                };

                Ok(ServerConfig {
                    port: config.get::<u16>("port").unwrap_or(4242),
                    host: config.get::<String>("host").unwrap_or("localhost".into()),
                    log_level,
                })
            }
            _ => Err(anyhow!("Server config must be a table"))?,
        }
    }
}

struct LuaRequest {
    method: Bytes,
    path: Bytes,
    headers: SmallVec<[(Bytes, Bytes); 8]>,
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
    if config.log_level != "nope" {
        info!("🚀 Rover server running at http://{}", addr);
        if config.log_level == "debug" {
            info!("🐛 Debug mode enabled");
        }
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

    let headers: SmallVec<[(Bytes, Bytes); 8]> = if parts.headers.is_empty() {
        SmallVec::new()
    } else {
        parts
            .headers
            .iter()
            .filter_map(|(k, v)| {
                v.to_str().ok().map(|v_str| {
                    (
                        Bytes::from(k.as_str().to_string()),
                        Bytes::from(v_str.to_string()),
                    )
                })
            })
            .collect()
    };

    let query: SmallVec<[(Bytes, Bytes); 8]> = match parts.uri.query() {
        Some(q) => form_urlencoded::parse(q.as_bytes())
            .map(|(k, v)| {
                (
                    Bytes::from(k.into_owned()),
                    Bytes::from(v.into_owned()),
                )
            })
            .collect(),
        None => SmallVec::new(),
    };

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
        method: Bytes::from(parts.method.as_str().to_string()),
        path: Bytes::from(parts.uri.path().to_string()),
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
    if config.log_level != "nope" {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level)),
            )
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .init();
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _ = runtime.block_on(server(lua, routes, config));
}
