use std::cmp::Ordering;
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

#[inline]
fn is_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }

        if i + 2 >= bytes.len() {
            return false;
        }

        let hi = bytes[i + 1];
        let lo = bytes[i + 2];
        if !hi.is_ascii_hexdigit() || !lo.is_ascii_hexdigit() {
            return false;
        }

        i += 3;
    }

    true
}

#[inline]
fn is_mount_catch_all(pattern: &[u8]) -> bool {
    pattern.ends_with(b"/{*__rover_mount_path}")
}

fn compare_dynamic_patterns(a: &[u8], b: &[u8]) -> Ordering {
    let a_is_mount = is_mount_catch_all(a);
    let b_is_mount = is_mount_catch_all(b);
    if a_is_mount != b_is_mount {
        return if a_is_mount {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }

    b.len().cmp(&a.len()).then_with(|| a.cmp(b))
}

pub struct FastRouter {
    router: Router<SmallVec<[(HttpMethod, usize); 2]>>,
    handlers: Vec<Function>,
    static_routes: HashMap<(u64, HttpMethod), usize>,
    static_path_methods: HashMap<u64, SmallVec<[HttpMethod; 4]>>,
    mount_routes: Vec<MountRoute>,

    // WebSocket routing (separate from HTTP to avoid polluting hot path)
    ws_router: Router<u16>,       // path pattern -> endpoint_idx
    ws_static: HashMap<u64, u16>, // hash(path) -> endpoint_idx (static WS paths)
    has_ws_routes: bool,
}

struct MountRoute {
    base_path: String,
    method: HttpMethod,
    handler_idx: usize,
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

    fn dummy_route_with_body(lua: &Lua, method: HttpMethod, path: &str, body: &str) -> Route {
        let body = body.to_string();
        Route {
            method,
            pattern: Bytes::copy_from_slice(path.as_bytes()),
            param_names: Vec::new(),
            handler: lua
                .create_function(move |lua, ()| lua.create_string(&body))
                .unwrap(),
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

    #[test]
    fn should_return_not_found_for_unknown_path() {
        let lua = Lua::new();
        let router = FastRouter::from_routes(vec![dummy_route(&lua, HttpMethod::Get, "/items")])
            .expect("router");

        match router.match_route(HttpMethod::Get, "/missing") {
            RouteMatch::NotFound => {}
            _ => panic!("expected 404 match"),
        }
    }

    #[test]
    fn should_auto_map_head_to_get_for_dynamic_routes() {
        let lua = Lua::new();
        let mut route = dummy_route(&lua, HttpMethod::Get, "/items/{id}");
        route.is_static = false;
        let router = FastRouter::from_routes(vec![route]).expect("router");

        match router.match_route(HttpMethod::Head, "/items/123") {
            RouteMatch::Found {
                is_head, params, ..
            } => {
                assert!(is_head);
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, Bytes::from_static(b"id"));
                assert_eq!(params[0].1, Bytes::from_static(b"123"));
            }
            _ => panic!("expected HEAD fallback for dynamic route"),
        }
    }

    #[test]
    fn should_return_not_found_for_invalid_percent_encoded_param() {
        let lua = Lua::new();
        let mut route = dummy_route(&lua, HttpMethod::Get, "/items/{id}");
        route.is_static = false;
        let router = FastRouter::from_routes(vec![route]).expect("router");

        match router.match_route(HttpMethod::Get, "/items/%ZZ") {
            RouteMatch::NotFound => {}
            _ => panic!("expected invalid decode to resolve as not found"),
        }
    }

    #[test]
    fn should_reject_invalid_percent_encoded_ws_param() {
        let lua = Lua::new();
        let mut route = dummy_route(&lua, HttpMethod::Get, "/items");
        route.is_static = true;
        let mut router = FastRouter::from_routes(vec![route]).expect("router");
        router
            .add_ws_routes(vec![("/ws/{room}".to_string(), 7, false)])
            .expect("ws routes");

        assert!(router.match_ws_route("/ws/%ZZ").is_none());
    }

    #[test]
    fn should_match_dynamic_route_when_static_path_exists_for_other_method() {
        let lua = Lua::new();
        let static_route = dummy_route(&lua, HttpMethod::Get, "/users/me");
        let dynamic_route = Route {
            method: HttpMethod::Post,
            pattern: Bytes::from_static(b"/users/{id}"),
            param_names: vec!["id".to_string()],
            handler: lua.create_function(|_, ()| Ok(())).unwrap(),
            is_static: false,
            middlewares: Default::default(),
        };

        let router = FastRouter::from_routes(vec![static_route, dynamic_route]).expect("router");

        match router.match_route(HttpMethod::Post, "/users/me") {
            RouteMatch::Found { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, Bytes::from_static(b"id"));
                assert_eq!(params[0].1, Bytes::from_static(b"me"));
            }
            _ => panic!("expected dynamic route to match"),
        }
    }

    #[test]
    fn should_include_static_and_dynamic_methods_in_405_allow_header() {
        let lua = Lua::new();
        let static_route = dummy_route(&lua, HttpMethod::Get, "/users/me");
        let dynamic_route = Route {
            method: HttpMethod::Post,
            pattern: Bytes::from_static(b"/users/{id}"),
            param_names: vec!["id".to_string()],
            handler: lua.create_function(|_, ()| Ok(())).unwrap(),
            is_static: false,
            middlewares: Default::default(),
        };

        let router = FastRouter::from_routes(vec![static_route, dynamic_route]).expect("router");

        match router.match_route(HttpMethod::Put, "/users/me") {
            RouteMatch::MethodNotAllowed { allowed } => {
                assert!(allowed.contains(&HttpMethod::Get));
                assert!(allowed.contains(&HttpMethod::Head));
                assert!(allowed.contains(&HttpMethod::Post));
                assert!(allowed.contains(&HttpMethod::Options));
            }
            _ => panic!("expected 405 with merged methods"),
        }
    }

    #[test]
    fn should_prefer_exact_api_route_over_static_mount_catch_all() {
        let lua = Lua::new();

        let api_route = dummy_route_with_body(&lua, HttpMethod::Get, "/assets/health", "api");
        let static_mount_route = Route {
            method: HttpMethod::Get,
            pattern: Bytes::from_static(b"/assets/{*__rover_mount_path}"),
            param_names: vec!["__rover_mount_path".to_string()],
            handler: lua
                .create_function(|lua, ()| lua.create_string("static"))
                .unwrap(),
            is_static: false,
            middlewares: Default::default(),
        };

        let router = FastRouter::from_routes(vec![static_mount_route, api_route]).expect("router");

        match router.match_route(HttpMethod::Get, "/assets/health") {
            RouteMatch::Found {
                handler, params, ..
            } => {
                let body: mlua::String = handler.call(()).expect("call handler");
                assert_eq!(body.to_str().expect("body str"), "api");
                assert!(params.is_empty());
            }
            _ => panic!("expected exact API route to match first"),
        }

        match router.match_route(HttpMethod::Get, "/assets/app.js") {
            RouteMatch::Found {
                handler, params, ..
            } => {
                let body: mlua::String = handler.call(()).expect("call handler");
                assert_eq!(body.to_str().expect("body str"), "static");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, Bytes::from_static(b"__rover_mount_path"));
                assert_eq!(params[0].1, Bytes::from_static(b"app.js"));
            }
            _ => panic!("expected static mount route to match fallback"),
        }
    }

    #[test]
    fn should_match_dynamic_api_route_before_static_mount_regardless_of_registration_order() {
        let lua = Lua::new();

        let new_static_mount = || Route {
            method: HttpMethod::Get,
            pattern: Bytes::from_static(b"/assets/{*__rover_mount_path}"),
            param_names: vec!["__rover_mount_path".to_string()],
            handler: lua
                .create_function(|lua, ()| lua.create_string("static"))
                .unwrap(),
            is_static: false,
            middlewares: Default::default(),
        };

        let new_dynamic_api = || Route {
            method: HttpMethod::Get,
            pattern: Bytes::from_static(b"/assets/{id}"),
            param_names: vec!["id".to_string()],
            handler: lua
                .create_function(|lua, ()| lua.create_string("api"))
                .unwrap(),
            is_static: false,
            middlewares: Default::default(),
        };

        let router_mount_first =
            FastRouter::from_routes(vec![new_static_mount(), new_dynamic_api()]).expect("router");
        let router_api_first =
            FastRouter::from_routes(vec![new_dynamic_api(), new_static_mount()]).expect("router");

        for router in [router_mount_first, router_api_first] {
            match router.match_route(HttpMethod::Get, "/assets/health") {
                RouteMatch::Found {
                    handler, params, ..
                } => {
                    let body: mlua::String = handler.call(()).expect("call handler");
                    assert_eq!(body.to_str().expect("body str"), "api");
                    assert_eq!(params.len(), 1);
                    assert_eq!(params[0].0, Bytes::from_static(b"id"));
                    assert_eq!(params[0].1, Bytes::from_static(b"health"));
                }
                _ => panic!("expected dynamic API route to match first"),
            }

            match router.match_route(HttpMethod::Get, "/assets/js/app.js") {
                RouteMatch::Found {
                    handler, params, ..
                } => {
                    let body: mlua::String = handler.call(()).expect("call handler");
                    assert_eq!(body.to_str().expect("body str"), "static");
                    assert_eq!(params.len(), 1);
                    assert_eq!(params[0].0, Bytes::from_static(b"__rover_mount_path"));
                    assert_eq!(params[0].1, Bytes::from_static(b"js/app.js"));
                }
                _ => panic!("expected static mount fallback for nested asset path"),
            }
        }
    }
}

