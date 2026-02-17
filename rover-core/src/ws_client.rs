use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use mlua::prelude::*;
use mlua::{Function, LuaSerdeExt, RegistryKey, Table, Value};
use serde_json::{Map, Value as JsonValue};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{HeaderName, HeaderValue};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

#[derive(Clone, Debug)]
pub struct WsClientOptions {
    pub headers: HashMap<String, String>,
    pub protocols: Vec<String>,
    pub handshake_timeout_ms: u64,
    pub max_message_bytes: usize,
    pub auto_pong: bool,
    pub reconnect: ReconnectOptions,
    pub tls: TlsOptions,
}

#[derive(Clone, Debug)]
pub struct ReconnectOptions {
    pub enabled: bool,
    pub min_ms: u64,
    pub max_ms: u64,
    pub factor: f64,
    pub jitter: bool,
    pub max_attempts: u32,
}

#[derive(Clone, Debug)]
pub struct TlsOptions {
    pub roots: String,
    pub ca_file: Option<String>,
    pub insecure: bool,
    pub pin_sha256: Vec<String>,
}

impl Default for WsClientOptions {
    fn default() -> Self {
        Self {
            headers: HashMap::new(),
            protocols: Vec::new(),
            handshake_timeout_ms: 10_000,
            max_message_bytes: 4 * 1024 * 1024,
            auto_pong: true,
            reconnect: ReconnectOptions {
                enabled: false,
                min_ms: 250,
                max_ms: 10_000,
                factor: 2.0,
                jitter: true,
                max_attempts: 0,
            },
            tls: TlsOptions {
                roots: "bundled".to_string(),
                ca_file: None,
                insecure: false,
                pin_sha256: Vec::new(),
            },
        }
    }
}

#[derive(Clone)]
enum OutboundMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
}

type ClientSocket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

struct WsClientInner {
    url: String,
    opts: WsClientOptions,
    socket: Option<ClientSocket>,
    state_key: Option<RegistryKey>,
    send_queue: VecDeque<OutboundMessage>,
    connected: bool,
    negotiated_protocol: Option<String>,
}

impl WsClientInner {
    fn new(url: String, opts: WsClientOptions) -> Self {
        Self {
            url,
            opts,
            socket: None,
            state_key: None,
            send_queue: VecDeque::with_capacity(16),
            connected: false,
            negotiated_protocol: None,
        }
    }
}

pub fn create_ws_client(lua: &Lua, url: String, opts: Option<Table>) -> LuaResult<Table> {
    let options = parse_ws_client_options(opts)?;
    let inner = Rc::new(RefCell::new(WsClientInner::new(url, options)));

    let client = lua.create_table()?;
    client.set("join", Value::Nil)?;
    client.set("leave", Value::Nil)?;
    client.set("error", Value::Nil)?;

    let listen_table = create_listen_table(lua)?;
    client.set("listen", listen_table)?;

    let send_table = create_send_table(lua, inner.clone())?;
    client.set("send", send_table)?;

    {
        let inner = inner.clone();
        client.set(
            "connect",
            lua.create_function(move |lua, this: Table| connect_client(lua, &this, &inner))?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "pump",
            lua.create_function(move |lua, (this, timeout_ms): (Table, Option<u64>)| {
                pump_client(lua, &this, &inner, timeout_ms)
            })?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "run",
            lua.create_function(move |lua, this: Table| run_client(lua, &this, &inner))?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "close",
            lua.create_function(
                move |lua, (this, code, reason): (Table, Option<u16>, Option<String>)| {
                    close_client(lua, &this, &inner, code, reason)
                },
            )?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "is_connected",
            lua.create_function(move |_lua, _this: Table| Ok(inner.borrow().connected))?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "send_text",
            lua.create_function(move |_lua, (_this, text): (Table, String)| {
                inner
                    .borrow_mut()
                    .send_queue
                    .push_back(OutboundMessage::Text(text));
                Ok(())
            })?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "send_binary",
            lua.create_function(move |_lua, (_this, bytes): (Table, mlua::String)| {
                inner
                    .borrow_mut()
                    .send_queue
                    .push_back(OutboundMessage::Binary(bytes.as_bytes().to_vec()));
                Ok(())
            })?,
        )?;
    }

    {
        let inner = inner.clone();
        client.set(
            "ping",
            lua.create_function(
                move |_lua, (_this, payload): (Table, Option<mlua::String>)| {
                    let payload = payload.map(|p| p.as_bytes().to_vec()).unwrap_or_default();
                    inner
                        .borrow_mut()
                        .send_queue
                        .push_back(OutboundMessage::Ping(payload));
                    Ok(())
                },
            )?,
        )?;
    }

    Ok(client)
}

