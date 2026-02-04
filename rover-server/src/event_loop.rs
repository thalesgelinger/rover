use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::mem;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

use anyhow::Result;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use mlua::{Function, Lua, Thread, ThreadStatus, Value};
use slab::Slab;
use tracing::{debug, info, warn};

use crate::buffer_pool::BufferPool;
use crate::connection::{Connection, ConnectionState};
use crate::fast_router::FastRouter;
use crate::http_task::{
    CoroutineResponse, RequestContextPool, ThreadPool, execute_handler_coroutine,
};
use crate::table_pool::LuaTablePool;
use crate::ws_frame::{self, WsOpcode};
use crate::ws_handshake;
use crate::ws_lua::{SharedConnections, SharedWsManager};
use crate::ws_manager::WsManager;
use crate::{Bytes, HttpMethod, Route, ServerConfig, WsRoute};

const LISTENER: Token = Token(0);
const DEFAULT_COROUTINE_TIMEOUT_MS: u64 = 30000;

struct PendingCoroutine {
    thread: Thread,
    started_at: Instant,
    ctx_idx: usize,
}

pub struct EventLoop {
    poll: Poll,
    listener: TcpListener,
    connections: Rc<RefCell<Slab<Connection>>>,
    lua: Lua,
    router: FastRouter,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    yielded_coroutines: HashMap<usize, PendingCoroutine>,
    thread_pool: ThreadPool,
    request_pool: RequestContextPool,
    table_pool: LuaTablePool,
    buffer_pool: BufferPool,
    ws_manager: SharedWsManager,
}

