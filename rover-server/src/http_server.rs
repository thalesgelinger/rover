use anyhow::Result;
use mlua::Lua;
use std::net::SocketAddr;

use crate::{Route, ServerConfig, WsRoute, event_loop::EventLoop};

pub fn run_server(
    lua: Lua,
    routes: Vec<Route>,
    ws_routes: Vec<WsRoute>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    addr: SocketAddr,
) -> Result<()> {
    let mut event_loop = EventLoop::new(lua, routes, ws_routes, config, openapi_spec, addr)?;
    event_loop.run()
}
