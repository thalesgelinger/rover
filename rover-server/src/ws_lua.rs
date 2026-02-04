/// Lua DSL table factory for WebSocket endpoints.
///
/// Creates the `ws` table passed to `function api.chat.ws(ws)`.
/// Has two modes:
/// - Setup mode (route extraction): captures handler assignments via metamethods
/// - Runtime mode (handler execution): provides ws.send.*/ws.listen(topic) functionality
///
/// The ws table structure:
/// ```text
/// ws (table with metatable)
///   __ws_join       = nil/function  (captured via __newindex)
///   __ws_leave      = nil/function  (captured via __newindex)
///   listen (table with metatable)
///     __ws_handlers = {}  (event_name -> function)
///     __newindex    -> captures: function ws.listen.chat(msg, ctx, state)
///     __call        -> runtime: ws.listen("room:lobby") subscribes to topic
///   send (table with metatable)
///     __index       -> returns SendEventBuilder for any event name
///   error(code, msg) -> reject connection during join
/// ```

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, Table, Value};
use slab::Slab;

use crate::connection::Connection;
use crate::ws_manager::WsManager;

/// Shared WsManager wrapped for Lua app_data access.
pub type SharedWsManager = Rc<RefCell<WsManager>>;

/// Shared connections ref for send operations.
pub type SharedConnections = Rc<RefCell<Slab<Connection>>>;

/// Create the `ws` Lua table for a WebSocket endpoint.
///
/// During setup (route extraction), handler function assignments are captured.
/// The same table is stored as a RegistryKey and used at runtime for send/subscribe.
pub fn create_ws_table(lua: &Lua) -> mlua::Result<Table> {
    let ws = lua.create_table()?;

    // ── listen sub-table ──
    let listen = lua.create_table()?;
    let handlers = lua.create_table()?;
    listen.raw_set("__ws_handlers", handlers)?;

    let listen_meta = lua.create_table()?;

    // __newindex: captures `function ws.listen.chat(msg, ctx, state)`
    listen_meta.set(
        "__newindex",
        lua.create_function(|_lua, (tbl, key, value): (Table, String, Function)| {
            let handlers: Table = tbl.raw_get("__ws_handlers")?;
            handlers.raw_set(key, value)?;
            Ok(())
        })?,
    )?;

    // __call: runtime subscribe -- ws.listen("room:lobby")
    listen_meta.set(
        "__call",
        lua.create_function(|lua, (_self, topic): (Table, String)| {
            let mgr = lua
                .app_data_ref::<SharedWsManager>()
                .ok_or_else(|| mlua::Error::RuntimeError("WsManager not available".into()))?;
            let mut mgr = mgr.borrow_mut();
            let conn_idx = mgr.current_conn_idx;
            let topic_idx = mgr.subscribe(conn_idx, &topic);

            // Also update the connection's subscription list
            let conns = lua
                .app_data_ref::<SharedConnections>()
                .ok_or_else(|| mlua::Error::RuntimeError("Connections not available".into()))?;
            let mut conns = conns.borrow_mut();
            if let Some(conn) = conns.get_mut(conn_idx) {
                if let Some(ref mut ws) = conn.ws_data {
                    if !ws.subscriptions.contains(&topic_idx) {
                        ws.subscriptions.push(topic_idx);
                    }
                }
            }

            Ok(())
        })?,
    )?;

    let _ = listen.set_metatable(Some(listen_meta));
    ws.raw_set("listen", listen)?;

    // ── send sub-table ──
    let send = lua.create_table()?;
    let send_meta = lua.create_table()?;

    // __index: returns a SendEventBuilder for any event name
    // ws.send.chat -> SendEventBuilder { event_name: "chat" }
    send_meta.set(
        "__index",
        lua.create_function(|lua, (_self, event_name): (Table, String)| {
            create_send_event_builder(lua, event_name)
        })?,
    )?;

    let _ = send.set_metatable(Some(send_meta));
    ws.raw_set("send", send)?;

    // ── ws metatable: captures ws.join and ws.leave assignments ──
    let ws_meta = lua.create_table()?;
    ws_meta.set(
        "__newindex",
        lua.create_function(|_lua, (tbl, key, value): (Table, String, Value)| {
            match key.as_str() {
                "join" | "leave" => {
                    let internal_key = format!("__ws_{}", key);
                    tbl.raw_set(internal_key, value)?;
                }
                _ => {
                    tbl.raw_set(key, value)?;
                }
            }
            Ok(())
        })?,
    )?;
    let _ = ws.set_metatable(Some(ws_meta));

    // ── ws.error(code, msg) ──
    ws.raw_set(
        "error",
        lua.create_function(|_lua, (tbl, code, msg): (Table, u16, String)| {
            tbl.raw_set("__ws_error_code", code)?;
            tbl.raw_set("__ws_error_msg", msg)?;
            Ok(())
        })?,
    )?;

    Ok(ws)
}