impl EventLoop {
    pub fn new(
        lua: Lua,
        routes: Vec<Route>,
        ws_routes: Vec<WsRoute>,
        config: ServerConfig,
        openapi_spec: Option<serde_json::Value>,
        addr: SocketAddr,
    ) -> Result<Self> {
        let poll = Poll::new()?;
        let mut listener = TcpListener::bind(addr)?;

        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;

        let mut router = FastRouter::from_routes(routes)?;

        // Register WS endpoints
        let ws_manager = Rc::new(RefCell::new(WsManager::new()));
        let mut ws_patterns = Vec::new();

        for ws_route in ws_routes {
            let pattern = std::str::from_utf8(&ws_route.pattern)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in WS route pattern"))?
                .to_string();
            let endpoint_idx = ws_manager
                .borrow_mut()
                .register_endpoint(ws_route.endpoint_config);
            let is_static = ws_route.is_static;

            if config.log_level != "nope" {
                info!("  WS {} (endpoint #{})", pattern, endpoint_idx);
            }

            ws_patterns.push((pattern, endpoint_idx, is_static));
        }

        if !ws_patterns.is_empty() {
            router.add_ws_routes(ws_patterns)?;
        }

        // Set WsManager as Lua app_data so ws.send/ws.listen can access it
        lua.set_app_data(ws_manager.clone());

        // Shared connections for Lua send operations
        let connections: Rc<RefCell<Slab<Connection>>> =
            Rc::new(RefCell::new(Slab::with_capacity(1024)));
        lua.set_app_data::<SharedConnections>(connections.clone());

        let request_pool = RequestContextPool::new(&lua, 1024)?;
        let table_pool = LuaTablePool::new(1024);
        let buffer_pool = BufferPool::new();

        Ok(Self {
            poll,
            listener,
            connections,
            lua,
            router,
            config,
            openapi_spec,
            yielded_coroutines: HashMap::with_capacity(1024),
            thread_pool: ThreadPool::new(2048),
            request_pool,
            table_pool,
            buffer_pool,
            ws_manager,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let mut events = Events::with_capacity(1024);

        loop {
            self.poll.poll(&mut events, None)?;

            for event in events.iter() {
                match event.token() {
                    LISTENER => self.accept_connections()?,
                    token => self.handle_connection(token, event)?,
                }
            }

            self.check_timeouts()?;

            if !self.yielded_coroutines.is_empty() {
                self.resume_yielded_coroutines()?;
            }
        }
    }

    fn accept_connections(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut socket, _addr)) => {
                    let mut conns = self.connections.borrow_mut();
                    let entry = conns.vacant_entry();
                    let token = Token(entry.key() + 1);

                    self.poll
                        .registry()
                        .register(&mut socket, token, Interest::READABLE)?;

                    entry.insert(Connection::new(socket, token));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn handle_connection(&mut self, token: Token, event: &mio::event::Event) -> Result<()> {
        let conn_idx = token.0 - 1;

        if !self.connections.borrow().contains(conn_idx) {
            return Ok(());
        }

        // Check if this is a WebSocket connection
        {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx) {
                if conn.is_websocket() {
                    drop(conns);
                    return self.handle_ws_event(conn_idx, event);
                }
            }
        }

        let (should_process, should_close, should_reset, is_ws_upgrade_complete) = {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];

            match conn.state {
                ConnectionState::Reading if event.is_readable() => match conn.try_read() {
                    Ok(true) => (true, false, false, false),
                    Ok(false) => (false, false, false, false),
                    Err(_) => {
                        conn.state = ConnectionState::Closed;
                        (false, true, false, false)
                    }
                },
                ConnectionState::Writing if event.is_writable() => match conn.try_write() {
                    Ok(true) => {
                        // Check if this was a WS upgrade 101 response
                        if conn.pending_ws_upgrade.is_some() {
                            (false, false, false, true)
                        } else if conn.keep_alive {
                            (false, false, true, false)
                        } else {
                            conn.state = ConnectionState::Closed;
                            (false, true, false, false)
                        }
                    }
                    Ok(false) => (false, false, false, false),
                    Err(_) => {
                        conn.state = ConnectionState::Closed;
                        (false, true, false, false)
                    }
                },
                _ => (false, false, false, false),
            }
        };

        // Handle WS upgrade completion (101 fully written)
        if is_ws_upgrade_complete {
            return self.complete_ws_upgrade(conn_idx);
        }

        let is_closed_state = {
            let conns = self.connections.borrow();
            matches!(
                conns.get(conn_idx).map(|c| &c.state),
                Some(ConnectionState::Closed)
            )
        };

        if should_close || is_closed_state {
            self.recycle_write_buf(conn_idx);
            let mut conns = self.connections.borrow_mut();
            let mut conn = conns.remove(conn_idx);
            let _ = self.poll.registry().deregister(&mut conn.socket);
            return Ok(());
        }

        if should_reset {
            self.recycle_write_buf(conn_idx);
            let mut conns = self.connections.borrow_mut();
            if let Some(conn) = conns.get_mut(conn_idx) {
                conn.reset();
                let _ = conn.reregister(&self.poll.registry(), Interest::READABLE);
            }
        }

        if should_process {
            self.start_request_coroutine(conn_idx)?;
        }

        Ok(())
    }

    fn recycle_write_buf(&mut self, conn_idx: usize) {
        let mut conns = self.connections.borrow_mut();
        if let Some(conn) = conns.get_mut(conn_idx) {
            let buf = mem::take(&mut conn.write_buf);
            if !buf.is_empty() {
                self.buffer_pool.return_response_buf(buf);
            }
        }
    }

    // ── WebSocket upgrade ──