impl FastRouter {
    fn match_mount_route(&self, path: &str) -> Option<(&MountRoute, Bytes)> {
        for mount in &self.mount_routes {
            let raw_tail = if mount.base_path == "/" {
                match path.strip_prefix('/') {
                    Some(tail) => tail,
                    None => continue,
                }
            } else {
                let prefix = format!("{}/", mount.base_path.trim_end_matches('/'));
                match path.strip_prefix(&prefix) {
                    Some(tail) => tail,
                    None => continue,
                }
            };

            if raw_tail.is_empty() || !is_valid_percent_encoding(raw_tail) {
                continue;
            }

            let decoded = match urlencoding::decode(raw_tail) {
                Ok(d) => d.into_owned(),
                Err(_) => continue,
            };
            if decoded.is_empty() {
                continue;
            }

            return Some((mount, Bytes::copy_from_slice(decoded.as_bytes())));
        }

        None
    }

    fn collect_allowed_methods(
        static_methods: Option<&[HttpMethod]>,
        dynamic_methods: Option<&[(HttpMethod, usize)]>,
    ) -> Vec<HttpMethod> {
        let mut methods = SmallVec::<[HttpMethod; 8]>::new();

        if let Some(static_methods) = static_methods {
            methods.extend_from_slice(static_methods);
        }

        if let Some(dynamic_methods) = dynamic_methods {
            methods.extend(dynamic_methods.iter().map(|(m, _)| *m));
        }

        Self::normalize_allowed_methods(&methods)
    }

