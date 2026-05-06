use mlua::{Lua, Table, Value};

use crate::cookie;
use crate::env::{create_config_module, create_env_module};
use crate::html::create_html_module;
use crate::http::create_http_module;
use crate::io::create_io_module;
use crate::server::AppServer;
use crate::session;
use crate::ws_client;
use rover_auth::create_auth_module;
use rover_db::create_db_module;
use rover_ui::register_ui_module;

pub fn install_rover_surface(lua: &Lua) -> mlua::Result<Table> {
    let rover = lua.create_table()?;
    register_server_factory(lua, &rover)?;
    install_extra_modules(lua, &rover)?;
    register_ui_module(lua, &rover)?;
    Ok(rover)
}

pub fn install_extra_modules(lua: &Lua, rover: &Table) -> mlua::Result<()> {
    let guard = create_guard_module(lua)?;
    rover.set("guard", guard)?;

    let env_module = create_env_module(lua)?;
    rover.set("env", env_module)?;

    let config_module = create_config_module(lua)?;
    rover.set("config", config_module)?;

    let cookie_module = cookie::create_cookie_module(lua)?;
    rover.set("cookie", cookie_module)?;

    let auth_module = create_auth_module(lua)?;
    rover.set("auth", auth_module)?;

    let session_module = session::create_session_module(lua)?;
    rover.set("session", session_module)?;

    let io_module = create_io_module(lua)?;
    lua.globals().set("io", io_module)?;

    let debug_module: Table = lua
        .load(include_str!("debug.lua"))
        .set_name("debug.lua")
        .eval()?;
    lua.globals().set("debug", debug_module)?;

    let http_module = create_http_module(lua)?;
    rover.set("http", http_module)?;

    rover.set(
        "ws_client",
        lua.create_function(|lua, (url, opts): (String, Option<Table>)| {
            ws_client::create_ws_client(lua, url, opts)
        })?,
    )?;

    let html_module = create_html_module(lua)?;
    rover.set("html", html_module)?;

    let db_module = create_db_module(lua)?;
    rover.set("db", db_module)?;

    Ok(())
}

fn register_server_factory(lua: &Lua, rover: &Table) -> mlua::Result<()> {
    rover.set(
        "server",
        lua.create_function(|lua, opts: Table| {
            let server = lua.create_server(opts)?;
            Ok(server)
        })?,
    )
}

fn create_guard_module(lua: &Lua) -> mlua::Result<Table> {
    let guard: Table = lua
        .load(include_str!("guard.lua"))
        .set_name("guard.lua")
        .eval()?;
    let guard_meta = lua.create_table()?;
    guard_meta.set("__index", guard.clone())?;
    guard_meta.set(
        "__call",
        lua.create_function(|lua, (data, schema): (Value, Value)| {
            use crate::guard::{ValidationErrors, validate_table};

            let data_table = match data {
                Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "First argument must be a table".to_string(),
                    ));
                }
            };

            let schema_table = match schema {
                Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "Second argument must be a table".to_string(),
                    ));
                }
            };

            match validate_table(lua, &data_table, &schema_table, "") {
                Ok(validated) => Ok(validated),
                Err(errors) => {
                    let validation_errors = ValidationErrors::new(errors);
                    Err(mlua::Error::ExternalError(std::sync::Arc::new(
                        validation_errors,
                    )))
                }
            }
        })?,
    )?;
    let _ = guard.set_metatable(Some(guard_meta));
    Ok(guard)
}
