mod buffer_pool;
pub mod compression;
mod connection;
pub mod direct_json_parser;
mod event_loop;
pub mod fast_router;
mod http_server;
pub mod http_task;
pub mod lifecycle;
pub mod load_shedder;
pub mod multipart;
pub mod rate_limiter;
mod response;
pub mod session;
pub mod static_file;
pub mod store;
pub mod table_pool;
pub mod tls_reload;
pub use tls_reload::TlsCertReloader;
pub mod to_json;
pub mod ws_frame;
pub mod ws_handshake;
pub mod ws_lua;
pub mod ws_manager;

pub use fast_router::{FastRouter, RouteMatch};
pub use http_task::{CoroutineResponse, HttpResponse};
pub use lifecycle::{LifecycleConfig, LifecycleEvent, LifecycleManager, LifecyclePhase};
pub use load_shedder::{LoadShedConfig, LoadShedder, RequestGuard};
pub use rate_limiter::{RateLimitConfig, RateLimitPolicy, ScopedRateLimit};
pub use response::{
    RoverResponse, SseResponse, SseWriter, StreamingResponse, generate_sse_event_id,
    write_chunk_header, write_final_chunk,
};
pub use static_file::serve_static_file;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use anyhow::anyhow;
use rover_types::{Permission, PermissionMode, PermissionsConfig};
use std::collections::HashSet;
use std::str::FromStr;

use mlua::{
    FromLua, Function, Lua, RegistryKey,
    Value::{self},
};
use std::sync::Arc;
use tracing::info;

use crate::compression::CompressionAlgorithm;

