mod buffer_pool;
mod connection;
pub mod direct_json_parser;
mod event_loop;
mod fast_router;
mod http_server;
pub mod http_task;
mod response;
pub mod table_pool;
pub mod to_json;
pub mod ws_frame;
pub mod ws_handshake;
pub mod ws_lua;
pub mod ws_manager;

pub use http_task::{CoroutineResponse, HttpResponse};
pub use response::RoverResponse;
use std::net::SocketAddr;

use anyhow::anyhow;

use mlua::{
    FromLua, Function, Lua,
    Value::{self},
};
use tracing::info;

pub type Bytes = bytes::Bytes;

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

pub struct WsRoute {
    pub pattern: Bytes,
    pub param_names: Vec<String>,
    pub is_static: bool,
    pub endpoint_config: ws_manager::WsEndpointConfig,
}

pub struct RouteTable {
    pub routes: Vec<Route>,
    pub ws_routes: Vec<WsRoute>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub log_level: String,
    pub docs: bool,
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
                    docs: match config.get::<Value>("docs")? {
                        Value::Nil => true,
                        Value::Boolean(b) => b,
                        _ => true,
                    },
                })
            }
            _ => Err(anyhow!("Server config must be a table"))?,
        }
    }
}

pub fn run(
    lua: Lua,
    routes: RouteTable,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
) {
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

    let addr = format!("{}:{}", config.host, config.port);
    if config.log_level != "nope" {
        info!("🚀 Rover server running at http://{}", addr);
        if config.docs && openapi_spec.is_some() {
            info!("📚 API docs available at http://{}/docs", addr);
        }
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

    let sock_addr = SocketAddr::from((host, config.port));

    match http_server::run_server(
        lua,
        routes.routes,
        routes.ws_routes,
        config,
        openapi_spec,
        sock_addr,
    ) {
        Ok(_) => {}
        Err(e) => {
            if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::AddrInUse {
                    eprintln!("\n❌ Error: Unable to start server");
                    eprintln!(
                        "   Port {} is already in use on {}",
                        sock_addr.port(),
                        sock_addr.ip()
                    );
                    eprintln!(
                        "   Please choose a different port or stop the process using port {}\n",
                        sock_addr.port()
                    );
                    std::process::exit(1);
                }
            }
            eprintln!("\n❌ Error starting server: {}\n", e);
            std::process::exit(1);
        }
    }
}