/// Create a SendEventBuilder table for a specific event name.
///
/// When called with a data table: reply to sender (ws.send.ack { success = true })
/// When called with no args: returns a TargetSelector (ws.send.chat():all { ... })
fn create_send_event_builder(lua: &Lua, event_name: String) -> mlua::Result<Table> {
    let builder = lua.create_table()?;
    builder.raw_set("__event_name", event_name.as_str())?;

    let meta = lua.create_table()?;
    let event_for_call = event_name.clone();

    meta.set(
        "__call",
        lua.create_function(move |lua, (self_tbl, data): (Table, Value)| {
            match data {
                Value::Table(data_tbl) => {
                    // Direct call with data: reply to sender
                    // ws.send.ack { success = true }
                    send_to_current(lua, &event_for_call, &data_tbl)?;
                    Ok(Value::Nil)
                }
                Value::Nil => {
                    // No args: return TargetSelector
                    // ws.send.chat() -> TargetSelector
                    let event: String = self_tbl.raw_get("__event_name")?;
                    let selector = create_target_selector(lua, event)?;
                    Ok(Value::Table(selector))
                }
                _ => Err(mlua::Error::RuntimeError(
                    "ws.send.<event> expects a table or no arguments".into(),
                )),
            }
        })?,
    )?;

    let _ = builder.set_metatable(Some(meta));
    Ok(builder)
}

/// Create a TargetSelector table with :all(), :except(), :to() methods.
fn create_target_selector(lua: &Lua, event_name: String) -> mlua::Result<Table> {
    let selector = lua.create_table()?;
    selector.raw_set("__event_name", event_name.as_str())?;

    // :all(data) -> broadcast to all endpoint connections
    let event_all = event_name.clone();
    selector.raw_set(
        "all",
        lua.create_function(move |lua, (_self, data): (Table, Table)| {
            broadcast_all(lua, &event_all, &data, None)?;
            Ok(())
        })?,
    )?;

    // :except(data) -> broadcast to all except current connection
    let event_except = event_name.clone();
    selector.raw_set(
        "except",
        lua.create_function(move |lua, (_self, data): (Table, Table)| {
            let except_idx = {
                let mgr = lua
                    .app_data_ref::<SharedWsManager>()
                    .ok_or_else(|| mlua::Error::RuntimeError("WsManager not available".into()))?;
                mgr.borrow().current_conn_idx
            };
            broadcast_all(lua, &event_except, &data, Some(except_idx))?;
            Ok(())
        })?,
    )?;

    // :to(topic)(data) -> send to all topic subscribers
    let event_to = event_name.clone();
    selector.raw_set(
        "to",
        lua.create_function(move |lua, (_self, topic): (Table, String)| {
            let event_capture = event_to.clone();
            let topic_capture = topic.clone();
            let send_fn =
                lua.create_function(move |lua, data: Table| {
                    broadcast_to_topic(lua, &event_capture, &topic_capture, &data)?;
                    Ok(())
                })?;
            Ok(send_fn)
        })?,
    )?;

    Ok(selector)
}

// ── Send helper implementations ──

/// Serialize a Lua table as a JSON WebSocket message with injected "type" field.
/// Writes into the provided buffer.
fn serialize_event_json(
    event_name: &str,
    data: &Table,
    buf: &mut Vec<u8>,
) -> mlua::Result<()> {
    use crate::to_json::ToJson;

    // Serialize the data table to JSON first
    let mut data_json = Vec::with_capacity(128);
    data.to_json(&mut data_json)?;

    // If the data serialized as an object {...}, inject the type field
    if data_json.first() == Some(&b'{') && data_json.len() > 1 {
        buf.extend_from_slice(b"{\"type\":\"");
        // Escape event name (typically short ASCII, no escaping needed)
        for &b in event_name.as_bytes() {
            match b {
                b'"' => buf.extend_from_slice(b"\\\""),
                b'\\' => buf.extend_from_slice(b"\\\\"),
                _ => buf.push(b),
            }
        }
        buf.push(b'"');
        // If data had more fields, append them after the type
        if data_json.len() > 2 {
            // data_json is "{...}", skip opening {
            buf.push(b',');
            buf.extend_from_slice(&data_json[1..]);
        } else {
            buf.push(b'}');
        }
    } else {
        // Array or empty: wrap in {"type":"event","data":<value>}
        buf.extend_from_slice(b"{\"type\":\"");
        buf.extend_from_slice(event_name.as_bytes());
        buf.extend_from_slice(b"\",\"data\":");
        buf.extend_from_slice(&data_json);
        buf.push(b'}');
    }

    Ok(())
}