pub type Bytes = bytes::Bytes;
const DEFAULT_BODY_SIZE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsConfig {
    pub cert_file: String,
    pub key_file: String,
    pub reload_interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub algorithms: Vec<CompressionAlgorithm>,
    pub min_size: usize,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedProxyCidr {
    pub network: IpAddr,
    pub prefix_len: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedProxyRange {
    pub start: IpAddr,
    pub end: IpAddr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedProxy {
    Cidr(TrustedProxyCidr),
    Range(TrustedProxyRange),
}

impl TrustedProxy {
    pub fn contains_ip(&self, source_ip: IpAddr) -> bool {
        match self {
            TrustedProxy::Cidr(cidr) => match (cidr.network, source_ip) {
                (IpAddr::V4(network), IpAddr::V4(source)) => {
                    let prefix_len = cidr.prefix_len as u32;
                    let mask = if prefix_len == 0 {
                        0
                    } else {
                        u32::MAX << (32 - prefix_len)
                    };
                    (u32::from(network) & mask) == (u32::from(source) & mask)
                }
                (IpAddr::V6(network), IpAddr::V6(source)) => {
                    let prefix_len = cidr.prefix_len as u32;
                    let mask = if prefix_len == 0 {
                        0
                    } else {
                        u128::MAX << (128 - prefix_len)
                    };
                    (u128::from(network) & mask) == (u128::from(source) & mask)
                }
                _ => false,
            },
            TrustedProxy::Range(range) => match (range.start, range.end, source_ip) {
                (IpAddr::V4(start), IpAddr::V4(end), IpAddr::V4(source)) => {
                    let source = u32::from(source);
                    source >= u32::from(start) && source <= u32::from(end)
                }
                (IpAddr::V6(start), IpAddr::V6(end), IpAddr::V6(source)) => {
                    let source = u128::from(source);
                    source >= u128::from(start) && source <= u128::from(end)
                }
                _ => false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessConfig {
    /// Dependency readiness map. false means dependency is unavailable.
    pub dependencies: HashMap<String, bool>,
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            dependencies: HashMap::new(),
        }
    }
}

impl ReadinessConfig {
    pub fn failed_dependencies(&self) -> Vec<String> {
        let mut failed = self
            .dependencies
            .iter()
            .filter_map(|(name, is_ready)| if *is_ready { None } else { Some(name.clone()) })
            .collect::<Vec<_>>();
        failed.sort();
        failed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    Healthy,
    Degraded,
    DependencyFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessProbeResult {
    pub state: ReadinessState,
    pub status_code: u16,
    pub body: Bytes,
}

/// Compute readiness probe result based on lifecycle phase and dependencies.
///
/// # Status Codes and Response Bodies
///
/// - **200**: Ready to accept connections with all dependencies healthy
///   - Body: `{"status":"ready"}`
///
/// - **503**: Not accepting connections (draining/shutting down)
///   - Body: `{"status":"not_ready"}`
///   - State: `ReadinessState::Degraded`
///
/// - **503**: Dependencies unavailable
///   - Body: `{"status":"not_ready","reasons":[{"code":"dependency_unavailable","dependency":"<name>"}]}`
///   - State: `ReadinessState::DependencyFailure`
///
/// # Examples
///
/// ```
/// use rover_server::{LifecyclePhase, ReadinessState, readiness_probe_result};
///
/// // Healthy state
/// let result = readiness_probe_result(LifecyclePhase::Running, &[]);
/// assert_eq!(result.state, ReadinessState::Healthy);
/// assert_eq!(result.status_code, 200);
///
/// // Draining state
/// let result = readiness_probe_result(LifecyclePhase::Draining, &[]);
/// assert_eq!(result.state, ReadinessState::Degraded);
/// assert_eq!(result.status_code, 503);
///
/// // Dependency failure
/// let deps = vec!["database".to_string()];
/// let result = readiness_probe_result(LifecyclePhase::Running, &deps);
/// assert_eq!(result.state, ReadinessState::DependencyFailure);
/// assert_eq!(result.status_code, 503);
/// ```
pub fn readiness_probe_result(
    phase: LifecyclePhase,
    failed_dependencies: &[String],
) -> ReadinessProbeResult {
    if !phase.can_accept_connections() {
        return ReadinessProbeResult {
            state: ReadinessState::Degraded,
            status_code: 503,
            body: Bytes::from_static(b"{\"status\":\"not_ready\"}"),
        };
    }

    if !failed_dependencies.is_empty() {
        let reasons = failed_dependencies
            .iter()
            .map(|dependency| {
                serde_json::json!({
                    "code": "dependency_unavailable",
                    "dependency": dependency,
                })
            })
            .collect::<Vec<_>>();

        let body = serde_json::json!({
            "status": "not_ready",
            "reasons": reasons,
        });

        return ReadinessProbeResult {
            state: ReadinessState::DependencyFailure,
            status_code: 503,
            body: Bytes::from(
                serde_json::to_vec(&body)
                    .unwrap_or_else(|_| b"{\"status\":\"not_ready\"}".to_vec()),
            ),
        };
    }

    ReadinessProbeResult {
        state: ReadinessState::Healthy,
        status_code: 200,
        body: Bytes::from_static(b"{\"status\":\"ready\"}"),
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithms: vec![CompressionAlgorithm::Gzip, CompressionAlgorithm::Deflate],
            min_size: 1024,
            types: vec![],
        }
    }
}

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
    pub security_headers: bool,
    pub https_redirect: bool,
    pub strict_mode: bool,
    pub allow_public_bind: bool,
    pub allow_insecure_http: bool,
    pub allow_wildcard_cors_credentials: bool,
    pub allow_unbounded_body: bool,
    pub allow_insecure_security_header_overrides: bool,
    pub management_prefix: String,
    pub management_token: Option<String>,
    pub allow_unauthenticated_management: bool,
    pub trusted_proxies: Vec<TrustedProxy>,
    pub tls: Option<TlsConfig>,
    pub compress: CompressionConfig,
    pub rate_limit: RateLimitConfig,
    pub load_shed: LoadShedConfig,
    pub readiness: ReadinessConfig,
    /// Graceful shutdown drain timeout in seconds
    pub drain_timeout_secs: Option<u64>,
    /// Permissions configuration
    pub permissions: PermissionsConfig,
}

impl ServerConfig {
    pub fn management_docs_path(&self) -> String {
        format!("{}/docs", self.management_prefix)
    }

    pub fn is_trusted_proxy_source(&self, source_ip: IpAddr) -> bool {
        self.trusted_proxies
            .iter()
            .any(|trusted_proxy| trusted_proxy.contains_ip(source_ip))
    }

    fn parse_management_prefix(config: &mlua::Table) -> mlua::Result<String> {
        let value = config.get::<Value>("management_prefix")?;
        let prefix = match value {
            Value::Nil => "/_rover".to_string(),
            Value::String(s) => s.to_str()?.trim().to_string(),
            _ => Err(anyhow!("management_prefix should be a string"))?,
        };

        if !prefix.starts_with('/') {
            return Err(anyhow!("management_prefix must start with '/'"))?;
        }

        if prefix == "/" {
            return Err(anyhow!(
                "management_prefix cannot be '/'; use an isolated namespace like '/_rover'"
            ))?;
        }

        Ok(prefix.trim_end_matches('/').to_string())
    }

    fn parse_tls_config(config: &mlua::Table) -> mlua::Result<Option<TlsConfig>> {
        let tls_value = config.get::<Value>("tls")?;
        let tls_table = match tls_value {
            Value::Nil => return Ok(None),
            Value::Table(table) => table,
            _ => Err(anyhow!("tls should be a table"))?,
        };

        let cert_file = match tls_table.get::<Value>("cert_file")? {
            Value::String(s) => s.to_str()?.trim().to_string(),
            Value::Nil => Err(anyhow!("tls.cert_file is required when tls is set"))?,
            _ => Err(anyhow!("tls.cert_file should be a string"))?,
        };

        let key_file = match tls_table.get::<Value>("key_file")? {
            Value::String(s) => s.to_str()?.trim().to_string(),
            Value::Nil => Err(anyhow!("tls.key_file is required when tls is set"))?,
            _ => Err(anyhow!("tls.key_file should be a string"))?,
        };

        if cert_file.is_empty() {
            Err(anyhow!("tls.cert_file cannot be empty"))?;
        }

        if key_file.is_empty() {
            Err(anyhow!("tls.key_file cannot be empty"))?;
        }

        let reload_interval_secs = match tls_table.get::<Value>("reload_interval_secs")? {
            Value::Nil => 1,
            Value::Integer(n) if n > 0 => n as u64,
            Value::Number(n) if n > 0.0 => n as u64,
            Value::Integer(_) | Value::Number(_) => {
                Err(anyhow!("tls.reload_interval_secs should be > 0"))?
            }
            _ => Err(anyhow!("tls.reload_interval_secs should be a number"))?,
        };

        Ok(Some(TlsConfig {
            cert_file,
            key_file,
            reload_interval_secs,
        }))
    }

    fn parse_ip(raw: &str, field_name: &str) -> mlua::Result<IpAddr> {
        raw.parse::<IpAddr>()
            .map_err(|_| anyhow!("{} should be a valid IP address", field_name).into())
    }

    fn parse_trusted_proxy_cidr(raw: &str) -> mlua::Result<TrustedProxy> {
        let (network_raw, prefix_raw) = raw
            .split_once('/')
            .ok_or_else(|| anyhow!("trusted_proxies CIDR should be '<ip>/<prefix>'"))?;
        let network = Self::parse_ip(network_raw.trim(), "trusted_proxies CIDR network")?;
        let prefix_len = prefix_raw
            .trim()
            .parse::<u16>()
            .map_err(|_| anyhow!("trusted_proxies CIDR prefix should be a number"))?;

        let max_prefix = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix_len > max_prefix as u16 {
            Err(anyhow!(
                "trusted_proxies CIDR prefix {} is out of range for {}",
                prefix_len,
                match network {
                    IpAddr::V4(_) => "IPv4",
                    IpAddr::V6(_) => "IPv6",
                }
            ))?
        }

        Ok(TrustedProxy::Cidr(TrustedProxyCidr {
            network,
            prefix_len: prefix_len as u8,
        }))
    }

    fn parse_trusted_proxy_range(start_raw: &str, end_raw: &str) -> mlua::Result<TrustedProxy> {
        let start = Self::parse_ip(start_raw.trim(), "trusted_proxies range start")?;
        let end = Self::parse_ip(end_raw.trim(), "trusted_proxies range end")?;

        match (start, end) {
            (IpAddr::V4(start_v4), IpAddr::V4(end_v4)) => {
                if u32::from(start_v4) > u32::from(end_v4) {
                    Err(anyhow!(
                        "trusted_proxies range start must be <= end for IPv4"
                    ))?
                }

                Ok(TrustedProxy::Range(TrustedProxyRange {
                    start: IpAddr::V4(start_v4),
                    end: IpAddr::V4(end_v4),
                }))
            }
            (IpAddr::V6(start_v6), IpAddr::V6(end_v6)) => {
                if u128::from(start_v6) > u128::from(end_v6) {
                    Err(anyhow!(
                        "trusted_proxies range start must be <= end for IPv6"
                    ))?
                }

                Ok(TrustedProxy::Range(TrustedProxyRange {
                    start: IpAddr::V6(start_v6),
                    end: IpAddr::V6(end_v6),
                }))
            }
            _ => Err(anyhow!(
                "trusted_proxies range start and end must use the same IP family"
            ))?,
        }
    }

    fn parse_trusted_proxy_item(value: Value) -> mlua::Result<TrustedProxy> {
        match value {
            Value::String(value) => {
                let raw = value.to_str()?.trim().to_string();
                if raw.is_empty() {
                    Err(anyhow!("trusted_proxies entries cannot be empty strings"))?
                }

                if raw.contains('/') {
                    return Self::parse_trusted_proxy_cidr(&raw);
                }

                if let Some((start_raw, end_raw)) = raw.split_once('-') {
                    return Self::parse_trusted_proxy_range(start_raw, end_raw);
                }

                Err(anyhow!(
                    "trusted_proxies string entries should be CIDR ('10.0.0.0/8') or range ('10.0.0.1-10.0.0.20')"
                ))?
            }
            Value::Table(table) => {
                let cidr = table.get::<Value>("cidr")?;
                match cidr {
                    Value::String(cidr_value) => {
                        let raw = cidr_value.to_str()?.trim().to_string();
                        if raw.is_empty() {
                            Err(anyhow!("trusted_proxies[].cidr cannot be empty"))?
                        }
                        Self::parse_trusted_proxy_cidr(&raw)
                    }
                    Value::Nil => {
                        let start = match table.get::<Value>("start")? {
                            Value::String(v) => v.to_str()?.trim().to_string(),
                            Value::Nil => Err(anyhow!(
                                "trusted_proxies[] should include either 'cidr' or both 'start' and 'end'"
                            ))?,
                            _ => Err(anyhow!("trusted_proxies[].start should be a string"))?,
                        };

                        let end = match table.get::<Value>("end")? {
                            Value::String(v) => v.to_str()?.trim().to_string(),
                            Value::Nil => match table.get::<Value>("to")? {
                                Value::String(v) => v.to_str()?.trim().to_string(),
                                Value::Nil => Err(anyhow!(
                                    "trusted_proxies[] range end is required (use 'to' or ['end'])"
                                ))?,
                                _ => Err(anyhow!("trusted_proxies[].to should be a string"))?,
                            },
                            _ => Err(anyhow!("trusted_proxies[].end should be a string"))?,
                        };

                        if start.is_empty() || end.is_empty() {
                            Err(anyhow!("trusted_proxies[] range values cannot be empty"))?
                        }

                        Self::parse_trusted_proxy_range(&start, &end)
                    }
                    _ => Err(anyhow!("trusted_proxies[].cidr should be a string"))?,
                }
            }
            _ => Err(anyhow!(
                "trusted_proxies should contain string or table entries"
            ))?,
        }
    }

    fn parse_trusted_proxies(config: &mlua::Table) -> mlua::Result<Vec<TrustedProxy>> {
        let value = config.get::<Value>("trusted_proxies")?;
        let table = match value {
            Value::Nil => return Ok(Vec::new()),
            Value::Table(table) => table,
            _ => Err(anyhow!("trusted_proxies should be an array"))?,
        };

        let mut entries = Vec::new();
        for pair in table.sequence_values::<Value>() {
            let item = pair?;
            entries.push(Self::parse_trusted_proxy_item(item)?);
        }

        Ok(entries)
    }

    fn parse_rate_limit_policy(table: &mlua::Table) -> mlua::Result<RateLimitPolicy> {
        let requests_per_window = match table.get::<Value>("requests_per_window")? {
            Value::Nil => 1000,
            Value::Integer(n) if n > 0 => n as u32,
            Value::Number(n) if n > 0.0 => n as u32,
            _ => Err(anyhow!(
                "rate_limit.requests_per_window should be a positive number"
            ))?,
        };

        let window_secs = match table.get::<Value>("window_secs")? {
            Value::Nil => 60,
            Value::Integer(n) if n > 0 => n as u64,
            Value::Number(n) if n > 0.0 => n as u64,
            _ => Err(anyhow!(
                "rate_limit.window_secs should be a positive number"
            ))?,
        };

        let key_header = match table.get::<Value>("key_header")? {
            Value::Nil => None,
            Value::String(s) => Some(s.to_str()?.trim().to_string()),
            _ => Err(anyhow!("rate_limit.key_header should be a string"))?,
        };

        Ok(RateLimitPolicy {
            requests_per_window,
            window_secs,
            key_header,
        })
    }

    fn parse_compression_config(config: &mlua::Table) -> mlua::Result<CompressionConfig> {
        let compress_value = config.get::<Value>("compress")?;
        let table = match compress_value {
            Value::Nil => return Ok(CompressionConfig::default()),
            Value::Table(table) => table,
            _ => Err(anyhow!("compress should be a table"))?,
        };

        let enabled = match table.get::<Value>("enabled")? {
            Value::Nil => true,
            Value::Boolean(value) => value,
            _ => Err(anyhow!("compress.enabled should be a boolean"))?,
        };

        let min_size = match table.get::<Value>("min_size")? {
            Value::Nil => 1024,
            Value::Integer(value) if value >= 0 => value as usize,
            Value::Number(value) if value >= 0.0 => value as usize,
            Value::Integer(_) | Value::Number(_) => {
                Err(anyhow!("compress.min_size should be >= 0"))?
            }
            _ => Err(anyhow!("compress.min_size should be a number"))?,
        };

        let algorithms = match table.get::<Value>("algorithms")? {
            Value::Nil => CompressionConfig::default().algorithms,
            Value::Table(algorithms_table) => {
                let mut values = Vec::new();
                for pair in algorithms_table.sequence_values::<Value>() {
                    let value = pair?;
                    let algorithm = match value {
                        Value::String(name) => {
                            let name = name.to_str()?;
                            match name.to_ascii_lowercase().as_str() {
                                "gzip" | "x-gzip" => CompressionAlgorithm::Gzip,
                                "deflate" => CompressionAlgorithm::Deflate,
                                _ => Err(anyhow!(
                                    "compress.algorithms contains unsupported value '{}'; supported values: 'gzip' and 'deflate'",
                                    name
                                ))?,
                            }
                        }
                        _ => Err(anyhow!("compress.algorithms should be an array of strings"))?,
                    };
                    values.push(algorithm);
                }

                if values.is_empty() {
                    Err(anyhow!("compress.algorithms should not be empty"))?
                }

                values
            }
            _ => Err(anyhow!("compress.algorithms should be an array"))?,
        };

        let types = match table.get::<Value>("types")? {
            Value::Nil => Vec::new(),
            Value::Table(types_table) => {
                let mut values = Vec::new();
                for pair in types_table.sequence_values::<Value>() {
                    let value = pair?;
                    match value {
                        Value::String(content_type) => {
                            values.push(content_type.to_str()?.trim().to_string())
                        }
                        _ => Err(anyhow!("compress.types should be an array of strings"))?,
                    }
                }
                values
            }
            _ => Err(anyhow!("compress.types should be an array"))?,
        };

        Ok(CompressionConfig {
            enabled,
            algorithms,
            min_size,
            types,
        })
    }

    fn parse_rate_limit_config(config: &mlua::Table) -> mlua::Result<RateLimitConfig> {
        let rate_limit_value = config.get::<Value>("rate_limit")?;
        match rate_limit_value {
            Value::Nil => Ok(RateLimitConfig::default()),
            Value::Boolean(enabled) => Ok(RateLimitConfig {
                enabled,
                global: None,
                scoped: vec![],
            }),
            Value::Table(table) => {
                let enabled = match table.get::<Value>("enabled")? {
                    Value::Nil => true,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("rate_limit.enabled should be a boolean"))?,
                };

                let global = match table.get::<Value>("global")? {
                    Value::Nil => None,
                    Value::Table(t) => Some(Self::parse_rate_limit_policy(&t)?),
                    _ => Err(anyhow!("rate_limit.global should be a table"))?,
                };

                let scoped = match table.get::<Value>("scoped")? {
                    Value::Nil => vec![],
                    Value::Table(scoped_table) => {
                        let mut scoped = Vec::new();
                        for pair in scoped_table.pairs::<i64, mlua::Table>() {
                            let (_, entry) = pair?;
                            let path_pattern = match entry.get::<Value>("path_pattern")? {
                                Value::String(s) => s.to_str()?.trim().to_string(),
                                _ => Err(anyhow!("rate_limit.scoped[].path_pattern is required"))?,
                            };
                            let policy = Self::parse_rate_limit_policy(&entry)?;
                            scoped.push(ScopedRateLimit {
                                path_pattern,
                                policy,
                            });
                        }
                        scoped
                    }
                    _ => Err(anyhow!("rate_limit.scoped should be an array"))?,
                };

                Ok(RateLimitConfig {
                    enabled,
                    global,
                    scoped,
                })
            }
            _ => Err(anyhow!("rate_limit should be a boolean or table"))?,
        }
    }

    fn parse_load_shed_config(config: &mlua::Table) -> mlua::Result<LoadShedConfig> {
        let load_shed_value = config.get::<Value>("load_shed")?;
        match load_shed_value {
            Value::Nil => Ok(LoadShedConfig::default()),
            Value::Boolean(enabled) => {
                if enabled {
                    Ok(LoadShedConfig::default())
                } else {
                    Ok(LoadShedConfig {
                        max_inflight: None,
                        max_queue: None,
                    })
                }
            }
            Value::Table(table) => {
                let max_inflight = match table.get::<Value>("max_inflight")? {
                    Value::Nil => Some(10000),
                    Value::Integer(n) if n > 0 => Some(n as u64),
                    Value::Number(n) if n > 0.0 => Some(n as u64),
                    Value::Integer(_) | Value::Number(_) => {
                        Err(anyhow!("load_shed.max_inflight should be > 0"))?
                    }
                    _ => Err(anyhow!("load_shed.max_inflight should be a number"))?,
                };

                let max_queue = match table.get::<Value>("max_queue")? {
                    Value::Nil => Some(1000),
                    Value::Integer(n) if n > 0 => Some(n as u64),
                    Value::Number(n) if n > 0.0 => Some(n as u64),
                    Value::Integer(_) | Value::Number(_) => {
                        Err(anyhow!("load_shed.max_queue should be > 0"))?
                    }
                    _ => Err(anyhow!("load_shed.max_queue should be a number"))?,
                };

                Ok(LoadShedConfig {
                    max_inflight,
                    max_queue,
                })
            }
            _ => Err(anyhow!("load_shed should be a boolean or table"))?,
        }
    }

    fn parse_readiness_config(config: &mlua::Table) -> mlua::Result<ReadinessConfig> {
        let readiness_value = config.get::<Value>("readiness")?;
        let readiness_table = match readiness_value {
            Value::Nil => return Ok(ReadinessConfig::default()),
            Value::Table(table) => table,
            _ => Err(anyhow!("readiness should be a table"))?,
        };

        let dependencies = match readiness_table.get::<Value>("dependencies")? {
            Value::Nil => HashMap::new(),
            Value::Table(deps_table) => {
                let mut deps = HashMap::new();
                for pair in deps_table.pairs::<Value, Value>() {
                    let (key, value) = pair?;
                    let dep_name = match key {
                        Value::String(s) => s.to_str()?.trim().to_string(),
                        _ => Err(anyhow!("readiness.dependencies keys should be strings"))?,
                    };
                    if dep_name.is_empty() {
                        Err(anyhow!("readiness.dependencies keys cannot be empty"))?;
                    }

                    let is_ready = match value {
                        Value::Boolean(b) => b,
                        _ => Err(anyhow!(
                            "readiness.dependencies['{}'] should be a boolean",
                            dep_name
                        ))?,
                    };
                    deps.insert(dep_name, is_ready);
                }
                deps
            }
            _ => Err(anyhow!("readiness.dependencies should be a table"))?,
        };

        Ok(ReadinessConfig { dependencies })
    }

    fn parse_permissions_config(config: &mlua::Table) -> mlua::Result<PermissionsConfig> {
        let permissions_value = config.get::<Value>("permissions")?;
        match permissions_value {
            Value::Nil => Ok(PermissionsConfig::new()),
            Value::Table(table) => {
                let mode = match table.get::<Value>("mode")? {
                    Value::Nil => PermissionMode::Development,
                    Value::String(s) => {
                        let mode_str = s.to_str()?.to_lowercase();
                        match mode_str.as_str() {
                            "development" | "dev" => PermissionMode::Development,
                            "production" | "prod" => PermissionMode::Production,
                            _ => Err(anyhow!(
                                "permissions.mode must be 'development' or 'production', got '{}'",
                                mode_str
                            ))?,
                        }
                    }
                    _ => Err(anyhow!("permissions.mode should be a string"))?,
                };

                let mut allow: HashSet<Permission> = HashSet::new();
                let allow_table = table.get::<Value>("allow")?;
                if let Value::Table(allow_t) = allow_table {
                    for pair in allow_t.sequence_values::<Value>() {
                        let value = pair?;
                        if let Value::String(s) = value {
                            let perm_str = s.to_str()?;
                            match Permission::from_str(&perm_str) {
                                Ok(perm) => {
                                    allow.insert(perm);
                                }
                                Err(_) => Err(anyhow!(
                                    "permissions.allow contains invalid permission '{}'; valid values are: fs, net, env, process, ffi",
                                    perm_str
                                ))?,
                            }
                        } else {
                            Err(anyhow!("permissions.allow should be an array of strings"))?;
                        }
                    }
                }

                let mut deny: HashSet<Permission> = HashSet::new();
                let deny_table = table.get::<Value>("deny")?;
                if let Value::Table(deny_t) = deny_table {
                    for pair in deny_t.sequence_values::<Value>() {
                        let value = pair?;
                        if let Value::String(s) = value {
                            let perm_str = s.to_str()?;
                            match Permission::from_str(&perm_str) {
                                Ok(perm) => {
                                    deny.insert(perm);
                                }
                                Err(_) => Err(anyhow!(
                                    "permissions.deny contains invalid permission '{}'; valid values are: fs, net, env, process, ffi",
                                    perm_str
                                ))?,
                            }
                        } else {
                            Err(anyhow!("permissions.deny should be an array of strings"))?;
                        }
                    }
                }

                // Check for ambiguous permissions (same permission in both allow and deny)
                let ambiguous: Vec<_> = allow.intersection(&deny).collect();
                if !ambiguous.is_empty() {
                    let ambiguous_names: Vec<_> = ambiguous.iter().map(|p| p.as_str()).collect();
                    Err(anyhow!(
                        "permissions contains ambiguous permissions that appear in both allow and deny: {}",
                        ambiguous_names.join(", ")
                    ))?
                }

                Ok(PermissionsConfig { mode, allow, deny })
            }
            _ => Err(anyhow!("permissions should be a table"))?,
        }
    }

    fn startup_validation_errors(&self) -> Vec<String> {
        if !self.strict_mode {
            return Vec::new();
        }

        let mut errors = Vec::new();

        if self.host != "localhost" && self.host != "127.0.0.1" && !self.allow_public_bind {
            errors.push(format!(
                "strict_mode blocks host '{}'. Use localhost/127.0.0.1, or set allow_public_bind = true",
                self.host
            ));
        }

        if self.host != "localhost"
            && self.host != "127.0.0.1"
            && self.allow_public_bind
            && !self.https_redirect
            && !self.allow_insecure_http
        {
            errors.push(
                "strict_mode requires https_redirect=true for public bind. Set https_redirect = true, or set allow_insecure_http = true"
                    .to_string(),
            );
        }

        if self.body_size_limit.is_none() && !self.allow_unbounded_body {
            errors.push(
                "strict_mode requires body_size_limit. Set a positive limit, or set allow_unbounded_body = true"
                    .to_string(),
            );
        }

        if self.cors_credentials
            && matches!(self.cors_origin.as_deref(), Some("*"))
            && !self.allow_wildcard_cors_credentials
        {
            errors.push(
                "strict_mode blocks cors_origin='*' with cors_credentials=true. Set a specific origin, or set allow_wildcard_cors_credentials = true"
                    .to_string(),
            );
        }

        if !self.security_headers && !self.allow_insecure_security_header_overrides {
            errors.push(
                "strict_mode requires security_headers=true. Set security_headers = true, or set allow_insecure_security_header_overrides = true"
                    .to_string(),
            );
        }

        errors
    }

    fn validate_startup(&self) -> anyhow::Result<()> {
        let errors = self.startup_validation_errors();
        if errors.is_empty() {
            return Ok(());
        }

        Err(anyhow!(errors.join("\n")))
    }
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

                let allow_insecure_http = match config.get::<Value>("allow_insecure_http")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("allow_insecure_http should be a boolean"))?,
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

                let security_headers = match config.get::<Value>("security_headers")? {
                    Value::Nil => true,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("security_headers should be a boolean"))?,
                };

                let https_redirect = match config.get::<Value>("https_redirect")? {
                    Value::Nil => false,
                    Value::Boolean(b) => b,
                    _ => Err(anyhow!("https_redirect should be a boolean"))?,
                };

                let allow_insecure_security_header_overrides =
                    match config.get::<Value>("allow_insecure_security_header_overrides")? {
                        Value::Nil => false,
                        Value::Boolean(b) => b,
                        _ => Err(anyhow!(
                            "allow_insecure_security_header_overrides should be a boolean"
                        ))?,
                    };

                let management_prefix = Self::parse_management_prefix(&config)?;

                let management_token = match config.get::<Value>("management_token")? {
                    Value::Nil => None,
                    Value::String(s) => {
                        let token = s.to_str()?.trim().to_string();
                        if token.is_empty() {
                            Err(anyhow!("management_token cannot be empty"))?
                        }
                        Some(token)
                    }
                    _ => Err(anyhow!("management_token should be a string"))?,
                };

                let allow_unauthenticated_management =
                    match config.get::<Value>("allow_unauthenticated_management")? {
                        Value::Nil => false,
                        Value::Boolean(b) => b,
                        _ => Err(anyhow!(
                            "allow_unauthenticated_management should be a boolean"
                        ))?,
                    };

                let drain_timeout_secs = match config.get::<Value>("drain_timeout_secs")? {
                    Value::Nil => None,
                    Value::Integer(n) if n > 0 => Some(n as u64),
                    Value::Number(n) if n > 0.0 => Some(n as u64),
                    Value::Integer(_) | Value::Number(_) => {
                        Err(anyhow!("drain_timeout_secs should be > 0"))?
                    }
                    _ => Err(anyhow!("drain_timeout_secs should be a number"))?,
                };

                let parsed = ServerConfig {
                    port: config.get::<u16>("port").unwrap_or(4242),
                    host,
                    log_level,
                    docs,
                    body_size_limit,
                    cors_origin,
                    cors_methods,
                    cors_headers,
                    cors_credentials,
                    security_headers,
                    https_redirect,
                    strict_mode,
                    allow_public_bind,
                    allow_insecure_http,
                    allow_wildcard_cors_credentials,
                    allow_unbounded_body,
                    allow_insecure_security_header_overrides,
                    management_prefix,
                    management_token,
                    allow_unauthenticated_management,
                    trusted_proxies: Self::parse_trusted_proxies(&config)?,
                    tls: Self::parse_tls_config(&config)?,
                    compress: Self::parse_compression_config(&config)?,
                    rate_limit: Self::parse_rate_limit_config(&config)?,
                    load_shed: Self::parse_load_shed_config(&config)?,
                    readiness: Self::parse_readiness_config(&config)?,
                    drain_timeout_secs,
                    permissions: Self::parse_permissions_config(&config)?,
                };

                parsed.validate_startup().map_err(mlua::Error::external)?;
                Ok(parsed)
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
    if let Err(err) = config.validate_startup() {
        eprintln!("\n❌ Invalid server startup config:");
        for line in err.to_string().lines() {
            eprintln!("   - {}", line);
        }
        eprintln!();
        std::process::exit(1);
    }

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
            info!(
                "📚 API docs available at http://{}{}",
                addr,
                config.management_docs_path()
            );
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
            if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                && io_err.kind() == std::io::ErrorKind::AddrInUse
            {
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
            eprintln!("\n❌ Error starting server: {}\n", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompressionConfig, DEFAULT_BODY_SIZE_LIMIT, LoadShedConfig, RateLimitConfig,
        ReadinessConfig, ServerConfig, TrustedProxy, TrustedProxyCidr, TrustedProxyRange,
    };
    use crate::compression::CompressionAlgorithm;
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
        assert!(config.security_headers);
        assert!(!config.https_redirect);
        assert_eq!(config.management_prefix, "/_rover");
        assert!(config.management_token.is_none());
        assert!(!config.allow_insecure_http);
        assert!(!config.allow_unauthenticated_management);
        assert!(config.trusted_proxies.is_empty());
        assert!(config.tls.is_none());
        assert_eq!(config.compress, CompressionConfig::default());
        assert_eq!(config.readiness, ReadinessConfig::default());
    }

    #[test]
    fn should_parse_compression_config() {
        let config = config_from_lua(
            "{ compress = { enabled = false, algorithms = { 'deflate' }, min_size = 2048, types = { 'application/json', 'text/html' } } }",
        );

        assert!(!config.compress.enabled);
        assert_eq!(
            config.compress.algorithms,
            vec![CompressionAlgorithm::Deflate]
        );
        assert_eq!(config.compress.min_size, 2048);
        assert_eq!(
            config.compress.types,
            vec!["application/json".to_string(), "text/html".to_string()]
        );
    }

    #[test]
    fn should_reject_invalid_compression_config() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ compress = { algorithms = { 'brotli' } } }")
            .eval()
            .expect("lua eval");

        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(
            err.to_string()
                .contains("compress.algorithms contains unsupported value 'brotli'; supported values: 'gzip' and 'deflate'")
        );
    }

    #[test]
    fn should_parse_tls_config() {
        let config = config_from_lua(
            "{ tls = { cert_file = '/tmp/cert.pem', key_file = '/tmp/key.pem', reload_interval_secs = 5 } }",
        );
        let tls = config.tls.expect("tls config");
        assert_eq!(tls.cert_file, "/tmp/cert.pem");
        assert_eq!(tls.key_file, "/tmp/key.pem");
        assert_eq!(tls.reload_interval_secs, 5);
    }

    #[test]
    fn should_reject_incomplete_tls_config() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ tls = { cert_file = '/tmp/cert.pem' } }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(err.to_string().contains("tls.key_file is required"));
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
        let config = config_from_lua(
            "{ host = '0.0.0.0', allow_public_bind = true, https_redirect = true }",
        );
        assert_eq!(config.host, "0.0.0.0");
    }

    #[test]
    fn should_reject_public_bind_without_https_redirect_in_strict_mode() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ host = '0.0.0.0', allow_public_bind = true }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(err.to_string().contains("https_redirect = true"));
    }

    #[test]
    fn should_allow_public_bind_without_https_redirect_when_explicitly_opted_out() {
        let config = config_from_lua(
            "{ host = '0.0.0.0', allow_public_bind = true, allow_insecure_http = true }",
        );
        assert!(config.allow_insecure_http);
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
    fn should_reject_disabling_security_headers_in_strict_mode() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ security_headers = false }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(
            err.to_string()
                .contains("allow_insecure_security_header_overrides = true")
        );
    }

    #[test]
    fn should_allow_disabling_security_headers_with_explicit_opt_out() {
        let config = config_from_lua(
            "{ security_headers = false, allow_insecure_security_header_overrides = true }",
        );
        assert!(!config.security_headers);
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

    #[test]
    fn should_report_all_strict_mode_startup_violations() {
        let config = ServerConfig {
            port: 4242,
            host: "0.0.0.0".to_string(),
            log_level: "info".to_string(),
            docs: false,
            body_size_limit: None,
            cors_origin: Some("*".to_string()),
            cors_methods: "GET".to_string(),
            cors_headers: "Content-Type".to_string(),
            cors_credentials: true,
            security_headers: false,
            https_redirect: false,
            strict_mode: true,
            allow_public_bind: false,
            allow_insecure_http: false,
            allow_wildcard_cors_credentials: false,
            allow_unbounded_body: false,
            allow_insecure_security_header_overrides: false,
            management_prefix: "/_rover".to_string(),
            management_token: None,
            allow_unauthenticated_management: false,
            trusted_proxies: Vec::new(),
            tls: None,
            compress: CompressionConfig::default(),
            rate_limit: RateLimitConfig::default(),
            load_shed: LoadShedConfig::default(),
            readiness: ReadinessConfig::default(),
            drain_timeout_secs: None,
            permissions: rover_types::PermissionsConfig::new(),
        };

        let err = config.validate_startup().expect_err("must reject config");
        let text = err.to_string();
        assert!(text.contains("allow_public_bind = true"));
        assert!(text.contains("allow_unbounded_body = true"));
        assert!(text.contains("allow_wildcard_cors_credentials = true"));
        assert!(text.contains("allow_insecure_security_header_overrides = true"));
    }

    #[test]
    fn should_parse_management_config() {
        let config = config_from_lua(
            "{ management_prefix = '/ops', management_token = 'abc123', allow_unauthenticated_management = true }",
        );
        assert_eq!(config.management_prefix, "/ops");
        assert_eq!(config.management_docs_path(), "/ops/docs");
        assert_eq!(config.management_token.as_deref(), Some("abc123"));
        assert!(config.allow_unauthenticated_management);
    }

    #[test]
    fn should_parse_trusted_proxies_from_cidr_and_range() {
        let config = config_from_lua(
            "{ trusted_proxies = { '10.0.0.0/8', { start = '192.168.0.1', to = '192.168.0.10' }, { cidr = 'fd00::/8' } } }",
        );

        assert_eq!(
            config.trusted_proxies,
            vec![
                TrustedProxy::Cidr(TrustedProxyCidr {
                    network: "10.0.0.0".parse().unwrap(),
                    prefix_len: 8,
                }),
                TrustedProxy::Range(TrustedProxyRange {
                    start: "192.168.0.1".parse().unwrap(),
                    end: "192.168.0.10".parse().unwrap(),
                }),
                TrustedProxy::Cidr(TrustedProxyCidr {
                    network: "fd00::".parse().unwrap(),
                    prefix_len: 8,
                })
            ]
        );
    }

    #[test]
    fn should_reject_invalid_trusted_proxy_cidr() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ trusted_proxies = { '10.0.0.0/999' } }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject invalid cidr");
        assert!(
            err.to_string()
                .contains("trusted_proxies CIDR prefix 999 is out of range for IPv4")
        );
    }

    #[test]
    fn should_reject_invalid_trusted_proxy_range() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ trusted_proxies = { '10.0.0.20-10.0.0.10' } }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject invalid range");
        assert!(
            err.to_string()
                .contains("trusted_proxies range start must be <= end for IPv4")
        );
    }

    #[test]
    fn should_match_trusted_proxy_sources() {
        let config = config_from_lua(
            "{ trusted_proxies = { '10.0.0.0/8', { start = '192.168.0.10', to = '192.168.0.20' }, 'fd00::/8' } }",
        );

        assert!(config.is_trusted_proxy_source("10.1.2.3".parse().unwrap()));
        assert!(config.is_trusted_proxy_source("192.168.0.15".parse().unwrap()));
        assert!(config.is_trusted_proxy_source("fd00::1".parse().unwrap()));
        assert!(!config.is_trusted_proxy_source("127.0.0.1".parse().unwrap()));
        assert!(!config.is_trusted_proxy_source("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn should_reject_non_isolated_management_prefix() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ management_prefix = '/' }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject config");
        assert!(err.to_string().contains("management_prefix cannot be '/'"));
    }

    #[test]
    fn should_parse_global_rate_limit() {
        let config = config_from_lua(
            "{ rate_limit = { enabled = true, global = { requests_per_window = 100, window_secs = 60 } } }",
        );
        assert!(config.rate_limit.enabled);
        let global = config.rate_limit.global.expect("global policy");
        assert_eq!(global.requests_per_window, 100);
        assert_eq!(global.window_secs, 60);
        assert!(global.key_header.is_none());
    }

    #[test]
    fn should_parse_rate_limit_with_key_header() {
        let config = config_from_lua(
            "{ rate_limit = { global = { requests_per_window = 50, window_secs = 30, key_header = 'X-API-Key' } } }",
        );
        let global = config.rate_limit.global.expect("global policy");
        assert_eq!(global.requests_per_window, 50);
        assert_eq!(global.window_secs, 30);
        assert_eq!(global.key_header, Some("X-API-Key".to_string()));
    }

    #[test]
    fn should_parse_scoped_rate_limits() {
        let config = config_from_lua(
            "{ rate_limit = { enabled = true, scoped = { { path_pattern = '/api/*', requests_per_window = 10, window_secs = 1 } } } }",
        );
        assert!(config.rate_limit.enabled);
        assert_eq!(config.rate_limit.scoped.len(), 1);
        let scoped = &config.rate_limit.scoped[0];
        assert_eq!(scoped.path_pattern, "/api/*");
        assert_eq!(scoped.policy.requests_per_window, 10);
        assert_eq!(scoped.policy.window_secs, 1);
    }

    #[test]
    fn should_disable_rate_limit_by_default() {
        let config = config_from_lua("{}");
        assert!(!config.rate_limit.enabled);
        assert!(config.rate_limit.global.is_none());
        assert!(config.rate_limit.scoped.is_empty());
    }

    #[test]
    fn should_parse_rate_limit_enabled_flag() {
        let config = config_from_lua("{ rate_limit = true }");
        assert!(config.rate_limit.enabled);

        let config = config_from_lua("{ rate_limit = false }");
        assert!(!config.rate_limit.enabled);
    }

    #[test]
    fn should_use_default_load_shed_config() {
        let config = config_from_lua("{}");
        assert!(config.load_shed.max_inflight.is_some());
        assert!(config.load_shed.max_queue.is_some());
        assert_eq!(config.load_shed.max_inflight.unwrap(), 10000);
        assert_eq!(config.load_shed.max_queue.unwrap(), 1000);
    }

    #[test]
    fn should_parse_load_shed_config() {
        let config = config_from_lua("{ load_shed = { max_inflight = 500, max_queue = 100 } }");
        assert_eq!(config.load_shed.max_inflight, Some(500));
        assert_eq!(config.load_shed.max_queue, Some(100));
    }

    #[test]
    fn should_disable_load_shed_when_false() {
        let config = config_from_lua("{ load_shed = false }");
        assert!(config.load_shed.max_inflight.is_none());
        assert!(config.load_shed.max_queue.is_none());
    }

    #[test]
    fn should_enable_load_shed_when_true() {
        let config = config_from_lua("{ load_shed = true }");
        assert!(config.load_shed.max_inflight.is_some());
        assert!(config.load_shed.max_queue.is_some());
    }

    #[test]
    fn should_parse_drain_timeout_secs() {
        let config = config_from_lua("{ drain_timeout_secs = 60 }");
        assert_eq!(config.drain_timeout_secs, Some(60));
    }

    #[test]
    fn should_parse_readiness_dependencies() {
        let config = config_from_lua(
            "{ readiness = { dependencies = { database = true, redis = false } } }",
        );
        assert_eq!(config.readiness.dependencies.get("database"), Some(&true));
        assert_eq!(config.readiness.dependencies.get("redis"), Some(&false));
        assert_eq!(
            config.readiness.failed_dependencies(),
            vec!["redis".to_string()]
        );
    }

    #[test]
    fn should_reject_non_boolean_readiness_dependency_status() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ readiness = { dependencies = { database = 'down' } } }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject readiness config");
        assert!(
            err.to_string()
                .contains("readiness.dependencies['database'] should be a boolean")
        );
    }

    #[test]
    fn should_default_drain_timeout_to_none() {
        let config = config_from_lua("{}");
        assert!(config.drain_timeout_secs.is_none());
    }

    #[test]
    fn should_reject_zero_drain_timeout() {
        let lua = Lua::new();
        let value: Value = lua
            .load("{ drain_timeout_secs = 0 }")
            .eval()
            .expect("lua eval");
        let err = ServerConfig::from_lua(value, &lua).expect_err("must reject zero drain_timeout");
        assert!(err.to_string().contains("drain_timeout_secs should be > 0"));
    }
}