fn create_listen_table(lua: &Lua) -> LuaResult<Table> {
    let listen = lua.create_table()?;
    let handlers = lua.create_table()?;
    listen.raw_set("__handlers", handlers)?;

    let meta = lua.create_table()?;
    meta.set(
        "__newindex",
        lua.create_function(|_lua, (tbl, key, value): (Table, Value, Value)| {
            let handlers: Table = tbl.raw_get("__handlers")?;
            match (key, value) {
                (Value::String(name), Value::Function(func)) => {
                    handlers.raw_set(name, func)?;
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "ws.listen handlers must be function values".to_string(),
                    ));
                }
            }
            Ok(())
        })?,
    )?;

    let _ = listen.set_metatable(Some(meta));
    Ok(listen)
}

fn create_send_table(lua: &Lua, inner: Rc<RefCell<WsClientInner>>) -> LuaResult<Table> {
    let send = lua.create_table()?;
    let meta = lua.create_table()?;

    meta.set(
        "__index",
        lua.create_function(move |lua, (_tbl, key): (Table, String)| {
            let event = key;
            let inner = inner.clone();

            lua.create_function(move |lua, payload: Value| {
                let payload_tbl = match payload {
                    Value::Table(t) => t,
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "ws.send.<event>(payload) requires payload table".to_string(),
                        ));
                    }
                };

                let encoded = encode_typed_event(lua, &event, payload_tbl)?;
                inner
                    .borrow_mut()
                    .send_queue
                    .push_back(OutboundMessage::Text(encoded));
                Ok(())
            })
        })?,
    )?;

    let _ = send.set_metatable(Some(meta));
    Ok(send)
}

fn connect_client(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
) -> LuaResult<bool> {
    if inner.borrow().connected {
        return Ok(true);
    }

    let (url, opts) = {
        let b = inner.borrow();
        (b.url.clone(), b.opts.clone())
    };

    if opts.tls.roots != "bundled"
        || opts.tls.ca_file.is_some()
        || opts.tls.insecure
        || !opts.tls.pin_sha256.is_empty()
    {
        return Err(mlua::Error::RuntimeError(
            "ws_client tls advanced options are not implemented yet".to_string(),
        ));
    }

    let mut request = url
        .into_client_request()
        .map_err(|e| mlua::Error::RuntimeError(format!("invalid websocket request: {e}")))?;

    for (k, v) in &opts.headers {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid ws header name '{k}': {e}")))?;
        let value = HeaderValue::from_str(v).map_err(|e| {
            mlua::Error::RuntimeError(format!("invalid ws header value for '{k}': {e}"))
        })?;
        request.headers_mut().insert(name, value);
    }

    if !opts.protocols.is_empty() {
        let joined = opts.protocols.join(", ");
        let value = HeaderValue::from_str(&joined)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid ws protocols header: {e}")))?;
        request
            .headers_mut()
            .insert(HeaderName::from_static("sec-websocket-protocol"), value);
    }

    let (mut socket, response) = tungstenite::connect(request)
        .map_err(|e| mlua::Error::RuntimeError(format!("websocket connect failed: {e}")))?;

    if !opts.protocols.is_empty() {
        let selected = response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let Some(selected_protocol) = selected else {
            return Err(mlua::Error::RuntimeError(
                "websocket protocol mismatch: server did not return Sec-WebSocket-Protocol"
                    .to_string(),
            ));
        };

        if !opts
            .protocols
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&selected_protocol))
        {
            return Err(mlua::Error::RuntimeError(format!(
                "websocket protocol mismatch: server returned '{selected_protocol}'"
            )));
        }

        inner.borrow_mut().negotiated_protocol = Some(selected_protocol);
    }

    let _ = set_nonblocking_socket(&mut socket);

    {
        let mut b = inner.borrow_mut();
        b.socket = Some(socket);
        b.connected = true;
    }

    let join_fn = client_tbl.get::<Value>("join")?;
    if let Value::Function(func) = join_fn {
        let ctx = lua.create_table()?;
        ctx.set("url", inner.borrow().url.clone())?;
        if let Some(protocol) = inner.borrow().negotiated_protocol.clone() {
            ctx.set("protocol", protocol)?;
        }

        match func.call::<Value>(ctx) {
            Ok(state) => update_state(lua, inner, state)?,
            Err(err) => emit_error(lua, client_tbl, inner, format!("join handler error: {err}"))?,
        }
    }

    Ok(true)
}