    fn start_request_coroutine(&mut self, conn_idx: usize) -> Result<()> {
        let started_at = Instant::now();

        let conns = self.connections.borrow();
        let conn = &conns[conn_idx];
        let method = conn.method_str().unwrap_or_default();
        let full_path = conn.path_str().unwrap_or_default();
        let (path, query_str) = if let Some(pos) = full_path.find('?') {
            (&full_path[..pos], Some(full_path[pos + 1..].to_string()))
        } else {
            (full_path, None)
        };

        #[allow(unused_variables)]
        let buf_ref: &[u8] = if !conn.parsed_buf.is_empty() {
            &conn.parsed_buf
        } else {
            &conn.read_buf
        };
        let (path_off, path_len) = conn.path_offset.unwrap_or((0, 0));
        let keep_alive = conn.keep_alive;

        // ── Check for WebSocket upgrade ──
        let has_upgrade = conn.header_offsets.iter().any(|&(name_off, name_len, val_off, val_len)| {
            let name = unsafe {
                std::str::from_utf8_unchecked(&buf_ref[name_off..name_off + name_len])
            };
            let val = unsafe {
                std::str::from_utf8_unchecked(&buf_ref[val_off..val_off + val_len])
            };
            name.eq_ignore_ascii_case("upgrade") && val.eq_ignore_ascii_case("websocket")
        });

        if has_upgrade {
            let path_owned = path.to_string();
            drop(conns);
            return self.handle_ws_upgrade(conn_idx, &path_owned, keep_alive);
        }

        // ── Regular HTTP path ──
        if self.config.docs && path == "/docs" && self.openapi_spec.is_some() {
            let html = rover_openapi::scalar_html(self.openapi_spec.as_ref().unwrap());
            drop(conns);
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.keep_alive = keep_alive;
            let buf = self.buffer_pool.get_response_buf();
            conn.set_response_with_buf(200, html.as_bytes(), Some("text/html"), buf);
            let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
            return Ok(());
        }

        let http_method = match HttpMethod::from_str(method) {
            Some(m) => m,
            None => {
                let error_msg = format!(
                    "Invalid HTTP method '{}'. Valid methods: {}",
                    method,
                    HttpMethod::valid_methods().join(", ")
                );
                drop(conns);
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(400, error_msg.as_bytes(), Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        let path_owned = path.to_string();
        let (handler, params) = match self.router.match_route(http_method, &path_owned) {
            Some((h, p)) => (h.clone(), p),
            None => {
                drop(conns);
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(404, b"Route not found", Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        let buf = if !conn.parsed_buf.is_empty() {
            conn.parsed_buf.clone()
        } else {
            conn.read_buf.clone().freeze()
        };

        let (method_off, method_len) = conn.method_offset.unwrap_or((0, 0));

        let query_offsets = if let Some(qs) = &query_str {
            let search_start = path_off;
            let search_end = (path_off + path_len).min(buf.len());
            if let Some(q_pos) = buf[search_start..search_end]
                .iter()
                .position(|&b| b == b'?')
            {
                let qs_start_abs = path_off + q_pos + 1;
                let mut offsets = Vec::new();
                let mut pos = 0usize;
                let qs_len = qs.len();

                while pos < qs_len {
                    let key_start = pos as u16;

                    while pos < qs_len && qs.as_bytes()[pos] != b'=' && qs.as_bytes()[pos] != b'&' {
                        pos += 1;
                    }

                    let key_len_raw = (pos - key_start as usize) as u8;

                    if pos >= qs_len || qs.as_bytes()[pos] == b'&' {
                        if key_len_raw > 0 {
                            offsets.push((
                                (qs_start_abs + key_start as usize) as u16,
                                key_len_raw,
                                (qs_start_abs + key_start as usize) as u16,
                                key_len_raw as u16,
                            ));
                        }
                        pos += 1;
                        continue;
                    }

                    pos += 1;
                    let val_start = pos as u16;

                    while pos < qs_len && qs.as_bytes()[pos] != b'&' {
                        pos += 1;
                    }

                    let val_len_raw = (pos - val_start as usize) as u16;
                    offsets.push((
                        (qs_start_abs + key_start as usize) as u16,
                        key_len_raw,
                        (qs_start_abs + val_start as usize) as u16,
                        val_len_raw,
                    ));

                    pos += 1;
                }
                offsets
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let mut header_offsets = Vec::with_capacity(conn.header_offsets.len());
        for &(name_off, name_len, val_off, val_len) in conn.header_offsets.iter() {
            header_offsets.push((
                name_off as u16,
                name_len as u8,
                val_off as u16,
                val_len as u16,
            ));
        }

        let (body_off, body_len) = conn
            .body
            .map(|(off, len)| (off as u32, len as u32))
            .unwrap_or((0, 0));

        // Drop the borrow before calling into Lua
        drop(conns);

        match execute_handler_coroutine(
            &self.lua,
            &handler,
            buf,
            method_off as u16,
            method_len as u8,
            path_off as u16,
            path_len as u16,
            body_off,
            body_len,
            header_offsets,
            query_offsets,
            &params,
            started_at,
            &mut self.thread_pool,
            &mut self.request_pool,
            &self.table_pool,
            &mut self.buffer_pool,
        ) {
            Ok(CoroutineResponse::Ready {
                status,
                body,
                content_type,
            }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_bytes_with_buf(status, body, content_type, buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
            }
            Ok(CoroutineResponse::Yielded { thread, ctx_idx }) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.thread = Some(thread.clone());
                self.yielded_coroutines.insert(
                    conn_idx,
                    PendingCoroutine {
                        thread,
                        started_at: Instant::now(),
                        ctx_idx,
                    },
                );
            }
            Err(_) => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(500, b"Internal server error", Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
            }
        }

        Ok(())
    }

    // ── WebSocket upgrade handling ──

    fn handle_ws_upgrade(
        &mut self,
        conn_idx: usize,
        path: &str,
        keep_alive: bool,
    ) -> Result<()> {
        // Match against WS router
        let (endpoint_idx, _params) = match self.router.match_ws_route(path) {
            Some((idx, p)) => (idx, p),
            None => {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(404, b"WebSocket route not found", Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        // Validate upgrade headers
        let accept_key = {
            let conns = self.connections.borrow();
            let conn = &conns[conn_idx];
            let buf: &[u8] = if !conn.parsed_buf.is_empty() {
                &conn.parsed_buf
            } else {
                &conn.read_buf
            };

            match ws_handshake::validate_upgrade_headers(buf, &conn.header_offsets) {
                Ok(key) => ws_handshake::compute_accept_key(key),
                Err(e) => {
                    drop(conns);
                    let mut conns = self.connections.borrow_mut();
                    let conn = &mut conns[conn_idx];
                    conn.keep_alive = false;
                    let buf = self.buffer_pool.get_response_buf();
                    conn.set_response_with_buf(
                        e.status_code(),
                        e.message().as_bytes(),
                        Some("text/plain"),
                        buf,
                    );
                    let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                    return Ok(());
                }
            }
        };

        // Build 101 response
        let mut response_buf = self.buffer_pool.get_response_buf();
        ws_handshake::build_upgrade_response(&accept_key, &mut response_buf);

        // Write the 101 response
        {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.write_buf = response_buf;
            conn.write_pos = 0;
            conn.body_buf = Bytes::new();
            conn.body_pos = 0;
            conn.state = ConnectionState::Writing;
            conn.pending_ws_upgrade = Some(endpoint_idx);
            let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
        }

        info!("WS upgrade initiated for conn {} -> endpoint #{}", conn_idx, endpoint_idx);

        Ok(())
    }

    fn complete_ws_upgrade(&mut self, conn_idx: usize) -> Result<()> {
        let endpoint_idx = {
            let conns = self.connections.borrow();
            match conns.get(conn_idx).and_then(|c| c.pending_ws_upgrade) {
                Some(idx) => idx,
                None => return Ok(()),
            }
        };

        // Collect info needed for join handler before upgrading
        let (buf, header_offsets, query_offsets, params, path_off, path_len, method_off, method_len) = {
            let conns = self.connections.borrow();
            let conn = &conns[conn_idx];
            let buf = if !conn.parsed_buf.is_empty() {
                conn.parsed_buf.clone()
            } else {
                conn.read_buf.clone().freeze()
            };
            let header_offsets: Vec<(u16, u8, u16, u16)> = conn
                .header_offsets
                .iter()
                .map(|&(no, nl, vo, vl)| (no as u16, nl as u8, vo as u16, vl as u16))
                .collect();
            let (po, pl) = conn.path_offset.unwrap_or((0, 0));
            let (mo, ml) = conn.method_offset.unwrap_or((0, 0));
            // For now, pass empty query/params (upgrade request already parsed)
            (buf, header_offsets, Vec::new(), Vec::new(), po as u16, pl as u16, mo as u16, ml as u8)
        };

        // Upgrade the connection
        {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            conn.upgrade_to_ws(endpoint_idx);
            let _ = conn.reregister(&self.poll.registry(), Interest::READABLE);
        }

        // Track the connection
        self.ws_manager.borrow_mut().add_connection(endpoint_idx, conn_idx);

        // Call ws.join(ctx) handler
        let mgr = self.ws_manager.borrow();
        let endpoint = &mgr.endpoints[endpoint_idx as usize];

        if let Some(ref join_key) = endpoint.join_handler {
            let join_fn: Function = self.lua.registry_value(join_key)?;

            // Create a request context for the join handler
            let (ctx, ctx_idx) = self.request_pool.acquire(
                &self.lua,
                buf,
                method_off,
                method_len,
                path_off,
                path_len,
                0, 0, // no body
                header_offsets,
                query_offsets,
                &params,
            )?;

            drop(mgr);

            // Set WsManager context for the join handler
            self.ws_manager
                .borrow_mut()
                .set_context(conn_idx, endpoint_idx);

            // Execute join handler
            let thread = self.thread_pool.acquire(&self.lua, &join_fn)?;
            match thread.resume::<Value>(ctx) {
                Ok(state_value) => {
                    // Store the returned state in Lua registry
                    if !matches!(state_value, Value::Nil) {
                        let state_key = self.lua.create_registry_value(state_value)?;
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            if let Some(ref mut ws) = conn.ws_data {
                                ws.state_key = Some(state_key);
                            }
                        }
                    }
                    self.thread_pool.release(thread);
                }
                Err(e) => {
                    warn!("WS join handler error: {}", e);
                    self.thread_pool.release(thread);
                }
            }

            self.request_pool.release(ctx_idx);
        }

        info!("WS connection {} upgraded to endpoint #{}", conn_idx, endpoint_idx);
        Ok(())
    }

    // ── WebSocket event handling ──

    fn handle_ws_event(&mut self, conn_idx: usize, event: &mio::event::Event) -> Result<()> {
        if event.is_readable() {
            self.handle_ws_readable(conn_idx)?;
        }

        if event.is_writable() {
            self.handle_ws_writable(conn_idx)?;
        }

        Ok(())
    }

    fn handle_ws_readable(&mut self, conn_idx: usize) -> Result<()> {
        // Read data from socket
        let bytes_read = {
            let mut conns = self.connections.borrow_mut();
            let conn = &mut conns[conn_idx];
            match conn.ws_read() {
                Ok(n) => n,
                Err(_) => {
                    drop(conns);
                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            }
        };

        if bytes_read == 0 {
            // EOF
            self.handle_ws_disconnect(conn_idx)?;
            return Ok(());
        }

        // Parse and process frames
        loop {
            let frame_result = {
                let conns = self.connections.borrow();
                let conn = &conns[conn_idx];
                let unprocessed = &conn.read_buf[..conn.read_pos];
                ws_frame::try_parse_frame(unprocessed)
                    .map(|h| (h.fin, h.opcode, h.masked, h.mask, h.payload_offset, h.payload_len, h.total_frame_len))
            };

            let Some((fin, opcode, masked, mask, payload_offset, payload_len, total_frame_len)) = frame_result else {
                break; // incomplete frame, wait for more data
            };

            // Extract and unmask payload
            let payload = {
                let mut conns = self.connections.borrow_mut();
                let conn = &mut conns[conn_idx];

                if masked && payload_len > 0 {
                    ws_frame::unmask_payload_in_place(
                        &mut conn.read_buf[payload_offset..payload_offset + payload_len],
                        mask,
                    );
                }

                let payload = conn.read_buf[payload_offset..payload_offset + payload_len].to_vec();

                // Advance buffer past this frame
                let remaining = conn.read_pos - total_frame_len;
                if remaining > 0 {
                    conn.read_buf.copy_within(total_frame_len..conn.read_pos, 0);
                }
                conn.read_pos = remaining;

                payload
            };

            match opcode {
                WsOpcode::Text | WsOpcode::Binary => {
                    if fin {
                        // Complete message
                        self.dispatch_ws_message(conn_idx, &payload)?;
                    } else {
                        // Start of fragmented message
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            if let Some(ref mut ws) = conn.ws_data {
                                ws.fragment_opcode = Some(opcode);
                                ws.fragment_buf = Some(payload);
                            }
                        }
                    }
                }
                WsOpcode::Continuation => {
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx) {
                        if let Some(ref mut ws) = conn.ws_data {
                            if let Some(ref mut frag) = ws.fragment_buf {
                                frag.extend_from_slice(&payload);
                            }
                            if fin {
                                let assembled = ws.fragment_buf.take().unwrap_or_default();
                                ws.fragment_opcode = None;
                                drop(conns);
                                self.dispatch_ws_message(conn_idx, &assembled)?;
                            }
                        }
                    }
                }
                WsOpcode::Ping => {
                    // Respond with pong echoing the payload
                    let mut frame_buf = self.ws_manager.borrow_mut().get_frame_buf();
                    ws_frame::write_pong_frame(&mut frame_buf, &payload);
                    let frame = Bytes::from(frame_buf);

                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx) {
                        conn.queue_ws_frame(frame);
                        let _ = conn.reregister(&self.poll.registry(), Interest::READABLE | Interest::WRITABLE);
                    }
                }
                WsOpcode::Pong => {
                    // Ignore pong responses
                }
                WsOpcode::Close => {
                    // Send close frame back if we haven't already
                    let should_close = {
                        let conns = self.connections.borrow();
                        conns.get(conn_idx)
                            .and_then(|c| c.ws_data.as_ref())
                            .map(|ws| !ws.close_sent)
                            .unwrap_or(false)
                    };

                    if should_close {
                        let status_code = if payload.len() >= 2 {
                            u16::from_be_bytes([payload[0], payload[1]])
                        } else {
                            1000
                        };

                        let mut frame_buf = self.ws_manager.borrow_mut().get_frame_buf();
                        ws_frame::write_close_frame(&mut frame_buf, status_code, "");
                        let frame = Bytes::from(frame_buf);

                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            if let Some(ref mut ws) = conn.ws_data {
                                ws.close_sent = true;
                            }
                            conn.queue_ws_frame(frame);
                            conn.state = ConnectionState::WsClosed;
                            let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                        }
                    }

                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            }
        }

        // If there are frames queued for writing, register for WRITABLE too
        {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx) {
                if let Some(ref ws) = conn.ws_data {
                    if !ws.write_queue.is_empty() {
                        drop(conns);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            let _ = conn.reregister(
                                &self.poll.registry(),
                                Interest::READABLE | Interest::WRITABLE,
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_ws_writable(&mut self, conn_idx: usize) -> Result<()> {
        let (drained, is_ws_closed) = {
            let mut conns = self.connections.borrow_mut();
            let conn = match conns.get_mut(conn_idx) {
                Some(c) => c,
                None => return Ok(()),
            };

            let drained = match conn.try_write_ws() {
                Ok(d) => d,
                Err(_) => {
                    conn.state = ConnectionState::Closed;
                    drop(conns);
                    self.handle_ws_disconnect(conn_idx)?;
                    return Ok(());
                }
            };

            let is_ws_closed = conn.state == ConnectionState::WsClosed;
            (drained, is_ws_closed)
        };

        if drained {
            if is_ws_closed {
                // Close handshake complete, disconnect
                self.handle_ws_disconnect(conn_idx)?;
            } else {
                // Queue empty, only listen for reads
                let mut conns = self.connections.borrow_mut();
                if let Some(conn) = conns.get_mut(conn_idx) {
                    let _ = conn.reregister(&self.poll.registry(), Interest::READABLE);
                }
            }
        }

        Ok(())
    }

    /// Dispatch a complete WebSocket text message to the appropriate Lua handler.
    fn dispatch_ws_message(&mut self, conn_idx: usize, payload: &[u8]) -> Result<()> {
        // Parse the JSON message
        let msg_value = match crate::direct_json_parser::json_bytes_ref_to_lua_direct(
            &self.lua,
            &Bytes::copy_from_slice(payload),
        ) {
            Ok(v) => v,
            Err(e) => {
                debug!("WS invalid JSON from conn {}: {}", conn_idx, e);
                return Ok(());
            }
        };

        // Extract the "type" field for event routing
        let (event_name, msg_table) = match msg_value {
            Value::Table(ref tbl) => {
                let type_val: Value = tbl.raw_get("type").unwrap_or(Value::Nil);
                match type_val {
                    Value::String(s) => {
                        let event = s.to_str()?.to_string();
                        // Remove "type" from the message table
                        let _ = tbl.raw_set("type", Value::Nil);
                        (Some(event), tbl.clone())
                    }
                    _ => (None, tbl.clone()),
                }
            }
            _ => {
                debug!("WS message is not a JSON object, ignoring");
                return Ok(());
            }
        };

        let endpoint_idx = {
            let conns = self.connections.borrow();
            conns.get(conn_idx)
                .and_then(|c| c.ws_data.as_ref())
                .map(|ws| ws.endpoint_idx)
                .unwrap_or(0)
        };

        // Get the handler function from the endpoint
        let handler_fn: Option<Function> = {
            let mgr = self.ws_manager.borrow();
            if let Some(endpoint) = mgr.endpoints.get(endpoint_idx as usize) {
                if let Some(event) = &event_name {
                    if let Some(key) = endpoint.event_handlers.get(event) {
                        self.lua.registry_value(key).ok()
                    } else {
                        // Try "message" catch-all
                        endpoint.event_handlers.get("message")
                            .and_then(|key| self.lua.registry_value(key).ok())
                    }
                } else {
                    // No type field, try "message" catch-all
                    endpoint.event_handlers.get("message")
                        .and_then(|key| self.lua.registry_value(key).ok())
                }
            } else {
                None
            }
        };

        let Some(handler_fn) = handler_fn else {
            debug!("WS no handler for event {:?} on endpoint {}", event_name, endpoint_idx);
            return Ok(());
        };

        // Set WsManager context
        self.ws_manager.borrow_mut().set_context(conn_idx, endpoint_idx);

        // Get the connection state for the handler
        let state_value: Value = {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx) {
                if let Some(ref ws) = conn.ws_data {
                    if let Some(ref key) = ws.state_key {
                        self.lua.registry_value(key).unwrap_or(Value::Nil)
                    } else {
                        Value::Nil
                    }
                } else {
                    Value::Nil
                }
            } else {
                Value::Nil
            }
        };

        // Create a minimal request context for the handler
        let ctx = self.lua.create_table()?;

        // Call handler: ws.listen.<event>(msg, ctx, state)
        let thread = self.thread_pool.acquire(&self.lua, &handler_fn)?;
        match thread.resume::<Value>((Value::Table(msg_table), Value::Table(ctx), state_value)) {
            Ok(new_state) => {
                // If handler returns a value, update the connection state
                if !matches!(new_state, Value::Nil) {
                    let state_key = self.lua.create_registry_value(new_state)?;
                    let mut conns = self.connections.borrow_mut();
                    if let Some(conn) = conns.get_mut(conn_idx) {
                        if let Some(ref mut ws) = conn.ws_data {
                            // Remove old state key
                            if let Some(old_key) = ws.state_key.take() {
                                self.lua.remove_registry_value(old_key)?;
                            }
                            ws.state_key = Some(state_key);
                        }
                    }
                }
                self.thread_pool.release(thread);
            }
            Err(e) => {
                warn!("WS handler error for event {:?}: {}", event_name, e);
                self.thread_pool.release(thread);
            }
        }

        // If handler queued frames, make sure we're registered for writing
        {
            let conns = self.connections.borrow();
            if let Some(conn) = conns.get(conn_idx) {
                if let Some(ref ws) = conn.ws_data {
                    if !ws.write_queue.is_empty() {
                        drop(conns);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            let _ = conn.reregister(
                                &self.poll.registry(),
                                Interest::READABLE | Interest::WRITABLE,
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_ws_disconnect(&mut self, conn_idx: usize) -> Result<()> {
        let endpoint_idx = {
            let conns = self.connections.borrow();
            match conns.get(conn_idx) {
                Some(conn) if conn.is_websocket() => {
                    conn.ws_data.as_ref().map(|ws| ws.endpoint_idx).unwrap_or(0)
                }
                _ => return Ok(()),
            }
        };

        // Set context for leave handler
        self.ws_manager.borrow_mut().set_context(conn_idx, endpoint_idx);

        // Call leave handler
        let leave_fn: Option<Function> = {
            let mgr = self.ws_manager.borrow();
            mgr.endpoints
                .get(endpoint_idx as usize)
                .and_then(|ep| ep.leave_handler.as_ref())
                .and_then(|key| self.lua.registry_value(key).ok())
        };

        if let Some(leave_fn) = leave_fn {
            let state_value: Value = {
                let conns = self.connections.borrow();
                conns.get(conn_idx)
                    .and_then(|c| c.ws_data.as_ref())
                    .and_then(|ws| ws.state_key.as_ref())
                    .and_then(|key| self.lua.registry_value(key).ok())
                    .unwrap_or(Value::Nil)
            };

            let thread = self.thread_pool.acquire(&self.lua, &leave_fn)?;
            match thread.resume::<Value>(state_value) {
                Ok(_) => {
                    self.thread_pool.release(thread);
                }
                Err(e) => {
                    warn!("WS leave handler error: {}", e);
                    self.thread_pool.release(thread);
                }
            }
        }

        // Unsubscribe from all topics
        {
            let conns = self.connections.borrow();
            self.ws_manager.borrow_mut().unsubscribe_all(conn_idx, &conns);
        }

        // Remove from endpoint tracking
        self.ws_manager.borrow_mut().remove_connection(endpoint_idx, conn_idx);

        // Remove state from Lua registry
        {
            let mut conns = self.connections.borrow_mut();
            if let Some(conn) = conns.get_mut(conn_idx) {
                if let Some(ref mut ws) = conn.ws_data {
                    if let Some(state_key) = ws.state_key.take() {
                        let _ = self.lua.remove_registry_value(state_key);
                    }
                }
            }
        }

        // Deregister and remove connection
        {
            let mut conns = self.connections.borrow_mut();
            if conns.contains(conn_idx) {
                let mut conn = conns.remove(conn_idx);
                let _ = self.poll.registry().deregister(&mut conn.socket);
            }
        }

        info!("WS connection {} disconnected from endpoint #{}", conn_idx, endpoint_idx);
        Ok(())
    }

    // ── Existing HTTP helper methods ──

    fn check_timeouts(&mut self) -> Result<()> {
        let mut to_timeout = Vec::new();

        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            // Skip WS connections -- they're long-lived
            {
                let conns = self.connections.borrow();
                if conns.get(conn_idx).map(|c| c.is_websocket()).unwrap_or(false) {
                    continue;
                }
            }
            if pending.started_at.elapsed().as_millis() as u64 > DEFAULT_COROUTINE_TIMEOUT_MS {
                to_timeout.push(conn_idx);
            }
        }

        for conn_idx in to_timeout {
            self.yielded_coroutines.remove(&conn_idx);
            let mut conns = self.connections.borrow_mut();
            if !conns.contains(conn_idx) {
                continue;
            }
            let conn = &mut conns[conn_idx];
            conn.thread = None;
            conn.state = ConnectionState::Writing;
            let buf = self.buffer_pool.get_response_buf();
            conn.set_response_with_buf(500, b"Coroutine timeout", Some("text/plain"), buf);
            let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
        }

        Ok(())
    }

    fn resume_yielded_coroutines(&mut self) -> Result<()> {
        let mut to_resume = Vec::new();

        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            if !self.connections.borrow().contains(conn_idx) {
                to_resume.push(conn_idx);
                continue;
            }

            match pending.thread.status() {
                ThreadStatus::Resumable => {
                    to_resume.push(conn_idx);
                }
                _ => {}
            }
        }

        for conn_idx in to_resume {
            if let Some(pending) = self.yielded_coroutines.remove(&conn_idx) {
                match pending.thread.resume(()) {
                    Ok(mlua::Value::Nil) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                    Ok(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                        }
                    }
                    Err(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        let mut conns = self.connections.borrow_mut();
                        if let Some(conn) = conns.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
