use hyper::{body::Bytes, StatusCode};
use mlua::Lua;
use smallvec::SmallVec;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, warn};

use crate::{fast_router::FastRouter, http_task::HttpTask, HttpMethod, Route, ServerConfig};

/// HTTP-specific request wrapper
pub struct LuaRequest {
    pub method: Bytes,
    pub path: Bytes,
    pub headers: SmallVec<[(Bytes, Bytes); 8]>,
    pub query: SmallVec<[(Bytes, Bytes); 8]>,
    pub body: Option<Bytes>,
    pub respond_to: tokio::sync::oneshot::Sender<crate::HttpResponse>,
    pub started_at: std::time::Instant,
}

/// Run the HTTP event loop that routes requests to Lua handlers
pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, _config: ServerConfig) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes).expect("Failed to build router");

        while let Some(req) = rx.recv().await {
            // Methods should be only lua functions, so lua function is utf8 safe
            let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };

            let method = match HttpMethod::from_str(method_str) {
                Some(m) => m,
                None => {
                    let _ = req.respond_to.send(crate::HttpResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: Bytes::from(format!(
                            "Invalid HTTP method '{}'. Valid methods: {}",
                            method_str,
                            HttpMethod::valid_methods().join(", ")
                        )),
                    });
                    continue;
                }
            };

            // Paths should be only lua functions, so lua function is utf8 safe
            let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

            let (handler, params) = match fast_router.match_route(method, path_str) {
                Some((h, p)) => (h, p),
                None => {
                    let elapsed = req.started_at.elapsed();
                    warn!(
                        "{} {} - 404 NOT_FOUND in {:.2}ms",
                        method,
                        path_str,
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = req.respond_to.send(crate::HttpResponse {
                        status: StatusCode::NOT_FOUND,
                        body: Bytes::from("Route not found"),
                    });
                    continue;
                }
            };

            // Create HTTP task and execute it directly
            let task = HttpTask {
                method: req.method,
                path: req.path,
                headers: req.headers,
                query: req.query,
                params,
                body: req.body,
                handler: handler.clone(),
                respond_to: req.respond_to,
                started_at: req.started_at,
            };

            // Execute the task
            if let Err(e) = task.execute(&lua).await {
                debug!("Task execution failed: {}", e);
            }
        }
    });
}
