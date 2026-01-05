use std::net::SocketAddr;
use std::time::Instant;
use anyhow::Result;
use mlua::Lua;
use smallvec::SmallVec;
use tiny_http::{Server, Response};
use form_urlencoded;

use crate::{Route, ServerConfig, event_loop::EventLoop, Bytes};

pub fn run_server(
    lua: Lua,
    routes: Vec<Route>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    addr: SocketAddr,
) -> Result<()> {
    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => return Err(anyhow::anyhow!("Failed to bind: {}", e)),
    };
    
    let mut event_loop = EventLoop::new(lua, routes, config, openapi_spec)?;

    loop {
        let mut request = server.recv().map_err(|e| anyhow::anyhow!("Recv error: {}", e))?;
        let started_at = Instant::now();

        let method = Bytes::from(request.method().as_str().to_string());
        let path = Bytes::from(request.url().to_string());

        let headers: SmallVec<[(Bytes, Bytes); 8]> = request
            .headers()
            .iter()
            .map(|h| {
                (
                    Bytes::from(h.field.as_str().as_str().to_string()),
                    Bytes::from(h.value.as_str().to_string()),
                )
            })
            .collect();

        let query: SmallVec<[(Bytes, Bytes); 8]> = if let Some(q) = request.url().split('?').nth(1) {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (Bytes::from(k.into_owned()), Bytes::from(v.into_owned())))
                .collect()
        } else {
            SmallVec::new()
        };

        let body = if let Some(len) = request.body_length() {
            if len > 0 {
                let mut buf = vec![0u8; len];
                if let Ok(_) = request.as_reader().read_exact(&mut buf) {
                    Some(Bytes::from(buf))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let http_response = event_loop.handle_request(method, path, headers, query, body, started_at);

        let mut response = Response::from_data(http_response.body);
        response = response.with_status_code(http_response.status);
        
        if let Some(ct) = http_response.content_type {
            response = response.with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).unwrap()
            );
        }

        let _ = request.respond(response);
    }
}
