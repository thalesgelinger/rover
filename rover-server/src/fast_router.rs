use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ahash::AHasher;
use anyhow::Result;
use bytes::Bytes;
use matchit::Router;
use mlua::Function;
use smallvec::SmallVec;

use crate::{HttpMethod, Route};

pub enum RouteMatch {
    Found {
        handler: Function,
        params: Vec<(Bytes, Bytes)>,
        is_head: bool,
    },
    MethodNotAllowed {
        allowed: Vec<HttpMethod>,
    },
    NotFound,
}

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
    static_path_methods: HashMap<u64, SmallVec<[HttpMethod; 4]>>,

    // WebSocket routing (separate from HTTP to avoid polluting hot path)
    ws_router: Router<u16>,       // path pattern -> endpoint_idx
    ws_static: HashMap<u64, u16>, // hash(path) -> endpoint_idx (static WS paths)
    has_ws_routes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    fn dummy_route(lua: &Lua, method: HttpMethod, path: &str) -> Route {
        Route {
            method,
            pattern: Bytes::copy_from_slice(path.as_bytes()),
            param_names: Vec::new(),
            handler: lua.create_function(|_, ()| Ok(())).unwrap(),
            is_static: true,
            middlewares: Default::default(),
        }
    }

    #[test]
    fn should_auto_map_head_to_get() {
        let lua = Lua::new();
        let router = FastRouter::from_routes(vec![dummy_route(&lua, HttpMethod::Get, "/items")])
            .expect("router");

        match router.match_route(HttpMethod::Head, "/items") {
            RouteMatch::Found { is_head, .. } => assert!(is_head),
            _ => panic!("expected HEAD to resolve from GET"),
        }
    }

    #[test]
    fn should_return_405_with_allow_for_known_path() {
        let lua = Lua::new();
        let router = FastRouter::from_routes(vec![
            dummy_route(&lua, HttpMethod::Get, "/items"),
            dummy_route(&lua, HttpMethod::Post, "/items"),
        ])
        .expect("router");

        match router.match_route(HttpMethod::Patch, "/items") {
            RouteMatch::MethodNotAllowed { allowed } => {
                assert!(allowed.contains(&HttpMethod::Get));
                assert!(allowed.contains(&HttpMethod::Post));
                assert!(allowed.contains(&HttpMethod::Head));
                assert!(allowed.contains(&HttpMethod::Options));
            }
            _ => panic!("expected 405 match"),
        }
    }
}

impl FastRouter {
    pub fn from_routes(routes: Vec<Route>) -> Result<Self> {
        let mut router = Router::new();
        let mut handlers = Vec::new();
        let mut pattern_map: HashMap<Vec<u8>, SmallVec<[(HttpMethod, usize); 2]>> = HashMap::new();
        let mut static_routes = HashMap::new();
        let mut static_path_methods: HashMap<u64, SmallVec<[HttpMethod; 4]>> = HashMap::new();

        for route in routes {
            let handler_idx = handlers.len();
            handlers.push(route.handler);

            if route.is_static {
                let pattern_str = std::str::from_utf8(&route.pattern)
                    .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
                let path_hash = hash_path(pattern_str);
                static_routes.insert((path_hash, route.method), handler_idx);
                static_path_methods
                    .entry(path_hash)
                    .or_insert_with(SmallVec::new)
                    .push(route.method);
            }

            let methods = pattern_map
                .entry(route.pattern.to_vec())
                .or_insert_with(SmallVec::new);
            if methods.iter().any(|(m, _)| *m == route.method) {
                if let Ok(pattern_str) = std::str::from_utf8(&route.pattern) {
                    tracing::warn!(
                        "Duplicate route method '{}' for path '{}'; last one wins",
                        route.method,
                        pattern_str
                    );
                }
            }
            methods.push((route.method, handler_idx));
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
            static_path_methods,
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

    fn normalize_allowed_methods(methods: &[HttpMethod]) -> Vec<HttpMethod> {
        let mut has_get = false;
        let mut has_head = false;
        let mut uniq = SmallVec::<[HttpMethod; 8]>::new();

        for method in methods {
            if !uniq.contains(method) {
                if *method == HttpMethod::Get {
                    has_get = true;
                }
                if *method == HttpMethod::Head {
                    has_head = true;
                }
                uniq.push(*method);
            }
        }

        if has_get && !has_head {
            uniq.push(HttpMethod::Head);
        }

        if !uniq.contains(&HttpMethod::Options) {
            uniq.push(HttpMethod::Options);
        }

        let mut out = uniq.to_vec();
        out.sort_by_key(|m| match m {
            HttpMethod::Get => 0,
            HttpMethod::Head => 1,
            HttpMethod::Post => 2,
            HttpMethod::Put => 3,
            HttpMethod::Patch => 4,
            HttpMethod::Delete => 5,
            HttpMethod::Options => 6,
        });
        out
    }

    /// Match route with proper 404/405 semantics and auto-HEAD support.
    pub fn match_route(&self, method: HttpMethod, path: &str) -> RouteMatch {
        // Fast path: static routes (no params)
        let path_hash = hash_path(path);
        if let Some(&handler_idx) = self.static_routes.get(&(path_hash, method)) {
            return RouteMatch::Found {
                handler: self.handlers[handler_idx].clone(),
                params: Vec::new(),
                is_head: method == HttpMethod::Head,
            };
        }

        // Auto-HEAD -> GET for static routes
        if method == HttpMethod::Head {
            if let Some(&handler_idx) = self.static_routes.get(&(path_hash, HttpMethod::Get)) {
                return RouteMatch::Found {
                    handler: self.handlers[handler_idx].clone(),
                    params: Vec::new(),
                    is_head: true,
                };
            }
        }

        // Path exists but method not allowed (static)
        if let Some(methods) = self.static_path_methods.get(&path_hash) {
            return RouteMatch::MethodNotAllowed {
                allowed: Self::normalize_allowed_methods(methods),
            };
        }

        // Slow path: dynamic routes with parameters
        let matched = match self.router.at(path) {
            Ok(m) => m,
            Err(_) => return RouteMatch::NotFound,
        };

        let handler_idx = if let Some((_, idx)) = matched.value.iter().find(|(m, _)| *m == method) {
            *idx
        } else if method == HttpMethod::Head {
            if let Some((_, idx)) = matched.value.iter().find(|(m, _)| *m == HttpMethod::Get) {
                *idx
            } else {
                return RouteMatch::MethodNotAllowed {
                    allowed: Self::normalize_allowed_methods(
                        &matched.value.iter().map(|(m, _)| *m).collect::<Vec<_>>(),
                    ),
                };
            }
        } else {
            return RouteMatch::MethodNotAllowed {
                allowed: Self::normalize_allowed_methods(
                    &matched.value.iter().map(|(m, _)| *m).collect::<Vec<_>>(),
                ),
            };
        };

        let mut params = Vec::with_capacity(matched.params.len());
        for (name, value) in matched.params.iter() {
            let decoded = match urlencoding::decode(value) {
                Ok(v) => v.into_owned(),
                Err(_) => return RouteMatch::NotFound,
            };
            if decoded.is_empty() {
                return RouteMatch::NotFound;
            }
            params.push((
                Bytes::copy_from_slice(name.as_bytes()),
                Bytes::copy_from_slice(decoded.as_bytes()),
            ));
        }

        RouteMatch::Found {
            handler: self.handlers[handler_idx].clone(),
            params,
            is_head: method == HttpMethod::Head,
        }
    }
}