fn pump_client(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
    timeout_ms: Option<u64>,
) -> LuaResult<u32> {
    if !inner.borrow().connected {
        return Ok(0);
    }

    if let Some(ms) = timeout_ms {
        let _ = set_stream_timeouts(inner, Some(Duration::from_millis(ms)));
    }

    let mut processed = 0u32;

    flush_send_queue(lua, client_tbl, inner)?;

    loop {
        let msg = {
            let mut b = inner.borrow_mut();
            let Some(socket) = b.socket.as_mut() else {
                return Ok(processed);
            };

            match socket.read() {
                Ok(m) => m,
                Err(tungstenite::Error::Io(err))
                    if err.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    break;
                }
                Err(tungstenite::Error::AlreadyClosed)
                | Err(tungstenite::Error::ConnectionClosed) => {
                    b.connected = false;
                    b.socket = None;
                    drop(b);
                    emit_leave(lua, client_tbl, inner, true, 1000, "connection closed")?;
                    break;
                }
                Err(err) => {
                    b.connected = false;
                    b.socket = None;
                    drop(b);
                    emit_error(
                        lua,
                        client_tbl,
                        inner,
                        format!("websocket read error: {err}"),
                    )?;
                    emit_leave(lua, client_tbl, inner, true, 1006, "abnormal closure")?;
                    break;
                }
            }
        };

        processed += 1;
        match msg {
            Message::Text(text) => {
                dispatch_text_message(lua, client_tbl, inner, &text)?;
            }
            Message::Binary(_bytes) => {}
            Message::Ping(payload) => {
                if inner.borrow().opts.auto_pong {
                    inner
                        .borrow_mut()
                        .send_queue
                        .push_back(OutboundMessage::Pong(payload.to_vec()));
                }
            }
            Message::Pong(_payload) => {}
            Message::Close(frame) => {
                let (code, reason) = frame
                    .map(|f| (f.code.into(), f.reason.to_string()))
                    .unwrap_or((1000, String::new()));
                {
                    let mut b = inner.borrow_mut();
                    b.connected = false;
                    b.socket = None;
                }
                emit_leave(lua, client_tbl, inner, true, code, &reason)?;
                break;
            }
            _ => {}
        }
    }

    flush_send_queue(lua, client_tbl, inner)?;
    Ok(processed)
}

