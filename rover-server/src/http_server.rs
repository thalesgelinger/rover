use anyhow::Result;
use mlua::{Lua, RegistryKey};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::{event_loop::EventLoop, Route, ServerConfig, WsRoute};

pub fn run_server(
    lua: Lua,
    routes: Vec<Route>,
    ws_routes: Vec<WsRoute>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    addr: SocketAddr,
    error_handler: Option<Arc<RegistryKey>>,
) -> Result<()> {
    let mut event_loop = EventLoop::new(
        lua,
        routes,
        ws_routes,
        config,
        openapi_spec,
        addr,
        error_handler,
    )?;
    event_loop.run()
}
