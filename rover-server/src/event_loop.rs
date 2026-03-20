use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::mem;
use std::net::{IpAddr, SocketAddr};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use mlua::{Function, Lua, RegistryKey, Thread, ThreadStatus, Value};
use rover_ui::SharedSignalRuntime;
use rover_ui::coroutine::{CoroutineResult, run_coroutine_with_delay};
use rover_ui::scheduler::SharedScheduler;
use slab::Slab;
use tracing::{debug, info, warn};

use crate::buffer_pool::BufferPool;
use crate::compression::{CompressionAlgorithm, compress, negotiate_encoding};
use crate::connection::{Connection, ConnectionState};
use crate::fast_router::{FastRouter, RouteMatch};
use crate::http_task::{
    CoroutineResponse, RequestContextPool, ThreadPool, execute_handler_coroutine,
    extract_or_generate_request_id,
};
use crate::lifecycle::{LifecycleConfig, LifecycleEvent, LifecycleManager, LifecyclePhase};
use crate::table_pool::LuaTablePool;
use crate::to_json::ToJson;
use crate::ws_frame::{self, WsOpcode};
use crate::ws_handshake;
use crate::ws_lua::{SharedConnections, SharedWsManager};
use crate::ws_manager::WsManager;
use crate::{Bytes, HttpMethod, Route, ServerConfig, SseWriter, WsRoute, generate_sse_event_id};

const LISTENER: Token = Token(0);
const DEFAULT_COROUTINE_TIMEOUT_MS: u64 = 30000;
const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 30;
const HEALTHZ_OK_BODY: &[u8] = b"{\"status\":\"ok\"}";
const DEFAULT_SECURITY_HEADERS: [(&str, &str); 3] = [
    ("X-Content-Type-Options", "nosniff"),
    ("X-Frame-Options", "DENY"),
    ("Referrer-Policy", "strict-origin-when-cross-origin"),
];

