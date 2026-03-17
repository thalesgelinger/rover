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
    FromLua, Function, Lua, RegistryKey,
    Value::{self},
};
use std::sync::Arc;
use tracing::info;

pub type Bytes = bytes::Bytes;
const DEFAULT_BODY_SIZE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HttpMethod {
    Get = 1,
    Head = 2,
    Options = 3,
    Post = 4,
    Put = 5,
    Patch = 6,
    Delete = 7,
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
                } else if bytes.eq_ignore_ascii_case(b"head") {
                    Some(Self::Head)
                } else {
                    None
                }
            }
            7 => {
                if bytes.eq_ignore_ascii_case(b"options") {
                    Some(Self::Options)
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
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    pub fn valid_methods() -> &'static [&'static str] {
        &["get", "head", "options", "post", "put", "patch", "delete"]
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Middleware function with shared ownership
#[derive(Clone)]
pub struct MiddlewareHandler {
    pub name: String,
    pub handler: Arc<RegistryKey>,
}

/// Chain of middlewares to execute before/after the route handler
#[derive(Default, Clone)]
pub struct MiddlewareChain {
    pub before: Vec<MiddlewareHandler>,
    pub after: Vec<MiddlewareHandler>,
}

impl MiddlewareChain {
    pub fn is_empty(&self) -> bool {
        self.before.is_empty() && self.after.is_empty()
    }
}

#[derive(Clone)]
pub struct Route {
    pub method: HttpMethod,
    pub pattern: Bytes,
    pub param_names: Vec<String>,
    pub handler: Function,
    pub is_static: bool,
    pub middlewares: MiddlewareChain,
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
    /// Optional error handler function (api.on_error)
    pub error_handler: Option<Arc<RegistryKey>>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub log_level: String,
    pub docs: bool,
    /// Maximum body size in bytes (None = no limit)
    pub body_size_limit: Option<usize>,
    pub cors_origin: Option<String>,
    pub cors_methods: String,
    pub cors_headers: String,
    pub cors_credentials: bool,
    pub strict_mode: bool,
    pub allow_public_bind: bool,
    pub allow_wildcard_cors_credentials: bool,
    pub allow_unbounded_body: bool,
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

                let strict_mode = match config.get::<Value>("strict_mode")? {
                    Value::Nil => true,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("strict_mode should be a boolean"))?,
                };

                let allow_public_bind = match config.get::<Value>("allow_public_bind")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("allow_public_bind should be a boolean"))?,
                };

                let allow_wildcard_cors_credentials =
                    match config.get::<Value>("allow_wildcard_cors_credentials")? {
                        Value::Nil => false,
                        Value::Boolean(b) => b,
                        _ => Err(anyhow!(
                            "allow_wildcard_cors_credentials should be a boolean"
                        ))?,
                    };

                let allow_unbounded_body = match config.get::<Value>("allow_unbounded_body")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("allow_unbounded_body should be a boolean"))?,
                };

                let body_size_limit = match config.get::<Value>("body_size_limit")? {
                    Value::Nil => Some(DEFAULT_BODY_SIZE_LIMIT),
                    Value::Integer(n) if n > 0 => Some(n as usize),
                    Value::Number(n) if n > 0.0 => Some(n as usize),
                    Value::Integer(_) | Value::Number(_) => None,
                    _ => Err(anyhow!(
                        "body_size_limit should be a positive number, or 0 to disable"
                    ))?,
                };

                let host = config.get::<String>("host").unwrap_or("localhost".into());
                let docs = match config.get::<Value>("docs")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => false,
                };
                let cors_origin = match config.get::<Value>("cors_origin")? {
                    Value::Nil => None,
                    Value::String(s) => Some(s.to_str()?.to_string()),
                    _ => None,
                };
                let cors_methods = match config.get::<Value>("cors_methods")? {
                    Value::Nil => "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD".to_string(),
                    Value::String(s) => s.to_str()?.to_string(),
                    _ => "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD".to_string(),
                };
                let cors_headers = match config.get::<Value>("cors_headers")? {
                    Value::Nil => "Content-Type, Authorization".to_string(),
                    Value::String(s) => s.to_str()?.to_string(),
                    _ => "Content-Type, Authorization".to_string(),
                };
                let cors_credentials = match config.get::<Value>("cors_credentials")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => false,
                };

                if strict_mode {
                    if host != "localhost" && host != "127.0.0.1" && !allow_public_bind {
                        Err(anyhow!(
                            "strict_mode blocks host '{}'. Use localhost/127.0.0.1, or set allow_public_bind = true",
                            host
                        ))?;
                    }
                    if body_size_limit.is_none() && !allow_unbounded_body {
                        Err(anyhow!(
                            "strict_mode requires body_size_limit. Set a positive limit, or set allow_unbounded_body = true"
                        ))?;
                    }
                    if cors_credentials
                        && matches!(cors_origin.as_deref(), Some("*"))
                        && !allow_wildcard_cors_credentials
                    {
                        Err(anyhow!(
                            "strict_mode blocks cors_origin='*' with cors_credentials=true. Set a specific origin, or set allow_wildcard_cors_credentials = true"
                        ))?;
                    }
                }

                Ok(ServerConfig {
                    port: config.get::<u16>("port").unwrap_or(4242),
                    host,
                    log_level,
                    docs,
                    body_size_limit,
                    cors_origin,
                    cors_methods,
                    cors_headers,
                    cors_credentials,
                    strict_mode,
                    allow_public_bind,
                    allow_wildcard_cors_credentials,
                    allow_unbounded_body,
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
    let error_handler = routes.error_handler.clone();
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
        error_handler,
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

#[cfg(test)]
mod tests {
    use super::{DEFAULT_BODY_SIZE_LIMIT, ServerConfig};
    use mlua::{FromLua, Lua, Value};

    fn config_from_lua(lua_src: &str) -> ServerConfig {
        let lua = Lua::new();
        let value: Value = lua.load(lua_src).eval().expect("lua eval");
        ServerConfig::from_lua(value, &lua).expect("server config")
    }

    #[test]
    fn should_parse_positive_body_size_limit() {
        let config = config_from_lua("{ body_size_limit = 1024 }");
        assert_eq!(config.body_size_limit, Some(1024));
    }

    #[test]
    fn should_use_secure_defaults() {
        let config = config_from_lua("{}");
        assert_eq!(config.strict_mode, true);
        assert_eq!(config.docs, false);
        assert_eq!(config.body_size_limit, Some(DEFAULT_BODY_SIZE_LIMIT));
    }

    #[test]
    fn should_reject_public_bind_in_strict_mode() {
        let lua = Lua::new();
        let value: Value = lua.load("{ host = '0.0.0.0' }").eval().expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(err.to_string().contains("allow_public_bind = true"));
    }

    #[test]
    fn should_allow_public_bind_with_explicit_opt_out() {
        let config = config_from_lua("{ host = '0.0.0.0', allow_public_bind = true }");
        assert_eq!(config.host, "0.0.0.0");
    }

    #[test]
    fn should_reject_unbounded_body_in_strict_mode() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ body_size_limit = 0 }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(err.to_string().contains("allow_unbounded_body = true"));
    }

    #[test]
    fn should_allow_unbounded_body_with_explicit_opt_out() {
        let config = config_from_lua("{ body_size_limit = 0, allow_unbounded_body = true }");
        assert_eq!(config.body_size_limit, None);
    }

    #[test]
    fn should_reject_wildcard_cors_with_credentials_in_strict_mode() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ cors_origin = '*', cors_credentials = true }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(
            err.to_string()
                .contains("allow_wildcard_cors_credentials = true")
        );
    }

    #[test]
    fn should_allow_wildcard_cors_with_credentials_with_explicit_opt_out() {
        let config = config_from_lua(
            "{ cors_origin = '*', cors_credentials = true, allow_wildcard_cors_credentials = true }",
        );
        assert_eq!(config.cors_origin.as_deref(), Some("*"));
        assert!(config.cors_credentials);
    }
}
