//! Integration tests for route precedence between API routes and static mounts

use bytes::Bytes;
use mlua::Lua;
use rover_server::{FastRouter, HttpMethod, Route, RouteMatch};

/// Create a dummy API route for testing
fn create_api_route(
    lua: &Lua,
    method: HttpMethod,
    path: &str,
    body: &str,
    is_static: bool,
) -> Route {
    let body = body.to_string();
    Route {
        method,
        pattern: Bytes::copy_from_slice(path.as_bytes()),
        param_names: if path.contains("{") {
            vec!["id".to_string()]
        } else {
            Vec::new()
        },
        handler: lua
            .create_function(move |lua, ()| lua.create_string(&body))
            .unwrap(),
        is_static,
        middlewares: Default::default(),
    }
}

/// Create a static mount route
fn create_static_mount_route(lua: &Lua, path_prefix: &str) -> Route {
    Route {
        method: HttpMethod::Get,
        pattern: Bytes::copy_from_slice(
            format!("{}/{{*__rover_mount_path}}", path_prefix).as_bytes(),
        ),
        param_names: vec!["__rover_mount_path".to_string()],
        handler: lua
            .create_function(|lua, ()| lua.create_string("static"))
            .unwrap(),
        is_static: false,
        middlewares: Default::default(),
    }
}

#[test]
fn exact_api_route_should_take_precedence_over_static_mount() {
    let lua = Lua::new();

    // API route: GET /assets/health
    let api_route = create_api_route(&lua, HttpMethod::Get, "/assets/health", "api-health", true);

    // Static mount: GET /assets/{*__rover_mount_path}
    let static_route = create_static_mount_route(&lua, "/assets");

    // Test with static mount registered first (order shouldn't matter)
    let router = FastRouter::from_routes(vec![static_route.clone(), api_route.clone()]).unwrap();

    // Exact match should hit API route
    match router.match_route(HttpMethod::Get, "/assets/health") {
        RouteMatch::Found {
            handler, params, ..
        } => {
            let body: mlua::String = handler.call(()).expect("call handler");
            assert_eq!(body.to_str().expect("body str"), "api-health");
            assert!(params.is_empty(), "exact route should have no params");
        }
        _ => panic!("expected exact API route to match, not static mount"),
    }

    // Unmatched path should hit static mount
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
        _ => panic!("expected static mount to match"),
    }
}

#[test]
fn dynamic_api_route_should_take_precedence_over_static_mount() {
    let lua = Lua::new();

    // Dynamic API route: GET /api/{id}
    let api_route = Route {
        method: HttpMethod::Get,
        pattern: Bytes::from_static(b"/api/{id}"),
        param_names: vec!["id".to_string()],
        handler: lua
            .create_function(|lua, ()| lua.create_string("dynamic-api"))
            .unwrap(),
        is_static: false,
        middlewares: Default::default(),
    };

    // Static mount: GET /api/{*__rover_mount_path}
    let static_route = create_static_mount_route(&lua, "/api");

    // Test with different registration orders
    for routes in [
        vec![static_route.clone(), api_route.clone()],
        vec![api_route.clone(), static_route.clone()],
    ] {
        let router = FastRouter::from_routes(routes).unwrap();

        // Dynamic API route should match first
        match router.match_route(HttpMethod::Get, "/api/users") {
            RouteMatch::Found {
                handler, params, ..
            } => {
                let body: mlua::String = handler.call(()).expect("call handler");
                assert_eq!(body.to_str().expect("body str"), "dynamic-api");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, Bytes::from_static(b"id"));
                assert_eq!(params[0].1, Bytes::from_static(b"users"));
            }
            _ => panic!("expected dynamic API route to match"),
        }

        // Static mount should serve files
        match router.match_route(HttpMethod::Get, "/api/static/file.txt") {
            RouteMatch::Found {
                handler, params, ..
            } => {
                let body: mlua::String = handler.call(()).expect("call handler");
                assert_eq!(body.to_str().expect("body str"), "static");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].1, Bytes::from_static(b"static/file.txt"));
            }
            _ => panic!("expected static mount to match"),
        }
    }
}

#[test]
fn multiple_api_routes_at_same_prefix_should_work_with_static_mount() {
    let lua = Lua::new();

    // Multiple API routes at /assets/*
    let health_route = create_api_route(&lua, HttpMethod::Get, "/assets/health", "health", true);
    let config_route = create_api_route(&lua, HttpMethod::Get, "/assets/config", "config", true);
    let dynamic_route = Route {
        method: HttpMethod::Get,
        pattern: Bytes::from_static(b"/assets/{id}"),
        param_names: vec!["id".to_string()],
        handler: lua
            .create_function(|lua, ()| lua.create_string("dynamic"))
            .unwrap(),
        is_static: false,
        middlewares: Default::default(),
    };

    // Static mount at /assets/*
    let static_route = create_static_mount_route(&lua, "/assets");

    let router = FastRouter::from_routes(vec![
        static_route,
        health_route,
        config_route,
        dynamic_route,
    ])
    .unwrap();

    // Exact routes should match
    match router.match_route(HttpMethod::Get, "/assets/health") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "health");
        }
        _ => panic!("expected health route"),
    }

    match router.match_route(HttpMethod::Get, "/assets/config") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "config");
        }
        _ => panic!("expected config route"),
    }

    // Dynamic route should match for non-exact paths
    match router.match_route(HttpMethod::Get, "/assets/123") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "dynamic");
        }
        _ => panic!("expected dynamic route"),
    }

    // Static mount should serve files for unmatched paths
    match router.match_route(HttpMethod::Get, "/assets/images/logo.png") {
        RouteMatch::Found {
            handler, params, ..
        } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "static");
            assert_eq!(params[0].1, Bytes::from_static(b"images/logo.png"));
        }
        _ => panic!("expected static mount"),
    }
}