fn hash_bytes(data: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownState {
    Running,
    Draining,
    Shutdown,
}

struct PendingSseChunk {
    chunk: Option<Bytes>,
    should_end: bool,
}

struct PendingCoroutine {
    thread: Thread,
    started_at: Instant,
    ctx_idx: usize,
}

#[derive(Debug, Clone)]
struct BuiltinProbeResponse {
    status: u16,
    body: Bytes,
    content_type: Option<&'static str>,
    headers: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::Bytes;
    use crate::compression::CompressionAlgorithm;
    use mlua::{Lua, Value};

    use super::{EventLoop, ServerConfig};

    #[test]
    fn should_accept_wildcard_accept_header() {
        assert!(EventLoop::accepts_content_type("*/*", "application/json"));
    }

    #[test]
    fn should_accept_exact_content_type() {
        assert!(EventLoop::accepts_content_type(
            "application/json",
            "application/json"
        ));
    }

    #[test]
    fn should_accept_major_wildcard() {
        assert!(EventLoop::accepts_content_type(
            "application/*",
            "application/json"
        ));
    }

    #[test]
    fn should_reject_non_matching_accept() {
        assert!(!EventLoop::accepts_content_type(
            "text/plain",
            "application/json"
        ));
    }

    fn base_config() -> ServerConfig {
        ServerConfig {
            port: 4242,
            host: "localhost".to_string(),
            log_level: "nope".to_string(),
            docs: false,
            body_size_limit: Some(1024),
            cors_origin: None,
            cors_methods: "GET".to_string(),
            cors_headers: "Content-Type".to_string(),
            cors_credentials: false,
            security_headers: true,
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
            compress: crate::CompressionConfig::default(),
            rate_limit: crate::RateLimitConfig::default(),
            load_shed: crate::LoadShedConfig::default(),
            readiness: crate::ReadinessConfig::default(),
            drain_timeout_secs: None,
        }
    }

    fn header_offsets_from_raw(raw: &[u8]) -> Vec<(usize, usize, usize, usize)> {
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while cursor + 1 < raw.len() {
            if raw[cursor] == b'\r' && raw[cursor + 1] == b'\n' {
                break;
            }

            let line_start = cursor;
            while cursor + 1 < raw.len() && !(raw[cursor] == b'\r' && raw[cursor + 1] == b'\n') {
                cursor += 1;
            }
            let line_end = cursor;
            let line = &raw[line_start..line_end];
            if let Some(colon) = line.iter().position(|b| *b == b':') {
                let name_len = colon;
                let value_start = if line.get(colon + 1) == Some(&b' ') {
                    line_start + colon + 2
                } else {
                    line_start + colon + 1
                };
                let value_len = line_end.saturating_sub(value_start);
                out.push((line_start, name_len, value_start, value_len));
            }

            cursor += 2;
        }
        out
    }

    #[test]
    fn should_apply_security_header_defaults() {
        let mut headers = HashMap::new();
        EventLoop::apply_default_security_headers(&base_config(), &mut headers);

        assert_eq!(
            headers.get("X-Content-Type-Options").map(String::as_str),
            Some("nosniff")
        );
        assert_eq!(
            headers.get("X-Frame-Options").map(String::as_str),
            Some("DENY")
        );
        assert_eq!(
            headers.get("Referrer-Policy").map(String::as_str),
            Some("strict-origin-when-cross-origin")
        );
    }

    #[test]
    fn should_keep_safe_security_header_overrides() {
        let mut headers = HashMap::new();
        headers.insert("x-frame-options".to_string(), "SAMEORIGIN".to_string());

        EventLoop::apply_default_security_headers(&base_config(), &mut headers);

        assert_eq!(
            headers.get("x-frame-options").map(String::as_str),
            Some("SAMEORIGIN")
        );
    }

    #[test]
    fn should_replace_unsafe_security_header_overrides_by_default() {
        let mut headers = HashMap::new();
        headers.insert("Referrer-Policy".to_string(), "unsafe-url".to_string());

        EventLoop::apply_default_security_headers(&base_config(), &mut headers);

        assert_eq!(
            headers.get("Referrer-Policy").map(String::as_str),
            Some("strict-origin-when-cross-origin")
        );
    }

    #[test]
    fn should_allow_unsafe_security_header_overrides_with_explicit_opt_out() {
        let mut headers = HashMap::new();
        headers.insert("Referrer-Policy".to_string(), "unsafe-url".to_string());

        let mut config = base_config();
        config.allow_insecure_security_header_overrides = true;
        EventLoop::apply_default_security_headers(&config, &mut headers);

        assert_eq!(
            headers.get("Referrer-Policy").map(String::as_str),
            Some("unsafe-url")
        );
    }

    #[test]
    fn should_parse_bearer_management_token() {
        let token = EventLoop::bearer_token("Bearer secret-token");
        assert_eq!(token, Some("secret-token"));
    }

    #[test]
    fn should_not_parse_invalid_bearer_management_token() {
        assert!(EventLoop::bearer_token("Basic abc").is_none());
        assert!(EventLoop::bearer_token("Bearer ").is_none());
    }

    #[test]
    fn should_detect_forwarded_header_names_case_insensitive() {
        assert!(EventLoop::is_forwarded_header_name("Forwarded"));
        assert!(EventLoop::is_forwarded_header_name("X-Forwarded-For"));
        assert!(EventLoop::is_forwarded_header_name("x-forwarded-proto"));
        assert!(!EventLoop::is_forwarded_header_name("Host"));
    }

    #[test]
    fn should_strip_forwarded_headers_when_proxy_is_untrusted() {
        let buf = b"forwarded: a\r\nx-forwarded-for: b\r\nhost: c\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> =
            vec![(0, 9, 11, 1), (14, 15, 31, 1), (34, 4, 40, 1)];

        let sanitized_untrusted = EventLoop::sanitize_header_offsets(buf, &headers, false);
        assert_eq!(sanitized_untrusted.len(), 1);
        assert_eq!(sanitized_untrusted[0], (34, 4, 40, 1));

        let sanitized_trusted = EventLoop::sanitize_header_offsets(buf, &headers, true);
        assert_eq!(sanitized_trusted.len(), 3);
    }

    #[test]
    fn should_derive_client_context_from_forwarded_when_trusted() {
        let raw = b"forwarded: for=203.0.113.10;proto=https\r\nhost: example.com\r\n\r\n";
        let headers = header_offsets_from_raw(raw);

        let (client_ip, client_proto) = EventLoop::derive_client_context(
            raw,
            &headers,
            Some("10.0.0.8".parse().unwrap()),
            true,
        );

        assert_eq!(client_ip, "203.0.113.10");
        assert_eq!(client_proto, "https");
    }

    #[test]
    fn should_derive_client_context_from_x_forwarded_headers_when_trusted() {
        let raw = b"x-forwarded-for: 198.51.100.9, 198.51.100.10\r\nx-forwarded-proto: HTTPS\r\nhost: example.com\r\n\r\n";
        let headers = header_offsets_from_raw(raw);

        let (client_ip, client_proto) = EventLoop::derive_client_context(
            raw,
            &headers,
            Some("10.0.0.8".parse().unwrap()),
            true,
        );

        assert_eq!(client_ip, "198.51.100.9");
        assert_eq!(client_proto, "https");
    }

    #[test]
    fn should_ignore_forwarded_headers_for_untrusted_source() {
        let raw = b"x-forwarded-for: 198.51.100.9\r\nx-forwarded-proto: https\r\nhost: example.com\r\n\r\n";
        let headers = header_offsets_from_raw(raw);

        let (client_ip, client_proto) = EventLoop::derive_client_context(
            raw,
            &headers,
            Some("10.0.0.8".parse().unwrap()),
            false,
        );

        assert_eq!(client_ip, "10.0.0.8");
        assert_eq!(client_proto, "http");
    }

    #[test]
    fn should_serve_healthz_probe() {
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Get,
            "/healthz",
            crate::LifecyclePhase::Running,
            &[],
        )
        .expect("healthz should be recognized");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"{\"status\":\"ok\"}"));
        assert_eq!(response.content_type, Some("application/json"));
    }

    #[test]
    fn should_serve_readyz_probe_when_running() {
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Get,
            "/readyz",
            crate::LifecyclePhase::Running,
            &[],
        )
        .expect("readyz should be recognized");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"{\"status\":\"ready\"}"));
        assert_eq!(response.content_type, Some("application/json"));
    }

    #[test]
    fn should_allow_head_for_probes() {
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Head,
            "/healthz",
            crate::LifecyclePhase::Running,
            &[],
        )
        .expect("healthz should be recognized");

        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("application/json"));
    }

    #[test]
    fn should_mark_readyz_not_ready_while_draining() {
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Get,
            "/readyz",
            crate::LifecyclePhase::Draining,
            &[],
        )
        .expect("readyz should be recognized");

        assert_eq!(response.status, 503);
        assert_eq!(
            response.body,
            Bytes::from_static(b"{\"status\":\"not_ready\"}")
        );
    }

    #[test]
    fn should_reject_non_get_methods_for_probes() {
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Post,
            "/healthz",
            crate::LifecyclePhase::Running,
            &[],
        )
        .expect("healthz should be recognized");

        assert_eq!(response.status, 405);
        assert_eq!(
            response.headers.get("Allow").map(String::as_str),
            Some("GET, HEAD")
        );
    }

    #[test]
    fn should_mark_readyz_not_ready_when_dependency_fails() {
        let failed_dependencies = vec!["database".to_string()];
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Get,
            "/readyz",
            crate::LifecyclePhase::Running,
            &failed_dependencies,
        )
        .expect("readyz should be recognized");

        assert_eq!(response.status, 503);
        let body = std::str::from_utf8(response.body.as_ref()).expect("utf8 body");
        assert!(body.contains("\"status\":\"not_ready\""));
        assert!(body.contains("\"code\":\"dependency_unavailable\""));
        assert!(body.contains("\"dependency\":\"database\""));
    }

    #[test]
    fn should_keep_healthz_ok_even_when_dependency_fails() {
        let failed_dependencies = vec!["database".to_string()];
        let response = EventLoop::builtin_probe_response(
            crate::HttpMethod::Get,
            "/healthz",
            crate::LifecyclePhase::Draining,
            &failed_dependencies,
        )
        .expect("healthz should be recognized");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"{\"status\":\"ok\"}"));
    }

    #[test]
    fn should_require_management_auth_by_default() {
        let config = base_config();
        assert!(!EventLoop::is_management_request_authorized(
            &config,
            None,
            Some("anything")
        ));
    }

    #[test]
    fn should_authorize_management_with_configured_token() {
        let mut config = base_config();
        config.management_token = Some("secret-token".to_string());

        assert!(EventLoop::is_management_request_authorized(
            &config,
            Some("Bearer secret-token"),
            None
        ));

        assert!(EventLoop::is_management_request_authorized(
            &config,
            None,
            Some("secret-token")
        ));
    }

    #[test]
    fn should_allow_unauthenticated_management_when_opted_out() {
        let mut config = base_config();
        config.allow_unauthenticated_management = true;
        assert!(EventLoop::is_management_request_authorized(
            &config, None, None
        ));
    }

    #[test]
    fn should_add_vary_header_when_missing() {
        let mut headers = HashMap::new();
        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");
        assert_eq!(headers.get("Vary"), Some(&"Accept-Encoding".to_string()));
    }

    #[test]
    fn should_append_to_existing_vary_header() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin".to_string());
        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");
        assert_eq!(
            headers.get("Vary"),
            Some(&"Origin, Accept-Encoding".to_string())
        );
    }

    #[test]
    fn should_not_duplicate_vary_header_value() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Accept-Encoding, Origin".to_string());
        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");
        assert_eq!(
            headers.get("Vary"),
            Some(&"Accept-Encoding, Origin".to_string())
        );
    }

    #[test]
    fn should_handle_existing_vary_header_case_insensitively() {
        let mut headers = HashMap::new();
        headers.insert("vary".to_string(), "Origin".to_string());

        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");

        assert_eq!(headers.len(), 1);
        assert_eq!(
            headers.get("vary"),
            Some(&"Origin, Accept-Encoding".to_string())
        );
        assert!(!headers.contains_key("Vary"));
    }

    #[test]
    fn should_not_duplicate_vary_values_case_insensitively() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "accept-encoding, Origin".to_string());

        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");

        assert_eq!(
            headers.get("Vary"),
            Some(&"accept-encoding, Origin".to_string())
        );
    }

    #[test]
    fn should_detect_existing_content_encoding_header_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("content-encoding".to_string(), "gzip".to_string());
        assert!(EventLoop::has_content_encoding(&headers));

        let mut headers = HashMap::new();
        headers.insert("Content-Encoding".to_string(), "deflate".to_string());
        assert!(EventLoop::has_content_encoding(&headers));
    }

    #[test]
    fn should_not_detect_content_encoding_when_header_is_absent() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        assert!(!EventLoop::has_content_encoding(&headers));
    }

    #[test]
    fn should_detect_compressible_content_types() {
        assert!(EventLoop::is_compressible(Some("text/html")));
        assert!(EventLoop::is_compressible(Some("application/json")));
        assert!(EventLoop::is_compressible(Some("application/javascript")));
        assert!(EventLoop::is_compressible(Some("application/xml")));
        assert!(EventLoop::is_compressible(Some("application/vnd.api+json")));
        assert!(EventLoop::is_compressible(Some("application/atom+xml")));
        assert!(!EventLoop::is_compressible(Some("text/event-stream")));
        assert!(!EventLoop::is_compressible(Some("image/png")));
        assert!(!EventLoop::is_compressible(Some("video/mp4")));
        assert!(!EventLoop::is_compressible(None));
    }

    #[test]
    fn should_compose_vary_header_preserving_original_key() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin".to_string());
        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");
        assert_eq!(
            headers.get("Vary"),
            Some(&"Origin, Accept-Encoding".to_string())
        );
    }

    #[test]
    fn should_detect_different_content_type_with_charset() {
        assert!(EventLoop::is_compressible(Some(
            "application/json;charset=utf-8"
        )));
        assert!(EventLoop::is_compressible(Some("text/html; charset=UTF-8")));
        assert!(!EventLoop::is_compressible(Some(
            "text/event-stream; charset=UTF-8"
        )));
    }

    #[test]
    fn should_reject_image_content_types() {
        assert!(!EventLoop::is_compressible(Some("image/png")));
        assert!(!EventLoop::is_compressible(Some("image/jpeg")));
        assert!(!EventLoop::is_compressible(Some("image/gif")));
        assert!(!EventLoop::is_compressible(Some("image/webp")));
    }

    #[test]
    fn should_reject_audio_video_content_types() {
        assert!(!EventLoop::is_compressible(Some("audio/mp3")));
        assert!(!EventLoop::is_compressible(Some("video/mp4")));
        assert!(!EventLoop::is_compressible(Some("video/webm")));
    }

    #[test]
    fn should_security_headers_preserve_values_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("x-frame-options".to_string(), "SAMEORIGIN".to_string());
        headers.insert("X-Content-Type-OPTIONS".to_string(), "nosniff".to_string());

        EventLoop::apply_default_security_headers(&base_config(), &mut headers);

        assert_eq!(
            headers.get("x-frame-options"),
            Some(&"SAMEORIGIN".to_string())
        );
    }

    #[test]
    fn should_default_security_values_with_config_disabled() {
        let mut config = base_config();
        config.security_headers = false;
        let mut headers = HashMap::new();

        EventLoop::apply_default_security_headers(&config, &mut headers);

        assert!(!headers.contains_key("X-Content-Type-Options"));
        assert!(!headers.contains_key("X-Frame-Options"));
    }

    #[test]
    fn should_vary_append_multiple_values() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Accept".to_string());
        EventLoop::add_vary_header(&mut headers, "Accept-Encoding");
        EventLoop::add_vary_header(&mut headers, "Authorization");

        let vary = headers.get("Vary").unwrap();
        assert!(vary.contains("Accept"));
        assert!(vary.contains("Accept-Encoding"));
        assert!(vary.contains("Authorization"));
    }

    #[test]
    fn should_parse_accept_encoding_from_header_buffer() {
        let buf = b"accept-encoding: gzip, deflate\r\nhost: example.com\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> = vec![(0, 15, 17, 13)];
        let result = EventLoop::negotiate_encoding_from_headers(
            buf,
            &headers,
            &[CompressionAlgorithm::Gzip, CompressionAlgorithm::Deflate],
        );
        assert_eq!(result, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_encoding_negotiation_missing_header() {
        let buf = b"host: example.com\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> = vec![(0, 4, 6, 11)];
        let result = EventLoop::negotiate_encoding_from_headers(
            buf,
            &headers,
            &[CompressionAlgorithm::Gzip, CompressionAlgorithm::Deflate],
        );
        assert_eq!(result, None);
    }

    #[test]
    fn should_negotiate_with_configured_algorithm_order() {
        let buf = b"accept-encoding: gzip;q=0.8, deflate;q=0.8\r\nhost: example.com\r\n\r\n";
        let headers: Vec<(usize, usize, usize, usize)> = vec![(0, 15, 17, 29)];
        let result = EventLoop::negotiate_encoding_from_headers(
            buf,
            &headers,
            &[CompressionAlgorithm::Deflate, CompressionAlgorithm::Gzip],
        );

        assert_eq!(result, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_accept_all_media_types() {
        assert!(EventLoop::accepts_content_type("*/*", "anything/really"));
        assert!(EventLoop::accepts_content_type("*/*", "application/json"));
        assert!(EventLoop::accepts_content_type("*/*", "text/html"));
    }

    #[test]
    fn should_accept_application_json_suffix() {
        assert!(EventLoop::accepts_content_type(
            "application/json",
            "application/json"
        ));
        assert!(EventLoop::accepts_content_type(
            "application/vnd.api+json",
            "application/vnd.api+json"
        ));
        assert!(!EventLoop::accepts_content_type(
            "text/plain",
            "application/vnd.api+json"
        ));
    }

    #[test]
    fn should_encode_sse_table_payload() {
        let lua = Lua::new();
        let payload = lua
            .load(
                r#"
                return {
                  event = "token",
                  id = "evt-42",
                  retry = 2500,
                  data = { ok = true, value = 7 },
                }
                "#,
            )
            .eval::<Value>()
            .expect("create SSE payload");

        let mut frame = Vec::new();
        EventLoop::write_sse_payload(&mut frame, payload).expect("encode SSE frame");

        let text = String::from_utf8(frame).expect("utf8 frame");
        assert!(text.contains("retry:2500\n\n"));
        assert!(text.contains("id:evt-42\n"));
        assert!(text.contains("event:token\n"));
        assert!(text.contains("data:{"));
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains("\"value\":7"));
        assert!(text.ends_with("\n\n"));
    }

    #[test]
    fn should_encode_sse_string_payload_with_generated_id() {
        let lua = Lua::new();
        let mut frame = Vec::new();
        EventLoop::write_sse_payload(
            &mut frame,
            Value::String(lua.create_string("hello").expect("create string")),
        )
        .expect("encode string SSE frame");

        let text = String::from_utf8(frame).expect("utf8 frame");
        assert!(text.starts_with("id:"));
        assert!(text.contains("\ndata:hello\n\n"));
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;

    #[test]
    fn should_shutdown_state_have_correct_ordering() {
        assert!((ShutdownState::Running as u8) < ShutdownState::Draining as u8);
        assert!((ShutdownState::Draining as u8) < ShutdownState::Shutdown as u8);
    }

    #[test]
    fn should_shutdown_state_be_copy() {
        let state = ShutdownState::Running;
        let copy = state;
        assert_eq!(state, copy);
    }

    #[test]
    fn should_shutdown_state_be_eq() {
        assert_eq!(ShutdownState::Running, ShutdownState::Running);
        assert_ne!(ShutdownState::Running, ShutdownState::Draining);
        assert_ne!(ShutdownState::Draining, ShutdownState::Shutdown);
    }

    #[test]
    fn should_shutdown_state_debug_include_state_name() {
        assert!(format!("{:?}", ShutdownState::Running).contains("Running"));
        assert!(format!("{:?}", ShutdownState::Draining).contains("Draining"));
        assert!(format!("{:?}", ShutdownState::Shutdown).contains("Shutdown"));
    }
}

#[cfg(test)]
mod hash_tests {
    use super::*;

    #[test]
    fn should_hash_bytes_deterministically() {
        let data1 = b"hello world";
        let data2 = b"hello world";
        assert_eq!(hash_bytes(data1), hash_bytes(data2));
    }

    #[test]
    fn should_hash_different_bytes_differently() {
        let data1 = b"hello world";
        let data2 = b"hello universe";
        assert_ne!(hash_bytes(data1), hash_bytes(data2));
    }

    #[test]
    fn should_hash_empty_bytes() {
        let hash = hash_bytes(b"");
        assert_ne!(hash, 0);
    }
}

pub struct EventLoop {
    poll: Poll,
    listener: TcpListener,
    connections: Rc<RefCell<Slab<Connection>>>,
    lua: Lua,
    router: FastRouter,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    yielded_coroutines: HashMap<usize, PendingCoroutine>,
    thread_pool: ThreadPool,
    request_pool: RequestContextPool,
    table_pool: LuaTablePool,
    buffer_pool: BufferPool,
    ws_manager: SharedWsManager,
    /// Optional error handler function for custom error formatting
    error_handler: Option<Arc<RegistryKey>>,
    /// Current shutdown state for graceful drain
    shutdown_state: ShutdownState,
    /// When draining started (for timeout)
    drain_started: Option<Instant>,
    /// Drain timeout duration
    drain_timeout: Duration,
    /// Lifecycle manager for server phases and hooks
    lifecycle_manager: LifecycleManager,
}

impl EventLoop {
    fn bearer_token(value: &str) -> Option<&str> {
        let (scheme, token) = value.trim().split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return None;
        }
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        Some(token)
    }

    fn is_management_request_authorized(
        config: &ServerConfig,
        authorization_header: Option<&str>,
        management_token_header: Option<&str>,
    ) -> bool {
        if config.allow_unauthenticated_management {
            return true;
        }

        let expected = match config.management_token.as_deref() {
            Some(token) => token,
            None => return false,
        };

        if management_token_header
            .map(str::trim)
            .is_some_and(|provided| provided == expected)
        {
            return true;
        }

        authorization_header
            .and_then(Self::bearer_token)
            .is_some_and(|provided| provided == expected)
    }

    fn get_header_value(
        buf: &[u8],
        headers: &[(usize, usize, usize, usize)],
        name: &str,
    ) -> Option<String> {
        for &(name_off, name_len, val_off, val_len) in headers {
            let key = unsafe { std::str::from_utf8_unchecked(&buf[name_off..name_off + name_len]) };
            if key.eq_ignore_ascii_case(name) {
                let value =
                    unsafe { std::str::from_utf8_unchecked(&buf[val_off..val_off + val_len]) };
                return Some(value.to_string());
            }
        }
        None
    }

    fn negotiate_encoding_from_headers(
        buf: &[u8],
        headers: &[(usize, usize, usize, usize)],
        configured_algorithms: &[CompressionAlgorithm],
    ) -> Option<CompressionAlgorithm> {
        Self::get_header_value(buf, headers, "accept-encoding")
            .and_then(|ae| negotiate_encoding(&ae, configured_algorithms))
    }

    fn is_forwarded_header_name(name: &str) -> bool {
        name.eq_ignore_ascii_case("forwarded")
            || name
                .get(..12)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("x-forwarded-"))
    }

    fn sanitize_header_offsets(
        buf: &[u8],
        headers: &[(usize, usize, usize, usize)],
        should_trust_forwarded: bool,
    ) -> Vec<(u16, u8, u16, u16)> {
        let mut sanitized = Vec::with_capacity(headers.len());
        for &(name_off, name_len, val_off, val_len) in headers {
            let header_name =
                unsafe { std::str::from_utf8_unchecked(&buf[name_off..name_off + name_len]) };
            if !should_trust_forwarded && Self::is_forwarded_header_name(header_name) {
                continue;
            }

            sanitized.push((
                name_off as u16,
                name_len as u8,
                val_off as u16,
                val_len as u16,
            ));
        }

        sanitized
    }

    fn parse_client_ip_token(value: &str) -> Option<String> {
        let trimmed = value.trim().trim_matches('"');
        if trimmed.is_empty() {
            return None;
        }

        if let Some(ipv6) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']').map(|(host, _)| host))
            && ipv6.parse::<IpAddr>().is_ok()
        {
            return Some(ipv6.to_string());
        }

        if trimmed.parse::<IpAddr>().is_ok() {
            return Some(trimmed.to_string());
        }

        if let Some((host, port)) = trimmed.rsplit_once(':')
            && !host.is_empty()
            && port.chars().all(|ch| ch.is_ascii_digit())
            && host.parse::<IpAddr>().is_ok()
        {
            return Some(host.to_string());
        }

        None
    }

    fn parse_client_proto_token(value: &str) -> Option<String> {
        let proto = value.trim().trim_matches('"').to_ascii_lowercase();
        match proto.as_str() {
            "http" | "https" => Some(proto),
            _ => None,
        }
    }

    fn parse_forwarded_client_ip(value: &str) -> Option<String> {
        let first_entry = value.split(',').next()?.trim();
        for pair in first_entry.split(';') {
            let (name, raw_value) = pair.split_once('=')?;
            if name.trim().eq_ignore_ascii_case("for") {
                return Self::parse_client_ip_token(raw_value);
            }
        }
        None
    }

    fn parse_forwarded_client_proto(value: &str) -> Option<String> {
        let first_entry = value.split(',').next()?.trim();
        for pair in first_entry.split(';') {
            let (name, raw_value) = pair.split_once('=')?;
            if name.trim().eq_ignore_ascii_case("proto") {
                return Self::parse_client_proto_token(raw_value);
            }
        }
        None
    }

    fn derive_client_context(
        buf: &[u8],
        headers: &[(usize, usize, usize, usize)],
        source_ip: Option<IpAddr>,
        should_trust_forwarded: bool,
    ) -> (String, String) {
        let fallback_ip = source_ip.map(|ip| ip.to_string()).unwrap_or_default();
        let mut client_ip = String::new();
        let mut client_proto = "http".to_string();

        if should_trust_forwarded {
            if let Some(forwarded) = Self::get_header_value(buf, headers, "forwarded") {
                if let Some(parsed_ip) = Self::parse_forwarded_client_ip(&forwarded) {
                    client_ip = parsed_ip;
                }
                if let Some(parsed_proto) = Self::parse_forwarded_client_proto(&forwarded) {
                    client_proto = parsed_proto;
                }
            }

            if client_ip.is_empty()
                && let Some(value) = Self::get_header_value(buf, headers, "x-forwarded-for")
                && let Some(first) = value.split(',').next()
                && let Some(parsed_ip) = Self::parse_client_ip_token(first)
            {
                client_ip = parsed_ip;
            }

            if client_proto == "http"
                && let Some(value) = Self::get_header_value(buf, headers, "x-forwarded-proto")
                && let Some(first) = value.split(',').next()
                && let Some(parsed_proto) = Self::parse_client_proto_token(first)
            {
                client_proto = parsed_proto;
            }
        }

        if client_ip.is_empty() {
            client_ip = fallback_ip;
        }

        (client_ip, client_proto)
    }

    fn accepts_content_type(accept: &str, content_type: &str) -> bool {
        let ct = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        for raw in accept.split(',') {
            let media = raw
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();

            if media.is_empty() || media == "*/*" {
                return true;
            }
            if media == ct {
                return true;
            }
            if let Some((major, _)) = media.split_once('/')
                && media.ends_with("/*")
                && ct.starts_with(&format!("{}/", major))
            {
                return true;
            }
        }

        false
    }

    fn builtin_probe_response(
        method: HttpMethod,
        path: &str,
        phase: LifecyclePhase,
        failed_dependencies: &[String],
    ) -> Option<BuiltinProbeResponse> {
        if path != "/healthz" && path != "/readyz" {
            return None;
        }

        if !matches!(method, HttpMethod::Get | HttpMethod::Head) {
            let mut headers = HashMap::new();
            headers.insert("Allow".to_string(), "GET, HEAD".to_string());
            return Some(BuiltinProbeResponse {
                status: 405,
                body: Bytes::from_static(b"Method Not Allowed"),
                content_type: Some("text/plain"),
                headers,
            });
        }

        let (status, body) = match path {
            "/healthz" => (200, Bytes::from_static(HEALTHZ_OK_BODY)),
            "/readyz" => {
                let readiness = crate::readiness_probe_result(phase, failed_dependencies);
                (readiness.status_code, readiness.body)
            }
            _ => return None,
        };

        Some(BuiltinProbeResponse {
            status,
            body,
            content_type: Some("application/json"),
            headers: HashMap::new(),
        })
    }

    fn find_header_key_ci<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
        headers
            .keys()
            .find(|k| k.eq_ignore_ascii_case(name))
            .map(String::as_str)
    }

    fn has_content_encoding(headers: &HashMap<String, String>) -> bool {
        Self::find_header_key_ci(headers, "Content-Encoding").is_some()
    }

    fn is_safe_security_header_override(name: &str, value: &str) -> bool {
        let value = value.trim().to_ascii_lowercase();
        match name.to_ascii_lowercase().as_str() {
            "x-content-type-options" => value == "nosniff",
            "x-frame-options" => value == "deny" || value == "sameorigin",
            "referrer-policy" => matches!(
                value.as_str(),
                "no-referrer" | "same-origin" | "strict-origin" | "strict-origin-when-cross-origin"
            ),
            _ => true,
        }
    }

    fn apply_default_security_headers(
        config: &ServerConfig,
        headers: &mut HashMap<String, String>,
    ) {
        if !config.security_headers {
            return;
        }

        for (name, default_value) in DEFAULT_SECURITY_HEADERS {
            if let Some(existing_key) = Self::find_header_key_ci(headers, name).map(str::to_string)
            {
                let existing_value = headers
                    .get(&existing_key)
                    .map(String::as_str)
                    .unwrap_or_default();

                if config.allow_insecure_security_header_overrides
                    || Self::is_safe_security_header_override(name, existing_value)
                {
                    continue;
                }

                headers.insert(existing_key, default_value.to_string());
                continue;
            }

            headers.insert(name.to_string(), default_value.to_string());
        }
    }

    fn set_http_response(
        &self,
        conn: &mut Connection,
        status: u16,
        body: Bytes,
        content_type: Option<&str>,
        mut headers: HashMap<String, String>,
        buf: Vec<u8>,
        compression: Option<CompressionAlgorithm>,
    ) {
        Self::apply_default_security_headers(&self.config, &mut headers);

        let is_compressible_type = Self::is_compressible(content_type);
        let already_encoded = Self::has_content_encoding(&headers);

        let (final_body, final_headers) = if let Some(algo) = compression {
            const MIN_COMPRESS_SIZE: usize = 1024;
            if !already_encoded && body.len() >= MIN_COMPRESS_SIZE && is_compressible_type {
                let compressed = compress(&body, algo);
                if compressed.len() < body.len() {
                    let mut h = headers;
                    h.insert("Content-Encoding".to_string(), algo.to_string());
                    Self::add_vary_header(&mut h, "Accept-Encoding");
                    if let Some(etag) = h.get("ETag") {
                        let encoding_suffix = format!("-{}", algo);
                        let new_etag = if etag.starts_with('"') && etag.ends_with('"') {
                            format!("\"{}{}\"", &etag[1..etag.len() - 1], encoding_suffix)
                        } else {
                            format!("\"{}{}\"", etag, encoding_suffix)
                        };
                        h.insert("ETag".to_string(), new_etag);
                    } else {
                        let computed_etag = format!("\"{}\"", hash_bytes(&compressed));
                        h.insert("ETag".to_string(), computed_etag);
                    }
                    (Bytes::from(compressed), h)
                } else {
                    let mut h = headers;
                    if is_compressible_type || already_encoded {
                        Self::add_vary_header(&mut h, "Accept-Encoding");
                    }
                    (body, h)
                }
            } else {
                let mut h = headers;
                if already_encoded {
                    Self::add_vary_header(&mut h, "Accept-Encoding");
                }
                (body, h)
            }
        } else {
            let mut h = headers;
            if is_compressible_type || already_encoded {
                Self::add_vary_header(&mut h, "Accept-Encoding");
            }
            (body, h)
        };

        if final_headers.is_empty() {
            conn.set_response_bytes_with_buf(status, final_body, content_type, buf);
            return;
        }

        conn.set_response_bytes_with_headers(
            status,
            final_body,
            content_type,
            Some(&final_headers),
            buf,
        );
    }

    fn is_compressible(content_type: Option<&str>) -> bool {
        let ct = match content_type {
            Some(t) => t,
            None => return false,
        };
        let lower = ct.to_ascii_lowercase();
        (lower.starts_with("text/") && !lower.starts_with("text/event-stream"))
            || lower.starts_with("application/json")
            || lower.starts_with("application/javascript")
            || lower.starts_with("application/xml")
            || lower.starts_with("application/atom+xml")
            || lower.starts_with("application/rss+xml")
            || lower.ends_with("+json")
            || lower.ends_with("+xml")
    }

    fn add_vary_header(headers: &mut HashMap<String, String>, value: &str) {
        if let Some(vary_key) = Self::find_header_key_ci(headers, "Vary").map(str::to_string) {
            let existing = headers.get(&vary_key).cloned().unwrap_or_default();
            let already_has_value = existing
                .split(',')
                .map(str::trim)
                .any(|current| current.eq_ignore_ascii_case(value));

            if !already_has_value {
                let new_value = if existing.trim().is_empty() {
                    value.to_string()
                } else {
                    format!("{}, {}", existing, value)
                };
                headers.insert(vary_key, new_value);
            }
        } else {
            headers.insert("Vary".to_string(), value.to_string());
        }
    }

    pub fn new(
        lua: Lua,
        routes: Vec<Route>,
        ws_routes: Vec<WsRoute>,
        config: ServerConfig,
        openapi_spec: Option<serde_json::Value>,
        addr: SocketAddr,
        error_handler: Option<Arc<RegistryKey>>,
    ) -> Result<Self> {
        let poll = Poll::new()?;
        let mut listener = TcpListener::bind(addr)?;

        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;

        let mut router = FastRouter::from_routes(routes)?;

        // Register WS endpoints
        let ws_manager = Rc::new(RefCell::new(WsManager::new()));
        let mut ws_patterns = Vec::new();

        for ws_route in ws_routes {
            let pattern = std::str::from_utf8(&ws_route.pattern)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in WS route pattern"))?
                .to_string();
            let endpoint_idx = ws_manager
                .borrow_mut()
                .register_endpoint(ws_route.endpoint_config);
            let is_static = ws_route.is_static;

            if config.log_level != "nope" {
                info!("  WS {} (endpoint #{})", pattern, endpoint_idx);
            }

            ws_patterns.push((pattern, endpoint_idx, is_static));
        }

        if !ws_patterns.is_empty() {
            router.add_ws_routes(ws_patterns)?;
        }

        // Set WsManager as Lua app_data so ws.send/ws.listen can access it
        lua.set_app_data(ws_manager.clone());

        // Shared connections for Lua send operations
        let connections: Rc<RefCell<Slab<Connection>>> =
            Rc::new(RefCell::new(Slab::with_capacity(1024)));
        lua.set_app_data::<SharedConnections>(connections.clone());

        let request_pool = RequestContextPool::new(&lua, 1024)?;
        let table_pool = LuaTablePool::new(1024);
        let buffer_pool = BufferPool::new();

        let drain_timeout = config
            .drain_timeout_secs
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(DEFAULT_DRAIN_TIMEOUT_SECS));

        let lifecycle_config = LifecycleConfig {
            enabled: true,
            hook_timeout_secs: 30,
            graceful_shutdown: true,
            drain_timeout_secs: drain_timeout.as_secs(),
            reload_on_signal: false,
        };
        let lifecycle_manager = LifecycleManager::with_config(lifecycle_config);

        Ok(Self {
            poll,
            listener,
            connections,
            lua,
            router,
            config,
            openapi_spec,
            yielded_coroutines: HashMap::with_capacity(1024),
            thread_pool: ThreadPool::new(2048),
            request_pool,
            table_pool,
            buffer_pool,
            ws_manager,
            error_handler,
            shutdown_state: ShutdownState::Running,
            drain_started: None,
            drain_timeout,
            lifecycle_manager,
        })
    }

    fn setup_signal_handler(_poll: &Poll) -> Result<crossbeam_channel::Receiver<()>> {
        let (tx, rx) = crossbeam_channel::bounded(1);

        #[cfg(unix)]
        {
            use signal_hook::consts::signal;
            use signal_hook::iterator::Signals;

            let mut signals = Signals::new([signal::SIGTERM, signal::SIGINT])?;
            let tx_clone = tx.clone();

            std::thread::spawn(move || {
                for sig in signals.forever() {
                    if sig == signal::SIGTERM || sig == signal::SIGINT {
                        let _ = tx_clone.try_send(());
                    }
                }
            });
        }

        #[cfg(not(unix))]
        {
            let _ = tx;
        }

        Ok(rx)
    }

    fn handle_signal(&mut self) -> Result<bool> {
        match self.shutdown_state {
            ShutdownState::Running => {
                if self.config.log_level != "nope" {
                    info!("Received shutdown signal, draining connections...");
                }
                self.lifecycle_manager.request_shutdown();
                self.lifecycle_manager
                    .execute_hooks(&self.lua, LifecycleEvent::ShutdownRequested)?;
                self.lifecycle_manager
                    .transition_to(LifecyclePhase::Draining);
                self.lifecycle_manager
                    .execute_hooks(&self.lua, LifecycleEvent::Draining)?;
                self.shutdown_state = ShutdownState::Draining;
                self.drain_started = Some(Instant::now());
                let _ = self.poll.registry().deregister(&mut self.listener);
                self.prepare_connections_for_shutdown();
                Ok(false)
            }
            ShutdownState::Draining => Ok(false),
            ShutdownState::Shutdown => Ok(true),
        }
    }

    fn prepare_connections_for_shutdown(&mut self) {
        let mut close_now = Vec::new();
        let mut flush_pending = Vec::new();

        {
            let mut conns = self.connections.borrow_mut();
            for (idx, conn) in conns.iter_mut() {
                let should_flush = matches!(
                    conn.state,
                    ConnectionState::Writing
                        | ConnectionState::StreamingHeaders
                        | ConnectionState::StreamingBody
                );

                if conn.prepare_for_shutdown() {
                    close_now.push(idx);
                } else if should_flush {
                    flush_pending.push(idx);
                }
            }
        }

        for idx in flush_pending {
            let mut conns = self.connections.borrow_mut();
            if let Some(conn) = conns.get_mut(idx) {
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
        }

        for idx in close_now {
            self.close_connection(idx);
        }
    }

    fn close_connection(&mut self, conn_idx: usize) {
        self.recycle_write_buf(conn_idx);

        let mut conns = self.connections.borrow_mut();
        if !conns.contains(conn_idx) {
            return;
        }

        let mut conn = conns.remove(conn_idx);
        let _ = self.poll.registry().deregister(&mut conn.socket);
    }

    fn is_drain_complete(&self) -> bool {
        let conns = self.connections.borrow();
        conns.is_empty() && self.yielded_coroutines.is_empty()
    }

    pub fn run(&mut self) -> Result<()> {
        let mut events = Events::with_capacity(1024);

        // Execute startup hooks
        self.lifecycle_manager
            .execute_hooks(&self.lua, LifecycleEvent::Startup)?;
        self.lifecycle_manager
            .transition_to(LifecyclePhase::Running);
        self.lifecycle_manager
            .execute_hooks(&self.lua, LifecycleEvent::Ready)?;

        let signal_rx = Self::setup_signal_handler(&self.poll)?;

        loop {
            let poll_timeout = match self.shutdown_state {
                ShutdownState::Running => {
                    self.next_poll_timeout().or(Some(Duration::from_millis(50)))
                }
                ShutdownState::Draining => {
                    if self.is_drain_complete() {
                        if self.config.log_level != "nope" {
                            info!("All connections drained, shutting down");
                        }
                        self.shutdown_state = ShutdownState::Shutdown;
                        self.lifecycle_manager
                            .transition_to(LifecyclePhase::ShuttingDown);
                    } else if let Some(started) = self.drain_started
                        && started.elapsed() >= self.drain_timeout
                    {
                        if self.config.log_level != "nope" {
                            let conns = self.connections.borrow();
                            let active_count = conns.len();
                            let yielded_count = self.yielded_coroutines.len();
                            info!(
                                "Drain timeout reached ({} connections, {} yielded coroutines remaining)",
                                active_count, yielded_count
                            );
                        }
                        self.shutdown_state = ShutdownState::Shutdown;
                        self.lifecycle_manager
                            .transition_to(LifecyclePhase::ShuttingDown);
                    }
                    Some(Duration::from_millis(50))
                }
                ShutdownState::Shutdown => {
                    if self.config.log_level != "nope" {
                        info!("Server shutdown complete");
                    }
                    // Execute shutdown hooks before returning
                    self.lifecycle_manager
                        .execute_hooks(&self.lua, LifecycleEvent::ShutdownComplete)?;
                    self.lifecycle_manager
                        .transition_to(LifecyclePhase::Shutdown);
                    return Ok(());
                }
            };

            self.poll.poll(&mut events, poll_timeout)?;

            for event in events.iter() {
                match event.token() {
                    LISTENER => {
                        if self.shutdown_state == ShutdownState::Running
                            && self
                                .lifecycle_manager
                                .current_phase()
                                .can_accept_connections()
                        {
                            self.accept_connections()?;
                        }
                    }
                    token => self.handle_connection(token, event)?,
                }
            }

            if self.shutdown_state == ShutdownState::Running
                && signal_rx.try_recv().is_ok()
                && self.handle_signal()?
            {
                return Ok(());
            }

            self.tick_lua_scheduler();

            self.check_timeouts()?;

            if !self.yielded_coroutines.is_empty() {
                self.resume_yielded_coroutines()?;
            }
        }
    }

    #[inline]
    fn next_poll_timeout(&self) -> Option<Duration> {
        let scheduler = match self.lua.app_data_ref::<SharedScheduler>() {
            Some(s) => s.clone(),
            None => return None,
        };

        let next_wake = scheduler.borrow().next_wake_time()?;
        let now = Instant::now();
        if next_wake <= now {
            Some(Duration::from_millis(0))
        } else {
            Some(next_wake.duration_since(now))
        }
    }

    fn sse_value_to_string(value: Value) -> mlua::Result<String> {
        match value {
            Value::Nil => Ok(String::new()),
            Value::String(s) => Ok(s.to_str()?.to_string()),
            Value::Integer(i) => Ok(i.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Boolean(b) => Ok(b.to_string()),
            Value::Table(table) => table.to_json_string(),
            other => Err(mlua::Error::RuntimeError(format!(
                "unsupported SSE data value: {:?}",
                other
            ))),
        }
    }

    fn write_sse_payload(frame: &mut Vec<u8>, value: Value) -> mlua::Result<()> {
        match value {
            Value::String(s) => {
                let id = generate_sse_event_id();
                let data = s.to_str()?;
                SseWriter::format_event(frame, None, &data, Some(&id));
                Ok(())
            }
            Value::Table(table) => {
                let event = match table.get::<Value>("event")? {
                    Value::Nil => None,
                    Value::String(name) => Some(name.to_str()?.to_string()),
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "SSE event name must be string, got {:?}",
                            other
                        )));
                    }
                };

                let id = match table.get::<Value>("id")? {
                    Value::Nil => Some(generate_sse_event_id()),
                    Value::String(id) => Some(id.to_str()?.to_string()),
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "SSE id must be string, got {:?}",
                            other
                        )));
                    }
                };

                if let Some(retry_ms) = table.get::<Option<u32>>("retry")? {
                    SseWriter::format_retry(frame, retry_ms);
                }

                if let Some(comment) = table.get::<Option<String>>("comment")? {
                    SseWriter::format_comment(frame, &comment);
                }

                let data = Self::sse_value_to_string(table.get::<Value>("data")?)?;
                SseWriter::format_event(frame, event.as_deref(), &data, id.as_deref());
                Ok(())
            }
            other => Err(mlua::Error::RuntimeError(format!(
                "SSE producer must return string, table, or nil, got {:?}",
                other
            ))),
        }
    }

    fn poll_sse_chunk(&self, conn_idx: usize) -> Result<Option<PendingSseChunk>> {
        let (event_producer, retry_pending, retry_ms) = {
            let conns = self.connections.borrow();
            let Some(conn) = conns.get(conn_idx) else {
                return Ok(None);
            };

            if conn.sse_data.is_none() || !matches!(conn.state, ConnectionState::StreamingBody) {
                return Ok(None);
            }

            if !conn.stream_chunks.is_empty() || conn.stream_final_sent {
                return Ok(None);
            }

            let sse = conn.sse_data.as_ref().unwrap();
            (
                Arc::clone(&sse.event_producer),
                sse.retry_pending,
                sse.retry_ms,
            )
        };

        let mut frame = Vec::with_capacity(256);
        if retry_pending && retry_ms > 0 {
            SseWriter::format_retry(&mut frame, retry_ms);
        }

        let producer: Function = self.lua.registry_value(&event_producer)?;
        let value = producer.call::<Value>(())?;
        let should_end = matches!(value, Value::Nil);

        if !should_end {
            Self::write_sse_payload(&mut frame, value)?;
        }

        Ok(Some(PendingSseChunk {
            chunk: (!frame.is_empty()).then(|| Bytes::from(frame)),
            should_end,
        }))
    }

    fn tick_lua_scheduler(&mut self) {
        let scheduler = match self.lua.app_data_ref::<SharedScheduler>() {
            Some(s) => s.clone(),
            None => return,
        };

        let runtime = match self.lua.app_data_ref::<SharedSignalRuntime>() {
            Some(r) => r.clone(),
            None => return,
        };

        let ready_ids = scheduler.borrow_mut().tick(Instant::now());
        if ready_ids.is_empty() {
            return;
        }

        let mut resumed_any = false;

        for id in ready_ids {
            if scheduler.borrow().is_cancelled(id) {
                continue;
            }

            let pending = match scheduler.borrow_mut().take_pending(id) {
                Ok(p) => p,
                Err(_) => continue,
            };

            resumed_any = true;

            match run_coroutine_with_delay(&self.lua, &runtime, &pending.thread, Value::Nil) {
                Ok(CoroutineResult::Completed) => {}
                Ok(CoroutineResult::YieldedDelay { delay_ms }) => {
                    scheduler
                        .borrow_mut()
                        .schedule_delay_with_id(id, pending.thread, delay_ms);
                }
                Ok(CoroutineResult::YieldedOther) => {}
                Err(e) => {
                    warn!("scheduled task error: {}", e);
                }
            }
        }

        if resumed_any {
            self.reregister_ws_writers();
        }
    }

    fn accept_connections(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut socket, _addr)) => {
                    let mut conns = self.connections.borrow_mut();
                    let entry = conns.vacant_entry();
                    let token = Token(entry.key() + 1);

                    self.poll
                        .registry()
                        .register(&mut socket, token, Interest::READABLE)?;

                    entry.insert(Connection::new(socket, token));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn handle_connection(&mut self, token: Token, event: &mio::event::Event) -> Result<()> {
        let conn_idx = token.0 - 1;

        if !self.connections.borrow().contains(conn_idx) {
            return Ok(());
        }

        // Check if this is a WebSocket connection
        {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx)
                && conn.is_websocket()
            {
                drop(conns);
                return self.handle_ws_event(conn_idx, event);
            }
        }

        let (should_process, should_close, should_reset, is_ws_upgrade_complete) = {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];

            match conn.state {
                ConnectionState::Reading if event.is_readable() => match conn.try_read() {
                    Ok(true) => (true, false, false, false),
                    Ok(false) => (false, false, false, false),
                    Err(_) => {
                        conn.state = ConnectionState::Closed;
                        (false, true, false, false)
                    }
                },
                ConnectionState::Writing if event.is_writable() => match conn.try_write() {
                    Ok(true) => {
                        // Check if this was a WS upgrade 101 response
                        if conn.pending_ws_upgrade.is_some() {
                            (false, false, false, true)
                        } else if conn.keep_alive {
                            (false, false, true, false)
                        } else {
                            conn.state = ConnectionState::Closed;
                            (false, true, false, false)
                        }
                    }
                    Ok(false) => (false, false, false, false),
                    Err(_) => {
                        conn.state = ConnectionState::Closed;
                        (false, true, false, false)
                    }
                },
                ConnectionState::StreamingHeaders if event.is_writable() => {
                    match conn.try_write_stream() {
                        Ok(true) => {
                            // Headers written, transition to streaming body
                            // The stream will call the producer to get chunks
                            (false, false, false, false)
                        }
                        Ok(false) => (false, false, false, false),
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            (false, true, false, false)
                        }
                    }
                }
                ConnectionState::StreamingBody if event.is_writable() => {
                    match conn.try_write_stream() {
                        Ok(true) => {
                            // Streaming complete
                            if conn.keep_alive {
                                conn.reset();
                                (false, false, true, false)
                            } else {
                                conn.state = ConnectionState::Closed;
                                (false, true, false, false)
                            }
                        }
                        Ok(false) => (false, false, false, false),
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            (false, true, false, false)
                        }
                    }
                }
                _ => (false, false, false, false),
            }
        };

        // Handle streaming - produce chunks after headers are written
        let streaming_conn_idx = {
            let conns = self.connections.borrow();
            if matches!(
                conns.get(conn_idx).map(|c| &c.state),
                Some(ConnectionState::StreamingBody)
            ) {
                Some(conn_idx)
            } else {
                None
            }
        };

        if let Some(idx) = streaming_conn_idx {
            // Call the chunk producer to get more chunks
            let chunks_to_write = {
                let conns = self.connections.borrow();
                let conn = &conns[idx];
                if let Some(ref producer_key) = conn.stream_producer {
                    let producer: mlua::Function = self.lua.registry_value(producer_key)?;
                    let mut chunks = Vec::new();

                    // Call producer in a loop until it returns nil or we hit a limit
                    // This produces chunks with backpressure-safe handling
                    loop {
                        let result = producer.call::<mlua::Value>(())?;
                        match result {
                            mlua::Value::String(s) => {
                                let bytes = s.as_bytes();
                                chunks.push(Bytes::copy_from_slice(&bytes));
                            }
                            mlua::Value::Nil => {
                                // End of stream
                                break;
                            }
                            _ => {
                                // Invalid return, treat as end
                                break;
                            }
                        }
                    }
                    Some(chunks)
                } else {
                    None
                }
            };

            if let Some(chunks) = chunks_to_write {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[idx];
                for chunk in chunks {
                    conn.queue_stream_chunk(chunk);
                }
            }

            // Queue final chunk if producer finished
            {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[idx];
                conn.queue_stream_end();
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
        }

        let sse_conn_idx = {
            let conns = self.connections.borrow();
            if matches!(
                conns.get(conn_idx).map(|c| &c.state),
                Some(ConnectionState::StreamingBody)
            ) && conns
                .get(conn_idx)
                .and_then(|c| c.sse_data.as_ref())
                .is_some()
            {
                Some(conn_idx)
            } else {
                None
            }
        };

        if let Some(idx) = sse_conn_idx {
            match self.poll_sse_chunk(idx) {
                Ok(Some(pending)) => {
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(idx) {
                        if let Some(ref mut sse) = conn.sse_data {
                            sse.retry_pending = false;
                            sse.last_write = Some(Instant::now());
                        }
                        if let Some(chunk) = pending.chunk {
                            conn.queue_stream_chunk(chunk);
                        }
                        if pending.should_end {
                            conn.queue_stream_end();
                            conn.sse_data = None;
                        }
                        let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("SSE producer error for connection {}: {}", idx, e);
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(idx) {
                        conn.queue_stream_end();
                        conn.sse_data = None;
                        let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    }
                }
            }
        }

        // Handle WS upgrade completion (101 fully written)
        if is_ws_upgrade_complete {
            return self.complete_ws_upgrade(conn_idx);
        }

        let is_closed_state = {
            let conns = self.connections.borrow();
            matches!(
                conns.get(conn_idx).map(|c| &c.state),
                Some(ConnectionState::Closed)
            )
        };

        if should_close || is_closed_state {
            self.close_connection(conn_idx);
            return Ok(());
        }

        if should_reset {
            self.recycle_write_buf(conn_idx);
            let mut conns = self.connections.borrow_mut();
            if let Some(conn) = conns.get_mut(conn_idx) {
                conn.reset();
                let _ = conn.reregister(self.poll.registry(), Interest::READABLE);
            }
        }

        if should_process {
            self.start_request_coroutine(conn_idx)?;
        }

        Ok(())
    }

    fn recycle_write_buf(&mut self, conn_idx: usize) {
        let mut conns = self.connections.borrow_mut();
        if let Some(conn) = conns.get_mut(conn_idx) {
            let buf = mem::take(&mut conn.write_buf);
            if !buf.is_empty() {
                self.buffer_pool.return_response_buf(buf);
            }
        }
    }

    // ── WebSocket upgrade ──

    fn start_request_coroutine(&mut self, conn_idx: usize) -> Result<()> {
        let started_at = Instant::now();

        let conns = self.connections.borrow();
        let conn = &conns[conn_idx];
        let method = conn.method_str().unwrap_or_default();
        let full_path = conn.path_str().unwrap_or_default();
        let (path, query_str) = if let Some(pos) = full_path.find('?') {
            (&full_path[..pos], Some(full_path[pos + 1..].to_string()))
        } else {
            (full_path, None)
        };

        #[allow(unused_variables)]
        let buf_ref: &[u8] = if !conn.parsed_buf.is_empty() {
            &conn.parsed_buf
        } else {
            &conn.read_buf
        };
        let (path_off, path_len) = conn.path_offset.unwrap_or((0, 0));
        let keep_alive = conn.keep_alive;
        let source_ip = conn.socket.peer_addr().ok().map(|addr| addr.ip());
        let should_trust_forwarded = source_ip
            .map(|ip| self.config.is_trusted_proxy_source(ip))
            .unwrap_or(false);

        // ── Check for WebSocket upgrade ──
        let has_upgrade =
            conn.header_offsets
                .iter()
                .any(|&(name_off, name_len, val_off, val_len)| {
                    let name = unsafe {
                        std::str::from_utf8_unchecked(&buf_ref[name_off..name_off + name_len])
                    };
                    let val = unsafe {
                        std::str::from_utf8_unchecked(&buf_ref[val_off..val_off + val_len])
                    };
                    name.eq_ignore_ascii_case("upgrade") && val.eq_ignore_ascii_case("websocket")
                });

        if has_upgrade {
            let path_owned = path.to_string();
            drop(conns);
            return self.handle_ws_upgrade(conn_idx, &path_owned, keep_alive);
        }

        // ── Regular HTTP path ──
        if self.config.docs
            && path == self.config.management_docs_path()
            && self.openapi_spec.is_some()
        {
            let authorization =
                Self::get_header_value(buf_ref, &conn.header_offsets, "authorization");
            let management_token =
                Self::get_header_value(buf_ref, &conn.header_offsets, "x-rover-management-token");
            let is_authorized = Self::is_management_request_authorized(
                &self.config,
                authorization.as_deref(),
                management_token.as_deref(),
            );

            if !is_authorized {
                drop(conns);
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                let body =
                    Bytes::from_static(b"{\"error\":\"Management endpoint requires auth token\"}");
                self.set_http_response(
                    conn,
                    401,
                    body,
                    Some("application/json"),
                    HashMap::new(),
                    buf,
                    None,
                );
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }

            let html = rover_openapi::scalar_html(self.openapi_spec.as_ref().unwrap());
            drop(conns);
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.keep_alive = keep_alive;
            let buf = self.buffer_pool.get_response_buf();
            self.set_http_response(
                conn,
                200,
                Bytes::copy_from_slice(html.as_bytes()),
                Some("text/html"),
                HashMap::new(),
                buf,
                None,
            );
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            return Ok(());
        }

        // CORS preflight handling before method parsing (OPTIONS may be auto-handled)
        if method.eq_ignore_ascii_case("options")
            && let (Some(cors_origin), Some(origin), Some(_acr_method)) = (
                self.config.cors_origin.as_ref(),
                Self::get_header_value(buf_ref, &conn.header_offsets, "origin"),
                Self::get_header_value(
                    buf_ref,
                    &conn.header_offsets,
                    "access-control-request-method",
                ),
            )
        {
            drop(conns);
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.keep_alive = keep_alive;
            let mut headers = HashMap::new();
            let allow_origin = if cors_origin == "*" {
                "*".to_string()
            } else {
                origin
            };
            headers.insert("Access-Control-Allow-Origin".to_string(), allow_origin);
            headers.insert(
                "Access-Control-Allow-Methods".to_string(),
                self.config.cors_methods.clone(),
            );
            headers.insert(
                "Access-Control-Allow-Headers".to_string(),
                self.config.cors_headers.clone(),
            );
            if self.config.cors_credentials {
                headers.insert(
                    "Access-Control-Allow-Credentials".to_string(),
                    "true".to_string(),
                );
            }
            let buf = self.buffer_pool.get_response_buf();
            self.set_http_response(
                conn,
                204,
                Bytes::new(),
                Some("text/plain"),
                headers,
                buf,
                None,
            );
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            return Ok(());
        }

        let http_method = match HttpMethod::from_str(method) {
            Some(m) => m,
            None => {
                let error_msg = format!(
                    "Invalid HTTP method '{}'. Valid methods: {}",
                    method,
                    HttpMethod::valid_methods().join(", ")
                );
                drop(conns);
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                self.set_http_response(
                    conn,
                    400,
                    Bytes::from(error_msg.into_bytes()),
                    Some("text/plain"),
                    HashMap::new(),
                    buf,
                    None,
                );
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        if let Some(probe_response) = Self::builtin_probe_response(
            http_method,
            path,
            self.lifecycle_manager.current_phase(),
            &self.config.readiness.failed_dependencies(),
        ) {
            drop(conns);
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.keep_alive = keep_alive;
            let body = if http_method == HttpMethod::Head {
                Bytes::new()
            } else {
                probe_response.body
            };
            let buf = self.buffer_pool.get_response_buf();
            self.set_http_response(
                conn,
                probe_response.status,
                body,
                probe_response.content_type,
                probe_response.headers,
                buf,
                None,
            );
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            return Ok(());
        }

        let path_owned = path.to_string();
        let (handler, params, is_head_request) =
            match self.router.match_route(http_method, &path_owned) {
                RouteMatch::Found {
                    handler,
                    params,
                    is_head,
                } => (handler, params, is_head),
                RouteMatch::MethodNotAllowed { allowed } => {
                    // OPTIONS auto-response when path exists
                    if http_method == HttpMethod::Options {
                        drop(conns);
                        let mut conns = self.connections.borrow_mut();
                        let conn = &mut conns[conn_idx];
                        conn.keep_alive = keep_alive;
                        let mut headers = HashMap::new();
                        let allow = allowed
                            .iter()
                            .map(|m| m.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        headers.insert("Allow".to_string(), allow);
                        let buf = self.buffer_pool.get_response_buf();
                        self.set_http_response(
                            conn,
                            204,
                            Bytes::new(),
                            Some("text/plain"),
                            headers,
                            buf,
                            None,
                        );
                        let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                        return Ok(());
                    }

                    drop(conns);
                    let mut conns = self.connections.borrow_mut();
                    let conn = &mut conns[conn_idx];
                    conn.keep_alive = keep_alive;
                    let mut headers = HashMap::new();
                    let allow = allowed
                        .iter()
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    headers.insert("Allow".to_string(), allow);
                    let buf = self.buffer_pool.get_response_buf();
                    self.set_http_response(
                        conn,
                        405,
                        Bytes::from_static(b"Method Not Allowed"),
                        Some("text/plain"),
                        headers,
                        buf,
                        None,
                    );
                    let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    return Ok(());
                }
                RouteMatch::NotFound => {
                    drop(conns);
                    let mut conns = self.connections.borrow_mut();
                    let conn = &mut conns[conn_idx];
                    conn.keep_alive = keep_alive;
                    let buf = self.buffer_pool.get_response_buf();
                    self.set_http_response(
                        conn,
                        404,
                        Bytes::from_static(b"Route not found"),
                        Some("text/plain"),
                        HashMap::new(),
                        buf,
                        None,
                    );
                    let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    return Ok(());
                }
            };

        let buf = if !conn.parsed_buf.is_empty() {
            conn.parsed_buf.clone()
        } else {
            conn.read_buf.clone().freeze()
        };

        let (method_off, method_len) = conn.method_offset.unwrap_or((0, 0));

        let query_offsets = if let Some(qs) = &query_str {
            let search_start = path_off;
            let search_end = (path_off + path_len).min(buf.len());
            if let Some(q_pos) = buf[search_start..search_end]
                .iter()
                .position(|&b| b == b'?')
            {
                let qs_start_abs = path_off + q_pos + 1;
                let mut offsets = Vec::new();
                let mut pos = 0usize;
                let qs_len = qs.len();

                while pos < qs_len {
                    let key_start = pos as u16;

                    while pos < qs_len && qs.as_bytes()[pos] != b'=' && qs.as_bytes()[pos] != b'&' {
                        pos += 1;
                    }

                    let key_len_raw = (pos - key_start as usize) as u8;

                    if pos >= qs_len || qs.as_bytes()[pos] == b'&' {
                        if key_len_raw > 0 {
                            offsets.push((
                                (qs_start_abs + key_start as usize) as u16,
                                key_len_raw,
                                (qs_start_abs + key_start as usize) as u16,
                                key_len_raw as u16,
                            ));
                        }
                        pos += 1;
                        continue;
                    }

                    pos += 1;
                    let val_start = pos as u16;

                    while pos < qs_len && qs.as_bytes()[pos] != b'&' {
                        pos += 1;
                    }

                    let val_len_raw = (pos - val_start as usize) as u16;
                    offsets.push((
                        (qs_start_abs + key_start as usize) as u16,
                        key_len_raw,
                        (qs_start_abs + val_start as usize) as u16,
                        val_len_raw,
                    ));

                    pos += 1;
                }
                offsets
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let header_offsets =
            Self::sanitize_header_offsets(buf_ref, &conn.header_offsets, should_trust_forwarded);
        let (client_ip, client_proto) = Self::derive_client_context(
            buf_ref,
            &conn.header_offsets,
            source_ip,
            should_trust_forwarded,
        );

        let accept_header = Self::get_header_value(buf_ref, &conn.header_offsets, "accept");
        let accept_encoding = Self::negotiate_encoding_from_headers(
            buf_ref,
            &conn.header_offsets,
            &self.config.compress.algorithms,
        );

        // Generate or extract request ID for correlation
        let request_id = extract_or_generate_request_id(buf_ref, &conn.header_offsets);

        let (body_off, body_len) = conn
            .body
            .map(|(off, len)| (off as u32, len as u32))
            .unwrap_or((0, 0));

        // Check body size limit if configured
        if let Some(max_size) = self.config.body_size_limit
            && (body_len as usize) > max_size
        {
            drop(conns);
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.keep_alive = keep_alive;
            let buf = self.buffer_pool.get_response_buf();
            let error_body = format!(
                "{{\"error\":\"Request body too large: {} bytes exceeds limit of {} bytes\"}}",
                body_len, max_size
            );
            conn.set_response_with_buf(413, error_body.as_bytes(), Some("application/json"), buf);
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            return Ok(());
        }

        // Drop the borrow before calling into Lua
        let origin_header = Self::get_header_value(buf_ref, &conn.header_offsets, "origin");
        drop(conns);

        match execute_handler_coroutine(
            &self.lua,
            &handler,
            buf,
            method_off as u16,
            method_len as u8,
            path_off as u16,
            path_len as u16,
            body_off,
            body_len,
            header_offsets,
            query_offsets,
            &params,
            request_id,
            client_ip,
            client_proto,
            started_at,
            &mut self.thread_pool,
            &mut self.request_pool,
            &self.table_pool,
            &mut self.buffer_pool,
            self.error_handler.as_ref(),
        ) {
            Ok(CoroutineResponse::Ready {
                status,
                body,
                content_type,
                headers,
            }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let body = if is_head_request { Bytes::new() } else { body };

                if let (Some(accept), Some(ct)) = (accept_header.as_deref(), content_type)
                    && status < 400
                    && !Self::accepts_content_type(accept, ct)
                {
                    let resp_buf = self.buffer_pool.get_response_buf();
                    self.set_http_response(
                        conn,
                        406,
                        Bytes::from_static(b"Not Acceptable"),
                        Some("text/plain"),
                        HashMap::new(),
                        resp_buf,
                        None,
                    );
                    let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    return Ok(());
                }

                let buf = self.buffer_pool.get_response_buf();
                let mut response_headers = headers.unwrap_or_default();
                if let (Some(cors_origin), Some(origin)) =
                    (self.config.cors_origin.as_ref(), origin_header.as_ref())
                {
                    let allow_origin = if cors_origin == "*" {
                        "*".to_string()
                    } else {
                        origin.clone()
                    };
                    response_headers
                        .insert("Access-Control-Allow-Origin".to_string(), allow_origin);
                    if self.config.cors_credentials {
                        response_headers.insert(
                            "Access-Control-Allow-Credentials".to_string(),
                            "true".to_string(),
                        );
                    }
                }
                self.set_http_response(
                    conn,
                    status,
                    body,
                    content_type,
                    response_headers,
                    buf,
                    accept_encoding,
                );
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
            Ok(CoroutineResponse::Streaming {
                status,
                content_type,
                headers,
                chunk_producer,
            }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;

                let mut response_headers = headers.unwrap_or_default();

                // CORS headers for streaming
                if let (Some(cors_origin), Some(origin)) =
                    (self.config.cors_origin.as_ref(), origin_header.as_ref())
                {
                    let allow_origin = if cors_origin == "*" {
                        "*".to_string()
                    } else {
                        origin.clone()
                    };
                    response_headers
                        .insert("Access-Control-Allow-Origin".to_string(), allow_origin);
                    if self.config.cors_credentials {
                        response_headers.insert(
                            "Access-Control-Allow-Credentials".to_string(),
                            "true".to_string(),
                        );
                    }
                }

                // Set up streaming response with chunked transfer encoding headers
                let buf = self.buffer_pool.get_response_buf();
                conn.set_streaming_headers(status, &content_type, Some(&response_headers), buf);
                conn.stream_producer = Some(Arc::clone(&chunk_producer));

                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
            Ok(CoroutineResponse::Sse {
                status,
                headers,
                event_producer,
                retry_ms,
            }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;

                let mut response_headers = headers.unwrap_or_default();
                response_headers
                    .entry("Cache-Control".to_string())
                    .or_insert_with(|| "no-cache".to_string());
                response_headers
                    .entry("X-Accel-Buffering".to_string())
                    .or_insert_with(|| "no".to_string());

                if let (Some(cors_origin), Some(origin)) =
                    (self.config.cors_origin.as_ref(), origin_header.as_ref())
                {
                    let allow_origin = if cors_origin == "*" {
                        "*".to_string()
                    } else {
                        origin.clone()
                    };
                    response_headers
                        .insert("Access-Control-Allow-Origin".to_string(), allow_origin);
                    if self.config.cors_credentials {
                        response_headers.insert(
                            "Access-Control-Allow-Credentials".to_string(),
                            "true".to_string(),
                        );
                    }
                }

                let buf = self.buffer_pool.get_response_buf();
                conn.set_streaming_headers(
                    status,
                    "text/event-stream",
                    Some(&response_headers),
                    buf,
                );
                conn.sse_data = Some(Box::new(crate::connection::SseConnectionData {
                    event_producer,
                    retry_ms: retry_ms.unwrap_or(0),
                    retry_pending: retry_ms.unwrap_or(0) > 0,
                    keepalive_ms: 0,
                    last_write: None,
                }));

                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
            Ok(CoroutineResponse::Yielded { thread, ctx_idx }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.thread = Some(thread.clone());
                self.yielded_coroutines.insert(
                    conn_idx,
                    PendingCoroutine {
                        thread,
                        started_at: Instant::now(),
                        ctx_idx,
                    },
                );
            }
            Err(_) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                self.set_http_response(
                    conn,
                    500,
                    Bytes::from_static(b"Internal server error"),
                    Some("text/plain"),
                    HashMap::new(),
                    buf,
                    None,
                );
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
            }
        }

        Ok(())
    }

    fn handle_ws_upgrade(&mut self, conn_idx: usize, path: &str, keep_alive: bool) -> Result<()> {
        // Match against WS router
        let (endpoint_idx, _params) = match self.router.match_ws_route(path) {
            Some((idx, p)) => (idx, p),
            None => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                self.set_http_response(
                    conn,
                    404,
                    Bytes::from_static(b"WebSocket route not found"),
                    Some("text/plain"),
                    HashMap::new(),
                    buf,
                    None,
                );
                let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        // Validate upgrade headers
        let accept_key = {
            let conns = self.connections.borrow();
            let conn = &conns[conn_idx];
            let buf: &[u8] = if !conn.parsed_buf.is_empty() {
                &conn.parsed_buf
            } else {
                &conn.read_buf
            };

            match ws_handshake::validate_upgrade_headers(buf, &conn.header_offsets) {
                Ok(key) => ws_handshake::compute_accept_key(key),
                Err(e) => {
                    drop(conns);
                    let mut conns = self.connections.borrow_mut();
                    let conn = &mut conns[conn_idx];
                    conn.keep_alive = false;
                    let buf = self.buffer_pool.get_response_buf();
                    self.set_http_response(
                        conn,
                        e.status_code(),
                        Bytes::copy_from_slice(e.message().as_bytes()),
                        Some("text/plain"),
                        HashMap::new(),
                        buf,
                        None,
                    );
                    let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                    return Ok(());
                }
            }
        };

        // Build 101 response
        let mut response_buf = self.buffer_pool.get_response_buf();
        ws_handshake::build_upgrade_response(&accept_key, &mut response_buf);

        // Write the 101 response
        {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.write_buf = response_buf;
            conn.write_pos = 0;
            conn.body_buf = Bytes::new();
            conn.body_pos = 0;
            conn.state = ConnectionState::Writing;
            conn.pending_ws_upgrade = Some(endpoint_idx);
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
        }

        info!(
            "WS upgrade initiated for conn {} -> endpoint #{}",
            conn_idx, endpoint_idx
        );

        Ok(())
    }

    fn complete_ws_upgrade(&mut self, conn_idx: usize) -> Result<()> {
        let endpoint_idx = {
            let conns = self.connections.borrow();
            match conns.get(conn_idx).and_then(|c| c.pending_ws_upgrade) {
                Some(idx) => idx,
                None => return Ok(()),
            }
        };

        // Collect info needed for join handler before upgrading
        let (
            buf,
            header_offsets,
            header_offsets_raw,
            query_offsets,
            params,
            path_off,
            path_len,
            method_off,
            method_len,
            source_ip,
            should_trust_forwarded,
        ) = {
            let conns = self.connections.borrow();
            let conn = &conns[conn_idx];
            let buf = if !conn.parsed_buf.is_empty() {
                conn.parsed_buf.clone()
            } else {
                conn.read_buf.clone().freeze()
            };
            let header_offsets: Vec<(u16, u8, u16, u16)> = conn
                .header_offsets
                .iter()
                .map(|&(no, nl, vo, vl)| (no as u16, nl as u8, vo as u16, vl as u16))
                .collect();
            let header_offsets_raw = conn.header_offsets.clone();
            let source_ip = conn.socket.peer_addr().ok().map(|addr| addr.ip());
            let should_trust_forwarded = source_ip
                .map(|ip| self.config.is_trusted_proxy_source(ip))
                .unwrap_or(false);
            let (po, pl) = conn.path_offset.unwrap_or((0, 0));
            let (mo, ml) = conn.method_offset.unwrap_or((0, 0));
            let query_offsets = {
                let search_start = po;
                let search_end = (po + pl).min(buf.len());
                if let Some(q_pos) = buf[search_start..search_end]
                    .iter()
                    .position(|&b| b == b'?')
                {
                    let qs_start_abs = po + q_pos + 1;
                    let qs_len = search_end.saturating_sub(qs_start_abs);
                    let mut offsets = Vec::new();
                    let mut pos = 0usize;

                    while pos < qs_len {
                        let key_start = pos as u16;

                        while pos < qs_len
                            && buf[qs_start_abs + pos] != b'='
                            && buf[qs_start_abs + pos] != b'&'
                        {
                            pos += 1;
                        }

                        let key_len_raw = (pos - key_start as usize) as u8;

                        if pos >= qs_len || buf[qs_start_abs + pos] == b'&' {
                            if key_len_raw > 0 {
                                offsets.push((
                                    (qs_start_abs + key_start as usize) as u16,
                                    key_len_raw,
                                    (qs_start_abs + key_start as usize) as u16,
                                    key_len_raw as u16,
                                ));
                            }
                            pos += 1;
                            continue;
                        }

                        pos += 1;
                        let val_start = pos as u16;

                        while pos < qs_len && buf[qs_start_abs + pos] != b'&' {
                            pos += 1;
                        }

                        let val_len_raw = (pos - val_start as usize) as u16;
                        offsets.push((
                            (qs_start_abs + key_start as usize) as u16,
                            key_len_raw,
                            (qs_start_abs + val_start as usize) as u16,
                            val_len_raw,
                        ));

                        pos += 1;
                    }

                    offsets
                } else {
                    Vec::new()
                }
            };

            (
                buf,
                header_offsets,
                header_offsets_raw,
                query_offsets,
                Vec::new(),
                po as u16,
                pl as u16,
                mo as u16,
                ml as u8,
                source_ip,
                should_trust_forwarded,
            )
        };

        // Generate request ID for WebSocket connection
        let ws_request_id = extract_or_generate_request_id(&buf, &header_offsets_raw);
        let (client_ip, client_proto) = Self::derive_client_context(
            &buf,
            &header_offsets_raw,
            source_ip,
            should_trust_forwarded,
        );

        // Upgrade the connection
        {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.upgrade_to_ws(endpoint_idx);
            let _ = conn.reregister(self.poll.registry(), Interest::READABLE);
        }

        // Track the connection
        self.ws_manager
            .borrow_mut()
            .add_connection(endpoint_idx, conn_idx);

        // Call ws.join(ctx) handler
        let mgr = self.ws_manager.borrow();
        let endpoint = &mgr.endpoints[endpoint_idx as usize];

        if let Some(ref join_key) = endpoint.join_handler {
            let join_fn: Function = self.lua.registry_value(join_key)?;

            // Create a request context for the join handler
            let (ctx, ctx_idx) = self.request_pool.acquire(
                &self.lua,
                buf,
                method_off,
                method_len,
                path_off,
                path_len,
                0,
                0, // no body
                header_offsets,
                query_offsets,
                &params,
                ws_request_id.clone(),
                client_ip.clone(),
                client_proto.clone(),
            )?;

            drop(mgr);

            // Set WsManager context for the join handler
            self.ws_manager
                .borrow_mut()
                .set_context(conn_idx, endpoint_idx);

            // Execute join handler
            let thread = self.thread_pool.acquire(&self.lua, &join_fn)?;
            match thread.resume::<Value>(ctx) {
                Ok(state_value) => {
                    // Store the returned state in Lua registry
                    if !matches!(state_value, Value::Nil) {
                        let state_key = self.lua.create_registry_value(state_value)?;
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx)
                            && let Some(ref mut ws) = conn.ws_data
                        {
                            ws.state_key = Some(state_key);
                        }
                    }
                    self.thread_pool.release(thread);
                }
                Err(e) => {
                    warn!("WS join handler error: {}", e);
                    self.thread_pool.release(thread);
                }
            }

            self.request_pool.release(ctx_idx);

            // Join handler may have queued frames for other WS connections.
            self.reregister_ws_writers();
        }

        info!(
            "WS connection {} upgraded to endpoint #{}",
            conn_idx, endpoint_idx
        );
        Ok(())
    }

    // ── WebSocket event handling ──

    fn handle_ws_event(&mut self, conn_idx: usize, event: &mio::event::Event) -> Result<()> {
        if event.is_readable() {
            self.handle_ws_readable(conn_idx)?;
        }

        if event.is_writable() {
            self.handle_ws_writable(conn_idx)?;
        }

        Ok(())
    }

    fn handle_ws_readable(&mut self, conn_idx: usize) -> Result<()> {
        // Read data from socket
        let bytes_read = {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            match conn.ws_read() {
                Ok(n) => n,
                Err(_) => {
                    drop(conns);
                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            }
        };

        if bytes_read == 0 {
            // EOF
            self.handle_ws_disconnect(conn_idx)?;
            return Ok(());
        }

        // Parse and process frames
        loop {
            let frame_result = {
                let conns = self.connections.borrow();
                let conn = &conns[conn_idx];
                let unprocessed = &conn.read_buf[..conn.read_pos];
                ws_frame::try_parse_frame(unprocessed).map(|h| {
                    (
                        h.fin,
                        h.opcode,
                        h.masked,
                        h.mask,
                        h.payload_offset,
                        h.payload_len,
                        h.total_frame_len,
                    )
                })
            };

            let Some((fin, opcode, masked, mask, payload_offset, payload_len, total_frame_len)) =
                frame_result
            else {
                break; // incomplete frame, wait for more data
            };

            // Extract and unmask payload
            let payload = {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];

                if masked && payload_len > 0 {
                    ws_frame::unmask_payload_in_place(
                        &mut conn.read_buf[payload_offset..payload_offset + payload_len],
                        mask,
                    );
                }

                let payload = conn.read_buf[payload_offset..payload_offset + payload_len].to_vec();

                // Advance buffer past this frame
                let remaining = conn.read_pos - total_frame_len;
                if remaining > 0 {
                    conn.read_buf.copy_within(total_frame_len..conn.read_pos, 0);
                }
                conn.read_pos = remaining;

                payload
            };

            match opcode {
                WsOpcode::Text | WsOpcode::Binary => {
                    if fin {
                        // Complete message
                        self.dispatch_ws_message(conn_idx, &payload)?;
                    } else {
                        // Start of fragmented message
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx)
                            && let Some(ref mut ws) = conn.ws_data
                        {
                            ws.fragment_opcode = Some(opcode);
                            ws.fragment_buf = Some(payload);
                        }
                    }
                }
                WsOpcode::Continuation => {
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx)
                        && let Some(ref mut ws) = conn.ws_data
                    {
                        if let Some(ref mut frag) = ws.fragment_buf {
                            frag.extend_from_slice(&payload);
                        }
                        if fin {
                            let assembled = ws.fragment_buf.take().unwrap_or_default();
                            ws.fragment_opcode = None;
                            drop(conns);
                            self.dispatch_ws_message(conn_idx, &assembled)?;
                        }
                    }
                }
                WsOpcode::Ping => {
                    // Respond with pong echoing the payload
                    let mut frame_buf = self.ws_manager.borrow_mut().get_frame_buf();
                    ws_frame::write_pong_frame(&mut frame_buf, &payload);
                    let frame = Bytes::from(frame_buf);

                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx) {
                        conn.queue_ws_frame(frame);
                        let _ = conn.reregister(
                            self.poll.registry(),
                            Interest::READABLE | Interest::WRITABLE,
                        );
                    }
                }
                WsOpcode::Pong => {
                    // Ignore pong responses
                }
                WsOpcode::Close => {
                    // Send close frame back if we haven't already
                    let should_close = {
                        let conns = self.connections.borrow();
                        conns
                            .get(conn_idx)
                            .and_then(|c| c.ws_data.as_ref())
                            .map(|ws| !ws.close_sent)
                            .unwrap_or(false)
                    };

                    if should_close {
                        let status_code = if payload.len() >= 2 {
                            u16::from_be_bytes([payload[0], payload[1]])
                        } else {
                            1000
                        };

                        let mut frame_buf = self.ws_manager.borrow_mut().get_frame_buf();
                        ws_frame::write_close_frame(&mut frame_buf, status_code, "");
                        let frame = Bytes::from(frame_buf);

                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            if let Some(ref mut ws) = conn.ws_data {
                                ws.close_sent = true;
                            }
                            conn.queue_ws_frame(frame);
                            conn.state = ConnectionState::WsClosed;
                            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
                        }
                    }

                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            }
        }

        // If there are frames queued for writing, register for WRITABLE too
        {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx)
                && let Some(ref ws) = conn.ws_data
                && !ws.write_queue.is_empty()
            {
                drop(conns);
                let mut conns = self.connections.borrow_mut();
                if let Some(conn) = conns.get_mut(conn_idx) {
                    let _ = conn.reregister(
                        self.poll.registry(),
                        Interest::READABLE | Interest::WRITABLE,
                    );
                }
            }
        }

        Ok(())
    }

    fn handle_ws_writable(&mut self, conn_idx: usize) -> Result<()> {
        let (drained, is_ws_closed) = {
            let mut conns = self.connections.borrow_mut();
            let conn = match conns.get_mut(conn_idx) {
                Some(c) => c,
                None => return Ok(()),
            };

            let drained = match conn.try_write_ws() {
                Ok(d) => d,
                Err(_) => {
                    conn.state = ConnectionState::Closed;
                    drop(conns);
                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            };

            let is_ws_closed = conn.state == ConnectionState::WsClosed;
            (drained, is_ws_closed)
        };

        if drained {
            if is_ws_closed {
                // Close handshake complete, disconnect
                self.handle_ws_disconnect(conn_idx)?;
            } else {
                // Queue empty, only listen for reads
                let mut conns = self.connections.borrow_mut();
                if let Some(conn) = conns.get_mut(conn_idx) {
                    let _ = conn.reregister(self.poll.registry(), Interest::READABLE);
                }
            }
        }

        Ok(())
    }

    /// Dispatch a complete WebSocket text message to the appropriate Lua handler.
    fn dispatch_ws_message(&mut self, conn_idx: usize, payload: &[u8]) -> Result<()> {
        // Parse the JSON message
        let msg_value = match crate::direct_json_parser::json_bytes_ref_to_lua_direct(
            &self.lua,
            &Bytes::copy_from_slice(payload),
        ) {
            Ok(v) => v,
            Err(e) => {
                debug!("WS invalid JSON from conn {}: {}", conn_idx, e);
                return Ok(());
            }
        };

        // Extract the "type" field for event routing
        let (event_name, msg_table) = match msg_value {
            Value::Table(ref tbl) => {
                let type_val: Value = tbl.raw_get("type").unwrap_or(Value::Nil);
                match type_val {
                    Value::String(s) => {
                        let event = s.to_str()?.to_string();
                        // Remove "type" from the message table
                        let _ = tbl.raw_set("type", Value::Nil);
                        (Some(event), tbl.clone())
                    }
                    _ => (None, tbl.clone()),
                }
            }
            _ => {
                debug!("WS message is not a JSON object, ignoring");
                return Ok(());
            }
        };

        let endpoint_idx = {
            let conns = self.connections.borrow();
            conns
                .get(conn_idx)
                .and_then(|c| c.ws_data.as_ref())
                .map(|ws| ws.endpoint_idx)
                .unwrap_or(0)
        };

        // Get the handler function from the endpoint
        let handler_fn: Option<Function> = {
            let mgr = self.ws_manager.borrow();
            if let Some(endpoint) = mgr.endpoints.get(endpoint_idx as usize) {
                if let Some(event) = &event_name {
                    if let Some(key) = endpoint.event_handlers.get(event) {
                        self.lua.registry_value(key).ok()
                    } else {
                        // Try "message" catch-all
                        endpoint
                            .event_handlers
                            .get("message")
                            .and_then(|key| self.lua.registry_value(key).ok())
                    }
                } else {
                    // No type field, try "message" catch-all
                    endpoint
                        .event_handlers
                        .get("message")
                        .and_then(|key| self.lua.registry_value(key).ok())
                }
            } else {
                None
            }
        };

        let Some(handler_fn) = handler_fn else {
            debug!(
                "WS no handler for event {:?} on endpoint {}",
                event_name, endpoint_idx
            );
            return Ok(());
        };

        // Set WsManager context
        self.ws_manager
            .borrow_mut()
            .set_context(conn_idx, endpoint_idx);

        // Get the connection state for the handler
        let state_value: Value = {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx) {
                if let Some(ref ws) = conn.ws_data {
                    if let Some(ref key) = ws.state_key {
                        self.lua.registry_value(key).unwrap_or(Value::Nil)
                    } else {
                        Value::Nil
                    }
                } else {
                    Value::Nil
                }
            } else {
                Value::Nil
            }
        };

        // Create a minimal request context for the handler
        let ctx = self.lua.create_table()?;

        // Call handler: ws.listen.<event>(msg, ctx, state)
        let thread = self.thread_pool.acquire(&self.lua, &handler_fn)?;
        match thread.resume::<Value>((Value::Table(msg_table), Value::Table(ctx), state_value)) {
            Ok(new_state) => {
                // If handler returns a value, update the connection state
                if !matches!(new_state, Value::Nil) {
                    let state_key = self.lua.create_registry_value(new_state)?;
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx)
                        && let Some(ref mut ws) = conn.ws_data
                    {
                        // Remove old state key
                        if let Some(old_key) = ws.state_key.take() {
                            self.lua.remove_registry_value(old_key)?;
                        }
                        ws.state_key = Some(state_key);
                    }
                }
                self.thread_pool.release(thread);
            }
            Err(e) => {
                warn!("WS handler error for event {:?}: {}", event_name, e);
                self.thread_pool.release(thread);
            }
        }

        // Handler may have queued frames for sender and/or other WS connections.
        self.reregister_ws_writers();

        Ok(())
    }

    fn handle_ws_disconnect(&mut self, conn_idx: usize) -> Result<()> {
        let endpoint_idx = {
            let conns = self.connections.borrow();
            match conns.get(conn_idx) {
                Some(conn) if conn.is_websocket() => {
                    conn.ws_data.as_ref().map(|ws| ws.endpoint_idx).unwrap_or(0)
                }
                _ => return Ok(()),
            }
        };

        // Set context for leave handler
        self.ws_manager
            .borrow_mut()
            .set_context(conn_idx, endpoint_idx);

        // Call leave handler
        let leave_fn: Option<Function> = {
            let mgr = self.ws_manager.borrow();
            mgr.endpoints
                .get(endpoint_idx as usize)
                .and_then(|ep| ep.leave_handler.as_ref())
                .and_then(|key| self.lua.registry_value(key).ok())
        };

        if let Some(leave_fn) = leave_fn {
            let state_value: Value = {
                let conns = self.connections.borrow();
                conns
                    .get(conn_idx)
                    .and_then(|c| c.ws_data.as_ref())
                    .and_then(|ws| ws.state_key.as_ref())
                    .and_then(|key| self.lua.registry_value(key).ok())
                    .unwrap_or(Value::Nil)
            };

            let thread = self.thread_pool.acquire(&self.lua, &leave_fn)?;
            match thread.resume::<Value>(state_value) {
                Ok(_) => {
                    self.thread_pool.release(thread);
                }
                Err(e) => {
                    warn!("WS leave handler error: {}", e);
                    self.thread_pool.release(thread);
                }
            }
        }

        // Leave handler may have queued frames for other WS connections.
        self.reregister_ws_writers();

        // Unsubscribe from all topics
        {
            let conns = self.connections.borrow();
            self.ws_manager
                .borrow_mut()
                .unsubscribe_all(conn_idx, &conns);
        }

        // Remove from endpoint tracking
        self.ws_manager
            .borrow_mut()
            .remove_connection(endpoint_idx, conn_idx);

        // Remove state from Lua registry
        {
            let mut conns = self.connections.borrow_mut();
            if let Some(conn) = conns.get_mut(conn_idx)
                && let Some(ref mut ws) = conn.ws_data
                && let Some(state_key) = ws.state_key.take()
            {
                let _ = self.lua.remove_registry_value(state_key);
            }
        }

        // Deregister and remove connection
        {
            let mut conns = self.connections.borrow_mut();
            if conns.contains(conn_idx) {
                let mut conn = conns.remove(conn_idx);
                let _ = self.poll.registry().deregister(&mut conn.socket);
            }
        }

        info!(
            "WS connection {} disconnected from endpoint #{}",
            conn_idx, endpoint_idx
        );
        Ok(())
    }

    // ── Existing HTTP helper methods ──

    fn check_timeouts(&mut self) -> Result<()> {
        let mut to_timeout = Vec::new();

        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            // Skip WS connections -- they're long-lived
            {
                let conns = self.connections.borrow();
                if conns
                    .get(conn_idx)
                    .map(|c| c.is_websocket())
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if pending.started_at.elapsed().as_millis() as u64 > DEFAULT_COROUTINE_TIMEOUT_MS {
                to_timeout.push(conn_idx);
            }
        }

        for conn_idx in to_timeout {
            self.yielded_coroutines.remove(&conn_idx);
            let mut conns = self.connections.borrow_mut();
            if !conns.contains(conn_idx) {
                continue;
            }
            let conn = &mut conns[conn_idx];
            conn.thread = None;
            conn.state = ConnectionState::Writing;
            let buf = self.buffer_pool.get_response_buf();
            self.set_http_response(
                conn,
                500,
                Bytes::from_static(b"Coroutine timeout"),
                Some("text/plain"),
                HashMap::new(),
                buf,
                None,
            );
            let _ = conn.reregister(self.poll.registry(), Interest::WRITABLE);
        }

        Ok(())
    }

    #[inline]
    fn reregister_ws_writers(&mut self) {
        let pending: Vec<usize> = {
            let conns = self.connections.borrow();
            conns
                .iter()
                .filter_map(|(idx, conn)| {
                    conn.ws_data
                        .as_ref()
                        .and_then(|ws| (!ws.write_queue.is_empty()).then_some(idx))
                })
                .collect()
        };

        if pending.is_empty() {
            return;
        }

        let mut conns = self.connections.borrow_mut();
        for idx in pending {
            if let Some(conn) = conns.get_mut(idx) {
                let _ = conn.reregister(
                    self.poll.registry(),
                    Interest::READABLE | Interest::WRITABLE,
                );
            }
        }
    }

    fn resume_yielded_coroutines(&mut self) -> Result<()> {
        let mut to_resume = Vec::new();

        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            if !self.connections.borrow().contains(conn_idx) {
                to_resume.push(conn_idx);
                continue;
            }

            if pending.thread.status() == ThreadStatus::Resumable {
                to_resume.push(conn_idx);
            }
        }

        for conn_idx in to_resume {
            if let Some(pending) = self.yielded_coroutines.remove(&conn_idx) {
                match pending.thread.resume(()) {
                    Ok(mlua::Value::Nil) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                    Ok(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                        }
                    }
                    Err(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
