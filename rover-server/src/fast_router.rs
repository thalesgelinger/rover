use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ahash::AHasher;
use anyhow::Result;
use bytes::Bytes;
use matchit::Router;
use mlua::Function;
use smallvec::SmallVec;

use crate::{HttpMethod, Route};

#[inline]
fn hash_path(path: &str) -> u64 {
    let mut hasher = AHasher::default();
    path.hash(&mut hasher);
    hasher.finish()
}

pub struct FastRouter {
    router: Router<SmallVec<[(HttpMethod, usize); 2]>>,
    handlers: Vec<Function>,
    static_routes: HashMap<(u64, HttpMethod), usize>,

    // WebSocket routing (separate from HTTP to avoid polluting hot path)
    ws_router: Router<u16>,       // path pattern -> endpoint_idx
    ws_static: HashMap<u64, u16>, // hash(path) -> endpoint_idx (static WS paths)
    has_ws_routes: bool,
}

impl FastRouter {
    pub fn from_routes(routes: Vec<Route>) -> Result<Self> {
        let mut router = Router::new();
        let mut handlers = Vec::new();
        let mut pattern_map: HashMap<Vec<u8>, SmallVec<[(HttpMethod, usize); 2]>> = HashMap::new();
        let mut static_routes = HashMap::new();

        for route in routes {
            let handler_idx = handlers.len();
            handlers.push(route.handler);

            if route.is_static {
                let pattern_str = std::str::from_utf8(&route.pattern)
                    .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
                let path_hash = hash_path(pattern_str);
                static_routes.insert((path_hash, route.method), handler_idx);
            }

            pattern_map
                .entry(route.pattern.to_vec())
                .or_insert_with(SmallVec::new)
                .push((route.method, handler_idx));
        }

        for (pattern_bytes, methods) in pattern_map {
            let pattern_str = std::str::from_utf8(&pattern_bytes)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
            router.insert(pattern_str, methods)?;
        }

        Ok(Self {
            router,
            handlers,
            static_routes,
            ws_router: Router::new(),
            ws_static: HashMap::new(),
            has_ws_routes: false,
        })
    }

    /// Register WebSocket routes. Called after from_routes with the WS endpoint indices.
    pub fn add_ws_routes(&mut self, ws_patterns: Vec<(String, u16, bool)>) -> Result<()> {
        for (pattern, endpoint_idx, is_static) in ws_patterns {
            if is_static {
                let path_hash = hash_path(&pattern);
                self.ws_static.insert(path_hash, endpoint_idx);
            }
            self.ws_router.insert(&pattern, endpoint_idx)?;
        }
        self.has_ws_routes = true;
        Ok(())
    }

    /// Match a WebSocket route. Returns (endpoint_idx, params) if found.
    /// Only called when an HTTP Upgrade header is detected.
    pub fn match_ws_route(&self, path: &str) -> Option<(u16, Vec<(Bytes, Bytes)>)> {
        if !self.has_ws_routes {
            return None;
        }

        // Fast path: static WS routes
        let path_hash = hash_path(path);
        if let Some(&endpoint_idx) = self.ws_static.get(&path_hash) {
            return Some((endpoint_idx, Vec::new()));
        }

        // Slow path: dynamic WS routes with parameters
        let matched = self.ws_router.at(path).ok()?;
        let endpoint_idx = *matched.value;

        let mut params = Vec::with_capacity(matched.params.len());
        for (name, value) in matched.params.iter() {
            let decoded = urlencoding::decode(value).ok()?.into_owned();
            if decoded.is_empty() {
                return None;
            }
            params.push((
                Bytes::copy_from_slice(name.as_bytes()),
                Bytes::copy_from_slice(decoded.as_bytes()),
            ));
        }

        Some((endpoint_idx, params))
    }

    /// Match route and return handler + params (zero-copy where possible)
    pub fn match_route(
        &self,
        method: HttpMethod,
        path: &str,
    ) -> Option<(&Function, Vec<(Bytes, Bytes)>)> {
        // Fast path: static routes (no params)
        let path_hash = hash_path(path);
        if let Some(&handler_idx) = self.static_routes.get(&(path_hash, method)) {
            return Some((&self.handlers[handler_idx], Vec::new()));
        }

        // Slow path: dynamic routes with parameters
        let matched = self.router.at(path).ok()?;

        let handler_idx = matched
            .value
            .iter()
            .find(|(m, _)| *m == method)
            .map(|(_, idx)| *idx)?;

        let mut params = Vec::with_capacity(matched.params.len());
        for (name, value) in matched.params.iter() {
            let decoded = urlencoding::decode(value).ok()?.into_owned();
            if decoded.is_empty() {
                return None;
            }
            params.push((
                Bytes::copy_from_slice(name.as_bytes()),
                Bytes::copy_from_slice(decoded.as_bytes()),
            ));
        }

        Some((&self.handlers[handler_idx], params))
    }
}
