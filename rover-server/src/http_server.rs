use anyhow::Result;
use mlua::Lua;
use std::net::SocketAddr;

use crate::{Route, ServerConfig, event_loop::EventLoop};

pub fn run_server(
    lua: Lua,
    routes: Vec<Route>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    addr: SocketAddr,
) -> Result<()> {
    let mut event_loop = EventLoop::new(lua, routes, config, openapi_spec, addr)?;
    event_loop.run()
}