/// Build a WebSocket text frame from JSON payload, return as Bytes.
fn build_ws_text_frame(json: &[u8], frame_buf: &mut Vec<u8>) -> crate::Bytes {
    crate::ws_frame::write_frame(frame_buf, crate::ws_frame::WsOpcode::Text, json);
    crate::Bytes::from(std::mem::take(frame_buf))
}

/// Send a message to the current connection (reply to sender).
fn send_to_current(lua: &Lua, event_name: &str, data: &Table) -> mlua::Result<()> {
    let mgr = lua
        .app_data_ref::<SharedWsManager>()
        .ok_or_else(|| mlua::Error::RuntimeError("WsManager not available".into()))?;

    let conn_idx = mgr.borrow().current_conn_idx;

    // Serialize
    let mut json_buf = mgr.borrow_mut().get_frame_buf();
    serialize_event_json(event_name, data, &mut json_buf)?;

    let mut frame_buf = mgr.borrow_mut().get_frame_buf();
    let frame = build_ws_text_frame(&json_buf, &mut frame_buf);

    // Return json_buf to pool
    json_buf.clear();
    mgr.borrow_mut().return_frame_buf(json_buf);

    // Queue frame on current connection
    let conns = lua
        .app_data_ref::<SharedConnections>()
        .ok_or_else(|| mlua::Error::RuntimeError("Connections not available".into()))?;
    let mut conns = conns.borrow_mut();
    if let Some(conn) = conns.get_mut(conn_idx) {
        conn.queue_ws_frame(frame);
    }

    Ok(())
}

/// Broadcast a message to all connections on the current endpoint.
/// If `except` is Some, skip that connection index.
fn broadcast_all(
    lua: &Lua,
    event_name: &str,
    data: &Table,
    except: Option<usize>,
) -> mlua::Result<()> {
    let mgr = lua
        .app_data_ref::<SharedWsManager>()
        .ok_or_else(|| mlua::Error::RuntimeError("WsManager not available".into()))?;

    let endpoint_idx = mgr.borrow().current_endpoint_idx;

    // Serialize once
    let mut json_buf = mgr.borrow_mut().get_frame_buf();
    serialize_event_json(event_name, data, &mut json_buf)?;

    let mut frame_buf = mgr.borrow_mut().get_frame_buf();
    let frame = build_ws_text_frame(&json_buf, &mut frame_buf);

    json_buf.clear();
    mgr.borrow_mut().return_frame_buf(json_buf);

    // Get target list (clone to avoid borrow conflict)
    let targets: Vec<usize> = mgr
        .borrow()
        .get_endpoint_connections(endpoint_idx)
        .to_vec();

    // Queue frame on each target (Bytes::clone is O(1) refcount bump)
    let conns = lua
        .app_data_ref::<SharedConnections>()
        .ok_or_else(|| mlua::Error::RuntimeError("Connections not available".into()))?;
    let mut conns = conns.borrow_mut();
    for &target_idx in &targets {
        if except == Some(target_idx) {
            continue;
        }
        if let Some(conn) = conns.get_mut(target_idx) {
            if conn.is_websocket() {
                conn.queue_ws_frame(frame.clone());
            }
        }
    }

    Ok(())
}

/// Broadcast a message to all connections subscribed to a topic.
fn broadcast_to_topic(
    lua: &Lua,
    event_name: &str,
    topic: &str,
    data: &Table,
) -> mlua::Result<()> {
    let mgr = lua
        .app_data_ref::<SharedWsManager>()
        .ok_or_else(|| mlua::Error::RuntimeError("WsManager not available".into()))?;

    // Serialize once
    let mut json_buf = mgr.borrow_mut().get_frame_buf();
    serialize_event_json(event_name, data, &mut json_buf)?;

    let mut frame_buf = mgr.borrow_mut().get_frame_buf();
    let frame = build_ws_text_frame(&json_buf, &mut frame_buf);

    json_buf.clear();
    mgr.borrow_mut().return_frame_buf(json_buf);

    // Get topic members (clone to avoid borrow conflict)
    let members: Vec<usize> = mgr
        .borrow()
        .get_topic_members(topic)
        .map(|m| m.to_vec())
        .unwrap_or_default();

    // Queue frame on each member
    let conns = lua
        .app_data_ref::<SharedConnections>()
        .ok_or_else(|| mlua::Error::RuntimeError("Connections not available".into()))?;
    let mut conns = conns.borrow_mut();
    for &member_idx in &members {
        if let Some(conn) = conns.get_mut(member_idx) {
            if conn.is_websocket() {
                conn.queue_ws_frame(frame.clone());
            }
        }
    }

    Ok(())
}