fn run_client(lua: &Lua, client_tbl: &Table, inner: &Rc<RefCell<WsClientInner>>) -> LuaResult<()> {
    let reconnect = inner.borrow().opts.reconnect.clone();
    let mut attempts: u32 = 0;

    loop {
        if inner.borrow().connected {
            attempts = 0;
            let _ = pump_client(lua, client_tbl, inner, Some(16))?;
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        if !reconnect.enabled {
            break;
        }

        if reconnect.max_attempts > 0 && attempts >= reconnect.max_attempts {
            break;
        }

        let delay_ms = reconnect_delay_ms(&reconnect, attempts);
        attempts = attempts.saturating_add(1);
        std::thread::sleep(Duration::from_millis(delay_ms));

        if let Err(err) = connect_client(lua, client_tbl, inner) {
            let _ = emit_error(lua, client_tbl, inner, format!("reconnect failed: {err}"));
        }
    }

    Ok(())
}

fn reconnect_delay_ms(reconnect: &ReconnectOptions, attempts: u32) -> u64 {
    let base = reconnect.min_ms.max(1) as f64;
    let mut delay = base * reconnect.factor.max(1.0).powi(attempts as i32);
    if reconnect.jitter {
        delay *= 0.5;
    }
    let capped = delay as u64;
    capped.min(reconnect.max_ms.max(reconnect.min_ms))
}

fn close_client(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
    code: Option<u16>,
    reason: Option<String>,
) -> LuaResult<()> {
    let reason_owned = reason.unwrap_or_default();
    let close_code = code.unwrap_or(1000);

    {
        let mut b = inner.borrow_mut();
        if let Some(socket) = b.socket.as_mut() {
            let frame = tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::from(close_code),
                reason: reason_owned.clone().into(),
            };
            let _ = socket.close(Some(frame));
        }
        b.connected = false;
        b.socket = None;
    }

    emit_leave(lua, client_tbl, inner, false, close_code, &reason_owned)
}

fn dispatch_text_message(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
    text: &str,
) -> LuaResult<()> {
    if text.len() > inner.borrow().opts.max_message_bytes {
        return emit_error(
            lua,
            client_tbl,
            inner,
            "websocket message exceeds max_message_bytes".to_string(),
        );
    }

    let parsed: JsonValue = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(err) => {
            return emit_error(
                lua,
                client_tbl,
                inner,
                format!("websocket invalid json: {err}"),
            );
        }
    };

    let mut event = "message".to_string();
    let mut body = parsed;
    if let JsonValue::Object(ref mut map) = body {
        if let Some(JsonValue::String(kind)) = map.remove("type") {
            event = kind;
        }
    }

    let msg = json_to_lua_value(lua, &body)?;
    let state = get_state_value(lua, inner)?;

    let ctx = lua.create_table()?;
    ctx.set("url", inner.borrow().url.clone())?;
    ctx.set("event", event.clone())?;

    let listen_tbl: Table = client_tbl.get("listen")?;
    let handlers: Table = listen_tbl.raw_get("__handlers")?;

    let handler: Option<Function> = handlers
        .raw_get::<Value>(event.as_str())
        .ok()
        .and_then(|v| match v {
            Value::Function(f) => Some(f),
            _ => None,
        })
        .or_else(|| {
            handlers
                .raw_get::<Value>("message")
                .ok()
                .and_then(|v| match v {
                    Value::Function(f) => Some(f),
                    _ => None,
                })
        });

    if let Some(func) = handler {
        match func.call::<Value>((msg, ctx, state)) {
            Ok(new_state) => update_state(lua, inner, new_state)?,
            Err(err) => emit_error(
                lua,
                client_tbl,
                inner,
                format!("listen handler error: {err}"),
            )?,
        }
    }

    Ok(())
}

fn flush_send_queue(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
) -> LuaResult<()> {
    loop {
        let maybe_item = inner.borrow().send_queue.front().cloned();
        let Some(item) = maybe_item else {
            return Ok(());
        };

        let result = {
            let mut b = inner.borrow_mut();
            let Some(socket) = b.socket.as_mut() else {
                return Ok(());
            };

            match item {
                OutboundMessage::Text(text) => socket.send(Message::Text(text)),
                OutboundMessage::Binary(bytes) => socket.send(Message::Binary(bytes)),
                OutboundMessage::Ping(payload) => socket.send(Message::Ping(payload)),
                OutboundMessage::Pong(payload) => socket.send(Message::Pong(payload)),
            }
        };

        match result {
            Ok(_) => {
                let _ = inner.borrow_mut().send_queue.pop_front();
            }
            Err(tungstenite::Error::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
                break;
            }
            Err(err) => {
                {
                    let mut b = inner.borrow_mut();
                    b.connected = false;
                    b.socket = None;
                }
                emit_error(
                    lua,
                    client_tbl,
                    inner,
                    format!("websocket write error: {err}"),
                )?;
                emit_leave(lua, client_tbl, inner, true, 1006, "abnormal closure")?;
                break;
            }
        }
    }

    Ok(())
}

