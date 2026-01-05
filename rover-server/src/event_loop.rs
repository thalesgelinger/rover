use std::collections::HashMap;
use std::time::Instant;
use anyhow::Result;
use mlua::{Lua, Thread};
use smallvec::SmallVec;
use mio::{Events, Poll, Token};
use tracing::debug;

use crate::{HttpMethod, Route, ServerConfig, fast_router::FastRouter, HttpResponse, Bytes};

pub struct CoroutineState {
    pub thread: Thread,
    pub started_at: Instant,
    pub method: Bytes,
    pub path: Bytes,
}

pub struct EventLoop {
    lua: Lua,
    router: FastRouter,
    poll: Poll,
    coroutines: HashMap<Token, CoroutineState>,
    next_token: usize,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
}

impl EventLoop {
    pub fn new(
        lua: Lua,
        routes: Vec<Route>,
        config: ServerConfig,
        openapi_spec: Option<serde_json::Value>,
    ) -> Result<Self> {
        let router = FastRouter::from_routes(routes)?;
        let poll = Poll::new()?;
        
        Ok(Self {
            lua,
            router,
            poll,
            coroutines: HashMap::new(),
            next_token: 1,
            config,
            openapi_spec,
        })
    }

    pub fn handle_request(
        &mut self,
        method: Bytes,
        path: Bytes,
        headers: SmallVec<[(Bytes, Bytes); 8]>,
        query: SmallVec<[(Bytes, Bytes); 8]>,
        body: Option<Bytes>,
        started_at: Instant,
    ) -> HttpResponse {
        let method_str = unsafe { std::str::from_utf8_unchecked(&method) };
        let path_str = unsafe { std::str::from_utf8_unchecked(&path) };

        let http_method = match HttpMethod::from_str(method_str) {
            Some(m) => m,
            None => {
                return HttpResponse {
                    status: 400,
                    body: Bytes::from(format!(
                        "Invalid HTTP method '{}'. Valid methods: {}",
                        method_str,
                        HttpMethod::valid_methods().join(", ")
                    )),
                    content_type: Some("text/plain".to_string()),
                };
            }
        };

        if self.config.docs && path_str == "/docs" && self.openapi_spec.is_some() {
            let html = rover_openapi::scalar_html(self.openapi_spec.as_ref().unwrap());
            let elapsed = started_at.elapsed();
            debug!("GET /docs - 200 OK in {:.2}ms", elapsed.as_secs_f64() * 1000.0);
            return HttpResponse {
                status: 200,
                body: Bytes::from(html),
                content_type: Some("text/html".to_string()),
            };
        }

        let (handler, params) = match self.router.match_route(http_method, path_str) {
            Some((h, p)) => (h, p),
            None => {
                return HttpResponse {
                    status: 404,
                    body: Bytes::from("Route not found"),
                    content_type: Some("text/plain".to_string()),
                };
            }
        };

        match crate::http_task::execute_handler(
            &self.lua,
            handler,
            method.clone(),
            path.clone(),
            headers,
            query,
            params,
            body,
            started_at,
        ) {
            Ok(response) => response,
            Err(e) => HttpResponse {
                status: 500,
                body: Bytes::from(format!("Handler error: {}", e)),
                content_type: Some("text/plain".to_string()),
            },
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let mut events = Events::with_capacity(128);

        loop {
            self.poll.poll(&mut events, None)?;

            for event in events.iter() {
                if let Some(coro_state) = self.coroutines.remove(&event.token()) {
                    let _ = coro_state;
                }
            }
        }
    }
}
