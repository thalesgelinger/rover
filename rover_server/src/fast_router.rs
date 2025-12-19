use std::collections::HashMap;

use anyhow::Result;
use matchit::Router;
use mlua::Function;
use smallvec::SmallVec;

use crate::{HttpMethod, Route};

pub struct FastRouter {
    router: Router<SmallVec<[(HttpMethod, usize); 2]>>,
    handlers: Vec<Function>,
    static_routes: HashMap<(String, HttpMethod), usize>,
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
                    .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in route pattern"))?
                    .to_string();
                static_routes.insert((pattern_str, route.method), handler_idx);
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
        })
    }

    pub fn match_route(
        &self,
        method: HttpMethod,
        path: &str,
    ) -> Option<(&Function, HashMap<String, String>)> {
        if let Some(&handler_idx) = self.static_routes.get(&(path.to_string(), method)) {
            return Some((&self.handlers[handler_idx], HashMap::new()));
        }

        let matched = self.router.at(path).ok()?;

        let handler_idx = matched
            .value
            .iter()
            .find(|(m, _)| *m == method)
            .map(|(_, idx)| *idx)?;

        let handler = &self.handlers[handler_idx];

        let mut params = HashMap::with_capacity(matched.params.len());
        for (name, value) in matched.params.iter() {
            let decoded = urlencoding::decode(value).ok()?.into_owned();
            if decoded.is_empty() {
                return None;
            }
            params.insert(name.to_string(), decoded);
        }

        Some((handler, params))
    }
}