fn emit_leave(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
    remote: bool,
    code: u16,
    reason: &str,
) -> LuaResult<()> {
    let leave_fn = client_tbl.get::<Value>("leave")?;
    if let Value::Function(func) = leave_fn {
        let info = lua.create_table()?;
        info.set("remote", remote)?;
        info.set("code", code)?;
        info.set("reason", reason)?;
        let state = get_state_value(lua, inner)?;
        let _ = func.call::<Value>((info, state));
    }

    clear_state(lua, inner)
}

fn emit_error(
    lua: &Lua,
    client_tbl: &Table,
    inner: &Rc<RefCell<WsClientInner>>,
    message: String,
) -> LuaResult<()> {
    let err_fn = client_tbl.get::<Value>("error")?;
    if let Value::Function(func) = err_fn {
        let err_tbl = lua.create_table()?;
        err_tbl.set("message", message)?;

        let ctx = lua.create_table()?;
        ctx.set("url", inner.borrow().url.clone())?;

        let state = get_state_value(lua, inner)?;
        let _ = func.call::<Value>((err_tbl, ctx, state));
    }
    Ok(())
}

fn update_state(lua: &Lua, inner: &Rc<RefCell<WsClientInner>>, value: Value) -> LuaResult<()> {
    if matches!(value, Value::Nil) {
        return Ok(());
    }

    clear_state(lua, inner)?;
    let key = lua.create_registry_value(value)?;
    inner.borrow_mut().state_key = Some(key);
    Ok(())
}

fn clear_state(lua: &Lua, inner: &Rc<RefCell<WsClientInner>>) -> LuaResult<()> {
    if let Some(key) = inner.borrow_mut().state_key.take() {
        lua.remove_registry_value(key)?;
    }
    Ok(())
}

fn get_state_value(lua: &Lua, inner: &Rc<RefCell<WsClientInner>>) -> LuaResult<Value> {
    if let Some(ref key) = inner.borrow().state_key {
        lua.registry_value(key)
    } else {
        Ok(Value::Nil)
    }
}

fn encode_typed_event(lua: &Lua, event: &str, payload: Table) -> LuaResult<String> {
    let mut obj = Map::new();
    obj.insert("type".to_string(), JsonValue::String(event.to_string()));

    for pair in payload.pairs::<Value, Value>() {
        let (k, v) = pair?;
        if let Value::String(key) = k {
            let key = key.to_str()?.to_string();
            if key == "type" {
                continue;
            }
            let json: JsonValue = lua.from_value(v)?;
            obj.insert(key, json);
        }
    }

    serde_json::to_string(&JsonValue::Object(obj))
        .map_err(|e| mlua::Error::RuntimeError(format!("ws json encode failed: {e}")))
}

fn json_to_lua_value(lua: &Lua, value: &JsonValue) -> LuaResult<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(v) => Ok(Value::Boolean(*v)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(s) => Ok(Value::String(lua.create_string(s)?)),
        JsonValue::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
        JsonValue::Object(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
    }
}

