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
            if let Some(conn) = conns.get_mut(conn_idx)
                && let Some(ref mut ws) = conn.ws_data
                && !ws.subscriptions.contains(&topic_idx)
            {
                ws.subscriptions.push(topic_idx);
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
            let send_fn = lua.create_function(move |lua, data: Table| {
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
fn serialize_event_json(event_name: &str, data: &Table, buf: &mut Vec<u8>) -> mlua::Result<()> {
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
    let targets: Vec<usize> = mgr.borrow().get_endpoint_connections(endpoint_idx).to_vec();

    // Queue frame on each target (Bytes::clone is O(1) refcount bump)
    let conns = lua
        .app_data_ref::<SharedConnections>()
        .ok_or_else(|| mlua::Error::RuntimeError("Connections not available".into()))?;
    let mut conns = conns.borrow_mut();
    for &target_idx in &targets {
        if except == Some(target_idx) {
            continue;
        }
        if let Some(conn) = conns.get_mut(target_idx)
            && conn.is_websocket()
        {
            conn.queue_ws_frame(frame.clone());
        }
    }

    Ok(())
}

/// Broadcast a message to all connections subscribed to a topic.
fn broadcast_to_topic(lua: &Lua, event_name: &str, topic: &str, data: &Table) -> mlua::Result<()> {
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
        if let Some(conn) = conns.get_mut(member_idx)
            && conn.is_websocket()
        {
            conn.queue_ws_frame(frame.clone());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::{Lua, ObjectLike};

    fn make_data_table(lua: &Lua, fields: &[(&str, &str)]) -> Table {
        let tbl = lua.create_table().unwrap();
        for (k, v) in fields {
            tbl.raw_set(*k, *v).unwrap();
        }
        tbl
    }

    #[test]
    fn test_serialize_event_json_injects_type() {
        let lua = Lua::new();
        let data = make_data_table(&lua, &[("message", "hello"), ("user", "alice")]);
        let mut buf = Vec::new();
        serialize_event_json("chat", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "chat");
        assert_eq!(json["message"], "hello");
        assert_eq!(json["user"], "alice");
    }

    #[test]
    fn test_serialize_event_json_empty_object() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let mut buf = Vec::new();
        serialize_event_json("ping", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "ping");
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_serialize_event_json_escapes_event_name() {
        let lua = Lua::new();
        let data = make_data_table(&lua, &[("msg", "test")]);
        let mut buf = Vec::new();
        serialize_event_json("event\"with\"quotes", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "event\"with\"quotes");
    }

    #[test]
    fn test_serialize_event_json_escapes_backslash_in_event() {
        let lua = Lua::new();
        let data = make_data_table(&lua, &[("msg", "test")]);
        let mut buf = Vec::new();
        serialize_event_json("event\\with\\backslash", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "event\\with\\backslash");
    }

    #[test]
    fn test_serialize_event_json_preserves_nested_object() {
        let lua = Lua::new();
        let inner = lua.create_table().unwrap();
        inner.raw_set("x", 1).unwrap();
        inner.raw_set("y", 2).unwrap();
        let data = lua.create_table().unwrap();
        data.raw_set("coords", inner).unwrap();

        let mut buf = Vec::new();
        serialize_event_json("position", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "position");
        assert_eq!(json["coords"]["x"], 1);
        assert_eq!(json["coords"]["y"], 2);
    }

    #[test]
    fn test_serialize_event_json_array_wrapped() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.raw_set(1, "a").unwrap();
        data.raw_set(2, "b").unwrap();
        data.raw_set(3, "c").unwrap();

        let mut buf = Vec::new();
        serialize_event_json("list", &data, &mut buf).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["type"], "list");
        assert_eq!(json["data"], serde_json::json!(["a", "b", "c"]));
    }

    #[test]
    fn test_create_ws_table_has_listen_subtable() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        assert!(ws.raw_get::<Table>("listen").is_ok());
    }

    #[test]
    fn test_create_ws_table_has_send_subtable() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        assert!(ws.raw_get::<Table>("send").is_ok());
    }

    #[test]
    fn test_create_ws_table_has_error_fn() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        assert!(ws.raw_get::<Function>("error").is_ok());
    }

    #[test]
    fn test_ws_table_captures_join_handler() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();

        let join_fn = lua.create_function(|_, ()| Ok(())).unwrap();
        ws.set("join", join_fn).unwrap();

        let captured: Function = ws.raw_get("__ws_join").unwrap();
        assert!(captured.call::<()>(()).is_ok());
    }

    #[test]
    fn test_ws_table_captures_leave_handler() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();

        let leave_fn = lua.create_function(|_, ()| Ok(())).unwrap();
        ws.set("leave", leave_fn).unwrap();

        let captured: Function = ws.raw_get("__ws_leave").unwrap();
        assert!(captured.call::<()>(()).is_ok());
    }

    #[test]
    fn test_ws_table_listen_captures_event_handler() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let handler = lua.create_function(|_, ()| Ok(())).unwrap();
        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();
        assert!(captured.call::<()>(()).is_ok());
    }

    #[test]
    fn test_ws_table_listen_captures_message_fallback() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let handler = lua.create_function(|_, ()| Ok(())).unwrap();
        listen.set("message", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("message").unwrap();
        assert!(captured.call::<()>(()).is_ok());
    }

    #[test]
    fn test_ws_error_sets_error_code_and_msg() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();

        let error_fn: Function = ws.raw_get("error").unwrap();
        error_fn
            .call::<()>((ws.clone(), 4003u16, "Invalid token"))
            .unwrap();

        let code: u16 = ws.raw_get("__ws_error_code").unwrap();
        let msg: String = ws.raw_get("__ws_error_msg").unwrap();
        assert_eq!(code, 4003);
        assert_eq!(msg, "Invalid token");
    }

    #[test]
    fn test_send_event_builder_returns_target_selector_on_nil() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let send: Table = ws.raw_get("send").unwrap();

        let builder: Table = send.get("chat").unwrap();
        let selector: Table = builder.call(()).unwrap();

        assert!(selector.raw_get::<Function>("all").is_ok());
        assert!(selector.raw_get::<Function>("except").is_ok());
        assert!(selector.raw_get::<Function>("to").is_ok());
    }

    #[test]
    fn test_target_selector_has_all_except_to_methods() {
        let lua = Lua::new();
        let selector = create_target_selector(&lua, "test".to_string()).unwrap();

        assert!(selector.raw_get::<Function>("all").is_ok());
        assert!(selector.raw_get::<Function>("except").is_ok());
        assert!(selector.raw_get::<Function>("to").is_ok());
    }

    #[test]
    fn test_target_selector_to_returns_function() {
        let lua = Lua::new();
        let selector = create_target_selector(&lua, "test".to_string()).unwrap();

        let to_fn: Function = selector.raw_get("to").unwrap();
        let result: mlua::Value = to_fn.call((selector.clone(), "room:lobby")).unwrap();
        assert!(matches!(result, mlua::Value::Function(_)));
    }

    #[test]
    fn test_event_dispatch_routing_typed_handler() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let chat_called = Rc::new(RefCell::new(false));
        let chat_called_clone = chat_called.clone();
        let handler = lua
            .create_function(move |_, ()| {
                *chat_called_clone.borrow_mut() = true;
                Ok(())
            })
            .unwrap();
        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();
        captured.call::<()>(()).unwrap();

        assert!(*chat_called.borrow());
    }

    #[test]
    fn test_event_dispatch_routing_fallback_handler() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let message_called = Rc::new(RefCell::new(false));
        let message_called_clone = message_called.clone();
        let handler = lua
            .create_function(move |_, ()| {
                *message_called_clone.borrow_mut() = true;
                Ok(())
            })
            .unwrap();
        listen.set("message", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("message").unwrap();
        captured.call::<()>(()).unwrap();

        assert!(*message_called.borrow());
    }

    #[test]
    fn test_handler_receives_msg_ctx_state_arguments() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let received_msg = Rc::new(RefCell::new(None::<String>));
        let received_ctx_has_conn = Rc::new(RefCell::new(false));
        let received_state = Rc::new(RefCell::new(None::<i64>));

        let msg_clone = received_msg.clone();
        let ctx_clone = received_ctx_has_conn.clone();
        let state_clone = received_state.clone();

        let handler = lua
            .create_function(move |_, (msg, ctx, state): (Table, Table, Value)| {
                if let Ok(text) = msg.raw_get::<String>("text") {
                    *msg_clone.borrow_mut() = Some(text);
                }

                if ctx.raw_get::<Value>("conn").is_ok() {
                    *ctx_clone.borrow_mut() = true;
                }

                if let Value::Integer(n) = state {
                    *state_clone.borrow_mut() = Some(n);
                }

                Ok(Value::Integer(42))
            })
            .unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let msg = lua.create_table().unwrap();
        msg.raw_set("text", "hello").unwrap();

        let ctx = lua.create_table().unwrap();
        ctx.raw_set("conn", 1).unwrap();

        let state = Value::Integer(10);

        let result = captured.call::<Value>((msg, ctx, state)).unwrap();

        assert_eq!(*received_msg.borrow(), Some("hello".to_string()));
        assert!(*received_ctx_has_conn.borrow());
        assert_eq!(*received_state.borrow(), Some(10));
        assert!(matches!(result, Value::Integer(42)));
    }

    #[test]
    fn test_handler_return_value_updates_state() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let empty_table = lua.create_table().unwrap();
        let handler = lua
            .create_function(move |lua, ()| {
                let tbl = lua.create_table().unwrap();
                Ok(Value::Table(tbl))
            })
            .unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let result = captured.call::<Value>(()).unwrap();

        assert!(matches!(result, Value::Table(_)));

        let new_state = lua.create_table().unwrap();
        new_state.raw_set("count", 5).unwrap();

        let handler2 = lua
            .create_function(move |_, ()| Ok(Value::Table(new_state.clone())))
            .unwrap();

        listen.set("update", handler2).unwrap();

        let handlers2: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured2: Function = handlers2.raw_get("update").unwrap();

        let result2 = captured2.call::<Value>(()).unwrap();
        if let Value::Table(tbl) = result2 {
            assert_eq!(tbl.raw_get::<i64>("count").unwrap(), 5);
        } else {
            panic!("Expected table return value");
        }
    }

    #[test]
    fn test_handler_can_return_nil_to_keep_state_unchanged() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let handler = lua.create_function(|_, ()| Ok(Value::Nil)).unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let result = captured.call::<Value>(()).unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn test_event_routing_by_type_field() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let chat_called = Rc::new(RefCell::new(false));
        let ping_called = Rc::new(RefCell::new(false));

        let chat_clone = chat_called.clone();
        let ping_clone = ping_called.clone();

        let chat_handler = lua
            .create_function(move |_, ()| {
                *chat_clone.borrow_mut() = true;
                Ok(Value::Nil)
            })
            .unwrap();

        let ping_handler = lua
            .create_function(move |_, ()| {
                *ping_clone.borrow_mut() = true;
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("chat", chat_handler).unwrap();
        listen.set("ping", ping_handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();

        let chat_fn: Function = handlers.raw_get("chat").unwrap();
        chat_fn.call::<()>(()).unwrap();
        assert!(*chat_called.borrow());
        assert!(!*ping_called.borrow());

        let ping_fn: Function = handlers.raw_get("ping").unwrap();
        ping_fn.call::<()>(()).unwrap();
        assert!(*ping_called.borrow());
    }

    #[test]
    fn test_fallback_handler_for_messages_without_type() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let message_called = Rc::new(RefCell::new(false));
        let message_clone = message_called.clone();

        let message_handler = lua
            .create_function(move |_, ()| {
                *message_clone.borrow_mut() = true;
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("message", message_handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();

        let message_fn: Function = handlers.raw_get("message").unwrap();
        message_fn.call::<()>(()).unwrap();
        assert!(*message_called.borrow());
    }

    #[test]
    fn test_typed_handler_takes_precedence_over_fallback() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let chat_called = Rc::new(RefCell::new(false));
        let message_called = Rc::new(RefCell::new(false));

        let chat_clone = chat_called.clone();
        let message_clone = message_called.clone();

        let chat_handler = lua
            .create_function(move |_, ()| {
                *chat_clone.borrow_mut() = true;
                Ok(Value::Nil)
            })
            .unwrap();

        let message_handler = lua
            .create_function(move |_, ()| {
                *message_clone.borrow_mut() = true;
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("chat", chat_handler).unwrap();
        listen.set("message", message_handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();

        let chat_fn: Function = handlers.raw_get("chat").unwrap();
        chat_fn.call::<()>(()).unwrap();

        assert!(*chat_called.borrow());
        assert!(!*message_called.borrow());
    }

    #[test]
    fn test_handler_can_access_message_fields() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let received_fields = Rc::new(RefCell::new(Vec::new()));
        let fields_clone = received_fields.clone();

        let handler = lua
            .create_function(move |_, (msg, _ctx, _state): (Table, Table, Value)| {
                if let Ok(text) = msg.raw_get::<String>("text") {
                    fields_clone.borrow_mut().push(format!("text={}", text));
                }
                if let Ok(count) = msg.raw_get::<i64>("count") {
                    fields_clone.borrow_mut().push(format!("count={}", count));
                }
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let msg = lua.create_table().unwrap();
        msg.raw_set("text", "hello").unwrap();
        msg.raw_set("count", 42).unwrap();

        let ctx = lua.create_table().unwrap();
        let state = Value::Nil;

        captured.call::<()>((msg, ctx, state)).unwrap();

        let fields = received_fields.borrow();
        assert!(fields.contains(&"text=hello".to_string()));
        assert!(fields.contains(&"count=42".to_string()));
    }

    #[test]
    fn test_handler_can_access_context_fields() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let received_conn = Rc::new(RefCell::new(None::<i64>));
        let conn_clone = received_conn.clone();

        let handler = lua
            .create_function(move |_, (_msg, ctx, _state): (Table, Table, Value)| {
                if let Ok(conn) = ctx.raw_get::<i64>("conn") {
                    *conn_clone.borrow_mut() = Some(conn);
                }
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let msg = lua.create_table().unwrap();
        let ctx = lua.create_table().unwrap();
        ctx.raw_set("conn", 123).unwrap();
        let state = Value::Nil;

        captured.call::<()>((msg, ctx, state)).unwrap();

        assert_eq!(*received_conn.borrow(), Some(123));
    }

    #[test]
    fn test_handler_can_access_current_state() {
        let lua = Lua::new();
        let ws = create_ws_table(&lua).unwrap();
        let listen: Table = ws.raw_get("listen").unwrap();

        let received_state = Rc::new(RefCell::new(None::<i64>));
        let state_clone = received_state.clone();

        let handler = lua
            .create_function(move |_, (_msg, _ctx, state): (Table, Table, Value)| {
                if let Value::Integer(n) = state {
                    *state_clone.borrow_mut() = Some(n);
                }
                Ok(Value::Nil)
            })
            .unwrap();

        listen.set("chat", handler).unwrap();

        let handlers: Table = listen.raw_get("__ws_handlers").unwrap();
        let captured: Function = handlers.raw_get("chat").unwrap();

        let msg = lua.create_table().unwrap();
        let ctx = lua.create_table().unwrap();
        let state = Value::Integer(99);

        captured.call::<()>((msg, ctx, state)).unwrap();

        assert_eq!(*received_state.borrow(), Some(99));
    }
}