#[test]
fn post_request_to_static_mount_path_should_return_method_not_allowed() {
    let lua = Lua::new();

    // Static mount only supports GET
    let static_route = Route {
        method: HttpMethod::Get,
        pattern: Bytes::from_static(b"/assets/{*__rover_mount_path}"),
        param_names: vec!["__rover_mount_path".to_string()],
        handler: lua.create_function(|_, ()| Ok(())).unwrap(),
        is_static: false,
        middlewares: Default::default(),
    };

    let router = FastRouter::from_routes(vec![static_route]).unwrap();

    // POST to static mount should return 405
    match router.match_route(HttpMethod::Post, "/assets/app.js") {
        RouteMatch::MethodNotAllowed { allowed } => {
            assert!(allowed.contains(&HttpMethod::Get));
            assert!(allowed.contains(&HttpMethod::Head));
            assert!(allowed.contains(&HttpMethod::Options));
            assert!(!allowed.contains(&HttpMethod::Post));
        }
        _ => panic!("expected 405 Method Not Allowed"),
    }
}

#[test]
fn nested_static_mounts_should_work_correctly() {
    let lua = Lua::new();

    // Static mount at /assets
    let assets_mount = create_static_mount_route(&lua, "/assets");

    // Static mount at /uploads
    let uploads_mount = Route {
        method: HttpMethod::Get,
        pattern: Bytes::from_static(b"/uploads/{*__rover_mount_path}"),
        param_names: vec!["__rover_mount_path".to_string()],
        handler: lua
            .create_function(|lua, ()| lua.create_string("uploads-static"))
            .unwrap(),
        is_static: false,
        middlewares: Default::default(),
    };

    // API route at /assets/health (should take precedence)
    let health_route =
        create_api_route(&lua, HttpMethod::Get, "/assets/health", "api-health", true);

    let router = FastRouter::from_routes(vec![assets_mount, uploads_mount, health_route]).unwrap();

    // API route should match
    match router.match_route(HttpMethod::Get, "/assets/health") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "api-health");
        }
        _ => panic!("expected API route"),
    }

    // Assets static mount should match
    let result = router.match_route(HttpMethod::Get, "/assets/app.js");
    match result {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "static");
        }
        RouteMatch::MethodNotAllowed { .. } => panic!("got MethodNotAllowed"),
        RouteMatch::NotFound => panic!("got NotFound"),
    }

    // Uploads static mount should match
    match router.match_route(HttpMethod::Get, "/uploads/image.png") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "uploads-static");
        }
        _ => panic!("expected uploads static mount"),
    }
}

#[test]
fn head_request_should_match_static_mount() {
    let lua = Lua::new();

    let static_route = create_static_mount_route(&lua, "/assets");

    let router = FastRouter::from_routes(vec![static_route]).unwrap();

    // HEAD should match static mount (and be treated as HEAD, not converted to GET handler)
    match router.match_route(HttpMethod::Head, "/assets/app.js") {
        RouteMatch::Found { is_head, .. } => {
            assert!(is_head, "HEAD request should be marked as is_head");
        }
        _ => panic!("expected static mount to match HEAD"),
    }
}

#[test]
fn deep_nested_paths_in_static_mount() {
    let lua = Lua::new();

    let static_route = create_static_mount_route(&lua, "/assets");

    let router = FastRouter::from_routes(vec![static_route]).unwrap();

    // Deep nested paths should work
    match router.match_route(HttpMethod::Get, "/assets/js/lib/utils/helpers.js") {
        RouteMatch::Found {
            handler, params, ..
        } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "static");
            assert_eq!(params[0].1, Bytes::from_static(b"js/lib/utils/helpers.js"));
        }
        _ => panic!("expected static mount to match deep nested path"),
    }
}

#[test]
fn api_route_with_same_prefix_as_static_mount_but_different_method() {
    let lua = Lua::new();

    // POST API route at /assets/upload
    let upload_route = Route {
        method: HttpMethod::Post,
        pattern: Bytes::from_static(b"/assets/upload"),
        param_names: Vec::new(),
        handler: lua
            .create_function(|lua, ()| lua.create_string("upload-api"))
            .unwrap(),
        is_static: true,
        middlewares: Default::default(),
    };

    // GET static mount at /assets
    let static_route = create_static_mount_route(&lua, "/assets");

    let router = FastRouter::from_routes(vec![static_route, upload_route]).unwrap();

    // GET /assets/upload should return MethodNotAllowed because the path is "owned" by the API
    // (even though the API only supports POST). Static mount is only a fallback for paths
    // with no API routes at all.
    match router.match_route(HttpMethod::Get, "/assets/upload") {
        RouteMatch::MethodNotAllowed { allowed } => {
            assert!(allowed.contains(&HttpMethod::Post));
            assert!(allowed.contains(&HttpMethod::Options));
            assert!(!allowed.contains(&HttpMethod::Get));
        }
        _ => panic!("expected MethodNotAllowed for GET /assets/upload (path owned by API)"),
    }

    // POST /assets/upload should match API route
    match router.match_route(HttpMethod::Post, "/assets/upload") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "upload-api");
        }
        _ => panic!("expected API route for POST"),
    }

    // GET /assets/other.js should fall through to static mount (no API route at this path)
    match router.match_route(HttpMethod::Get, "/assets/other.js") {
        RouteMatch::Found { handler, .. } => {
            let body: mlua::String = handler.call(()).unwrap();
            assert_eq!(body.to_str().unwrap(), "static");
        }
        _ => panic!("expected static mount for unrelated path"),
    }
}