fn parse_ws_client_options(opts: Option<Table>) -> LuaResult<WsClientOptions> {
    let Some(opts) = opts else {
        return Ok(WsClientOptions::default());
    };

    let mut out = WsClientOptions::default();

    if let Ok(headers) = opts.get::<Table>("headers") {
        for pair in headers.pairs::<String, String>() {
            let (k, v) = pair?;
            out.headers.insert(k, v);
        }
    }

    if let Ok(protocols) = opts.get::<Table>("protocols") {
        for p in protocols.sequence_values::<String>() {
            out.protocols.push(p?);
        }
    }

    if let Ok(v) = opts.get::<u64>("handshake_timeout_ms") {
        out.handshake_timeout_ms = v;
    }
    if let Ok(v) = opts.get::<usize>("max_message_bytes") {
        out.max_message_bytes = v.max(1);
    }
    if let Ok(v) = opts.get::<bool>("auto_pong") {
        out.auto_pong = v;
    }

    if let Ok(reconnect) = opts.get::<Table>("reconnect") {
        if let Ok(v) = reconnect.get::<bool>("enabled") {
            out.reconnect.enabled = v;
        }
        if let Ok(v) = reconnect.get::<u64>("min_ms") {
            out.reconnect.min_ms = v;
        }
        if let Ok(v) = reconnect.get::<u64>("max_ms") {
            out.reconnect.max_ms = v;
        }
        if let Ok(v) = reconnect.get::<f64>("factor") {
            out.reconnect.factor = v;
        }
        if let Ok(v) = reconnect.get::<bool>("jitter") {
            out.reconnect.jitter = v;
        }
        if let Ok(v) = reconnect.get::<u32>("max_attempts") {
            out.reconnect.max_attempts = v;
        }
    }

    if let Ok(tls) = opts.get::<Table>("tls") {
        if let Ok(v) = tls.get::<String>("roots") {
            out.tls.roots = v;
        }
        if let Ok(v) = tls.get::<String>("ca_file") {
            out.tls.ca_file = Some(v);
        }
        if let Ok(v) = tls.get::<bool>("insecure") {
            out.tls.insecure = v;
        }
        if let Ok(pins) = tls.get::<Table>("pin_sha256") {
            for pin in pins.sequence_values::<String>() {
                out.tls.pin_sha256.push(pin?);
            }
        }
    }

    Ok(out)
}

fn set_nonblocking_socket(socket: &mut ClientSocket) -> std::io::Result<()> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_nonblocking(true),
        MaybeTlsStream::Rustls(stream) => stream.get_mut().set_nonblocking(true),
        _ => Ok(()),
    }
}

fn set_stream_timeouts(
    inner: &Rc<RefCell<WsClientInner>>,
    timeout: Option<Duration>,
) -> std::io::Result<()> {
    let mut b = inner.borrow_mut();
    let Some(socket) = b.socket.as_mut() else {
        return Ok(());
    };

    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            stream.set_read_timeout(timeout)?;
            stream.set_write_timeout(timeout)
        }
        MaybeTlsStream::Rustls(stream) => {
            let tcp = stream.get_mut();
            tcp.set_read_timeout(timeout)?;
            tcp.set_write_timeout(timeout)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_encode_typed_event() {
        let lua = Lua::new();
        let payload = lua.create_table().unwrap();
        payload.set("text", "hello").unwrap();
        let encoded = encode_typed_event(&lua, "chat", payload).unwrap();
        assert_eq!(encoded, r#"{"text":"hello","type":"chat"}"#);
    }

    #[test]
    fn should_parse_options() {
        let lua = Lua::new();
        let opts = lua.create_table().unwrap();

        let headers = lua.create_table().unwrap();
        headers.set("Authorization", "Bearer x").unwrap();
        opts.set("headers", headers).unwrap();

        let protocols = lua.create_table().unwrap();
        protocols.set(1, "chat.v1").unwrap();
        opts.set("protocols", protocols).unwrap();

        opts.set("max_message_bytes", 1024).unwrap();

        let parsed = parse_ws_client_options(Some(opts)).unwrap();
        assert_eq!(parsed.max_message_bytes, 1024);
        assert_eq!(parsed.protocols, vec!["chat.v1".to_string()]);
        assert_eq!(
            parsed.headers.get("Authorization").cloned(),
            Some("Bearer x".to_string())
        );
    }

    #[test]
    fn should_compute_reconnect_delay() {
        let opts = ReconnectOptions {
            enabled: true,
            min_ms: 250,
            max_ms: 10_000,
            factor: 2.0,
            jitter: false,
            max_attempts: 0,
        };
        assert_eq!(reconnect_delay_ms(&opts, 0), 250);
        assert_eq!(reconnect_delay_ms(&opts, 1), 500);
        assert_eq!(reconnect_delay_ms(&opts, 2), 1000);
    }
}
