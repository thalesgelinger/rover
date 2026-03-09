use anyhow::{Context, Result};
use bytes::Bytes;
use mlua::{Lua, Value};
use rover_server::{HttpMethod, MiddlewareChain, Route, RouteTable, RoverResponse, ServerConfig};
use std::fs;
use std::path::{Path, PathBuf};

pub struct WebServerOptions {
    pub root_dir: PathBuf,
    pub host: String,
    pub port: u16,
}

impl Default for WebServerOptions {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from(".rover/web"),
            host: "127.0.0.1".to_string(),
            port: 4242,
        }
    }
}

pub fn serve_static(options: WebServerOptions) -> Result<()> {
    let root = options.root_dir;
    let files = collect_assets(&root)?;
    let lua = Lua::new();
    let routes = build_routes(&lua, files)?;

    let route_table = RouteTable {
        routes,
        ws_routes: vec![],
        error_handler: None,
    };

    let config = ServerConfig {
        port: options.port,
        host: options.host,
        log_level: "info".to_string(),
        docs: false,
        body_size_limit: None,
        cors_origin: None,
        cors_methods: "GET, HEAD".to_string(),
        cors_headers: "Content-Type".to_string(),
        cors_credentials: false,
    };

    rover_server::run(lua, route_table, config, None);
    Ok(())
}

struct Asset {
    route_path: String,
    body: Bytes,
    content_type: &'static str,
}

fn collect_assets(root: &Path) -> Result<Vec<Asset>> {
    let mut assets = Vec::new();
    collect_assets_recursive(root, root, &mut assets)?;

    if !assets.iter().any(|a| a.route_path == "/") {
        if let Some(index_asset) = assets.iter().find(|a| a.route_path == "/index.html") {
            assets.push(Asset {
                route_path: "/".to_string(),
                body: index_asset.body.clone(),
                content_type: "text/html; charset=utf-8",
            });
        }
    }

    Ok(assets)
}

fn collect_assets_recursive(root: &Path, current: &Path, assets: &mut Vec<Asset>) -> Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("failed reading assets directory {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_assets_recursive(root, &path, assets)?;
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("failed to normalize asset path {}", path.display()))?;

        let mut route = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
        if route == "/index.html" {
            route = "/index.html".to_string();
        }

        let bytes =
            fs::read(&path).with_context(|| format!("failed reading asset {}", path.display()))?;
        let content_type = guess_content_type(&path);
        assets.push(Asset {
            route_path: route,
            body: Bytes::from(bytes),
            content_type,
        });
    }

    Ok(())
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("lua") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn build_routes(lua: &Lua, assets: Vec<Asset>) -> Result<Vec<Route>> {
    let mut routes = Vec::new();

    for asset in assets {
        let body = asset.body.clone();
        let content_type = asset.content_type;
        let handler = lua.create_function(move |lua, _ctx: Value| {
            let response = RoverResponse {
                status: 200,
                body: body.clone(),
                content_type,
                headers: None,
            };
            lua.create_userdata(response)
        })?;

        routes.push(Route {
            method: HttpMethod::Get,
            pattern: Bytes::from(asset.route_path),
            param_names: vec![],
            handler,
            is_static: true,
            middlewares: MiddlewareChain::default(),
        });
    }

    Ok(routes)
}