    pub fn from_routes(routes: Vec<Route>) -> Result<Self> {
        let mut router = Router::new();
        let mut handlers = Vec::new();
        let mut pattern_map: HashMap<Vec<u8>, SmallVec<[(HttpMethod, usize); 2]>> = HashMap::new();
        let mut static_routes = HashMap::new();
        let mut static_path_methods: HashMap<u64, SmallVec<[HttpMethod; 4]>> = HashMap::new();
        let mut mount_routes = Vec::new();

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
                    .or_default()
                    .push(route.method);
                continue;
            }

            if is_mount_catch_all(route.pattern.as_ref()) {
                let pattern_str = std::str::from_utf8(&route.pattern)
                    .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
                let suffix = "/{*__rover_mount_path}";
                let base_path = if pattern_str == suffix {
                    "/".to_string()
                } else {
                    pattern_str.trim_end_matches(suffix).to_string()
                };

                mount_routes.push(MountRoute {
                    base_path,
                    method: route.method,
                    handler_idx,
                });
                continue;
            }

            let methods = pattern_map.entry(route.pattern.to_vec()).or_default();
            if methods.iter().any(|(m, _)| *m == route.method)
                && let Ok(pattern_str) = std::str::from_utf8(&route.pattern)
            {
                tracing::warn!(
                    "Duplicate route method '{}' for path '{}'; last one wins",
                    route.method,
                    pattern_str
                );
            }
            methods.push((route.method, handler_idx));
        }

        let mut pattern_entries: Vec<_> = pattern_map.into_iter().collect();
        pattern_entries.sort_by(|(a, _), (b, _)| compare_dynamic_patterns(a, b));

        mount_routes.sort_by(|a, b| {
            b.base_path
                .len()
                .cmp(&a.base_path.len())
                .then_with(|| a.base_path.cmp(&b.base_path))
        });

        for (pattern_bytes, methods) in pattern_entries {
            let pattern_str = std::str::from_utf8(&pattern_bytes)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?;
            router.insert(pattern_str, methods)?;
        }

        Ok(Self {
            router,
            handlers,
            static_routes,
            static_path_methods,
            mount_routes,
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
            if !is_valid_percent_encoding(value) {
                return None;
            }
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
        if method == HttpMethod::Head
            && let Some(&handler_idx) = self.static_routes.get(&(path_hash, HttpMethod::Get))
        {
            return RouteMatch::Found {
                handler: self.handlers[handler_idx].clone(),
                params: Vec::new(),
                is_head: true,
            };
        }

        // Slow path: dynamic routes with parameters
        let static_methods = self.static_path_methods.get(&path_hash);
        let matched = self.router.at(path).ok();

        if let Some(matched) = matched {
            let handler_idx = if let Some((_, idx)) =
                matched.value.iter().find(|(m, _)| *m == method)
            {
                *idx
            } else if method == HttpMethod::Head {
                if let Some((_, idx)) = matched.value.iter().find(|(m, _)| *m == HttpMethod::Get) {
                    *idx
                } else {
                    return RouteMatch::MethodNotAllowed {
                        allowed: Self::collect_allowed_methods(
                            static_methods.map(|m| m.as_slice()),
                            Some(matched.value.as_slice()),
                        ),
                    };
                }
            } else {
                return RouteMatch::MethodNotAllowed {
                    allowed: Self::collect_allowed_methods(
                        static_methods.map(|m| m.as_slice()),
                        Some(matched.value.as_slice()),
                    ),
                };
            };

            let mut params = Vec::with_capacity(matched.params.len());
            for (name, value) in matched.params.iter() {
                if !is_valid_percent_encoding(value) {
                    return RouteMatch::NotFound;
                }
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

            return RouteMatch::Found {
                handler: self.handlers[handler_idx].clone(),
                params,
                is_head: method == HttpMethod::Head,
            };
        }

        if let Some(methods) = static_methods {
            return RouteMatch::MethodNotAllowed {
                allowed: Self::normalize_allowed_methods(methods),
            };
        }

        if let Some((mount, mount_path)) = self.match_mount_route(path) {
            if method == mount.method
                || (method == HttpMethod::Head && mount.method == HttpMethod::Get)
            {
                return RouteMatch::Found {
                    handler: self.handlers[mount.handler_idx].clone(),
                    params: vec![(Bytes::from_static(b"__rover_mount_path"), mount_path)],
                    is_head: method == HttpMethod::Head,
                };
            }

            return RouteMatch::MethodNotAllowed {
                allowed: Self::normalize_allowed_methods(&[mount.method]),
            };
        }

        RouteMatch::NotFound
    }
}
