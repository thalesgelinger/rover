use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::mem;

use anyhow::Result;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use mlua::{Lua, Thread, ThreadStatus};
use slab::Slab;

use crate::connection::{Connection, ConnectionState};
use crate::fast_router::FastRouter;
use crate::{HttpMethod, Route, ServerConfig, Bytes};
use crate::http_task::{execute_handler_coroutine, CoroutineResponse, ThreadPool, RequestContextPool};
use crate::buffer_pool::BufferPool;
use crate::table_pool::LuaTablePool;

const LISTENER: Token = Token(0);
const DEFAULT_COROUTINE_TIMEOUT_MS: u64 = 30000;
const TIMEOUT_CHECK_INTERVAL: Duration = Duration::from_millis(100);

struct PendingCoroutine {
    thread: Thread,
    started_at: Instant,
    ctx_idx: usize,
}

pub struct EventLoop {
    poll: Poll,
    listener: TcpListener,
    connections: Slab<Connection>,
    lua: Lua,
    router: FastRouter,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
    yielded_coroutines: HashMap<usize, PendingCoroutine>,
    last_timeout_check: Instant,
    thread_pool: ThreadPool,
    request_pool: RequestContextPool,
    table_pool: LuaTablePool,
    buffer_pool: BufferPool,
}

impl EventLoop {
    pub fn new(
        lua: Lua,
        routes: Vec<Route>,
        config: ServerConfig,
        openapi_spec: Option<serde_json::Value>,
        addr: SocketAddr,
    ) -> Result<Self> {
        let poll = Poll::new()?;
        let mut listener = TcpListener::bind(addr)?;

        poll.registry().register(&mut listener, LISTENER, Interest::READABLE)?;

        let router = FastRouter::from_routes(routes)?;

        let request_pool = RequestContextPool::new(&lua, 1024)?;

        let table_pool = LuaTablePool::new(1024);

        let buffer_pool = BufferPool::new();

        Ok(Self {
            poll,
            listener,
            connections: Slab::with_capacity(1024),
            lua,
            router,
            config,
            openapi_spec,
            yielded_coroutines: HashMap::with_capacity(1024),
            last_timeout_check: Instant::now(),
            thread_pool: ThreadPool::new(2048),
            request_pool,
            table_pool,
            buffer_pool,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let mut events = Events::with_capacity(1024);

        loop {
            // Block on real events only (pure event-driven)
            self.poll.poll(&mut events, None)?;

            for event in events.iter() {
                match event.token() {
                    LISTENER => self.accept_connections()?,
                    token => self.handle_connection(token, event)?,
                }
            }

            // Always check timeouts (triggered by poll timeout or I/O completion)
            self.check_timeouts()?;

            // Resume yielded coroutines
            if !self.yielded_coroutines.is_empty() {
                self.resume_yielded_coroutines()?;
            }
        }
    }

    fn accept_connections(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut socket, _addr)) => {
                    let entry = self.connections.vacant_entry();
                    let token = Token(entry.key() + 1);

                    self.poll.registry().register(
                        &mut socket,
                        token,
                        Interest::READABLE,
                    )?;

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

        if !self.connections.contains(conn_idx) {
            return Ok(());
        }

        let (should_process, should_close, should_reset) = {
            let conn = &mut self.connections[conn_idx];

            match conn.state {
                ConnectionState::Reading if event.is_readable() => {
                    match conn.try_read() {
                        Ok(true) => (true, false, false),
                        Ok(false) => (false, false, false),
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            (false, true, false)
                        }
                    }
                }
                ConnectionState::Writing if event.is_writable() => {
                    match conn.try_write() {
                        Ok(true) => {
                            if conn.keep_alive {
                                (false, false, true)
                            } else {
                                conn.state = ConnectionState::Closed;
                                (false, true, false)
                            }
                        }
                        Ok(false) => (false, false, false),
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            (false, true, false)
                        }
                    }
                }
                _ => (false, false, false),
            }
        };

        let is_closed_state = matches!(self.connections.get(conn_idx).map(|c| &c.state), Some(ConnectionState::Closed));
        if should_close || is_closed_state {
            self.recycle_write_buf(conn_idx);
            let mut conn = self.connections.remove(conn_idx);
            let _ = self.poll.registry().deregister(&mut conn.socket);
            return Ok(());
        }

        if should_reset {
            self.recycle_write_buf(conn_idx);
            if let Some(conn) = self.connections.get_mut(conn_idx) {
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
        if let Some(conn) = self.connections.get_mut(conn_idx) {
            let buf = mem::take(&mut conn.write_buf);
            if !buf.is_empty() {
                self.buffer_pool.return_response_buf(buf);
            }
        }
    }

    fn start_request_coroutine(&mut self, conn_idx: usize) -> Result<()> {
        let started_at = Instant::now();

        let conn = &self.connections[conn_idx];
        let method = conn.method_str().unwrap_or_default();
        let full_path = conn.path_str().unwrap_or_default();
        let (path, query_str) = if let Some(pos) = full_path.find('?') {
            (&full_path[..pos], Some(&full_path[pos+1..]))
        } else {
            (full_path, None)
        };
        let keep_alive = conn.keep_alive;

        if self.config.docs && path == "/docs" && self.openapi_spec.is_some() {
            let html = rover_openapi::scalar_html(self.openapi_spec.as_ref().unwrap());
            let conn = &mut self.connections[conn_idx];
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
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(400, error_msg.as_bytes(), Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        // Direct route lookup (no caching - FastRouter is fast enough)
        let (handler, params) = match self.router.match_route(http_method, path) {
            Some((h, p)) => (h, p),
            None => {
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(404, b"Route not found", Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
                return Ok(());
            }
        };

        // Acquire query from pool (estimated size from query string length)
        let query_expected_size = query_str.map_or(0, |qs| qs.len() / 10);
        let mut query = self.buffer_pool.get_bytes_pairs(query_expected_size);
        if let Some(qs) = query_str {
            for (k, v) in form_urlencoded::parse(qs.as_bytes()) {
                query.push((Bytes::copy_from_slice(k.as_bytes()), Bytes::copy_from_slice(v.as_bytes())));
            }
        }

        // Acquire headers from pool (known exact size from connection)
        let header_count = conn.header_offsets.len();
        let mut headers = self.buffer_pool.get_bytes_pairs(header_count);
        let header_buf = if !conn.parsed_buf.is_empty() {
            conn.parsed_buf.clone()
        } else {
            conn.read_buf.clone().freeze()
        };
        for &(name_off, name_len, val_off, val_len) in conn.header_offsets.iter() {
            headers.push((
                header_buf.slice(name_off..name_off + name_len),
                header_buf.slice(val_off..val_off + val_len),
            ));
        }

        let body = conn.get_body();

        match execute_handler_coroutine(
            &self.lua,
            handler,
            method.as_bytes(),
            path.as_bytes(),
            &headers,
            &query,
            &params,
            body,
            started_at,
            &mut self.thread_pool,
            &mut self.request_pool,
            &self.table_pool,
        ) {
            Ok(CoroutineResponse::Ready { status, body, content_type }) => {
                // Return buffers to pool
                self.buffer_pool.return_bytes_pairs(headers);
                self.buffer_pool.return_bytes_pairs(query);

                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                // Use set_response_bytes for true zero-copy (body is already Bytes)
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_bytes_with_buf(status, body, content_type, buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);

            }
            Ok(CoroutineResponse::Yielded { thread, ctx_idx }) => {
                // Return buffers to pool (coroutine only needs ctx_idx)
                self.buffer_pool.return_bytes_pairs(headers);
                self.buffer_pool.return_bytes_pairs(query);

                let conn = &mut self.connections[conn_idx];
                conn.thread = Some(thread.clone());

                self.yielded_coroutines.insert(conn_idx, PendingCoroutine {
                    thread,
                    started_at: Instant::now(),
                    ctx_idx,
                });
            }
            Err(_) => {
                // Return buffers to pool
                self.buffer_pool.return_bytes_pairs(headers);
                self.buffer_pool.return_bytes_pairs(query);

                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                let buf = self.buffer_pool.get_response_buf();
                conn.set_response_with_buf(500, b"Internal server error", Some("text/plain"), buf);
                let _ = conn.reregister(&self.poll.registry(), Interest::WRITABLE);
            }
        }

        Ok(())
    }

    fn check_timeouts(&mut self) -> Result<()> {
        let mut to_timeout = Vec::new();

        // Collect timed-out coroutines
        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            if pending.started_at.elapsed().as_millis() as u64 > DEFAULT_COROUTINE_TIMEOUT_MS {
                to_timeout.push(conn_idx);
            }
        }

        // Handle timeouts
        for conn_idx in to_timeout {
            self.yielded_coroutines.remove(&conn_idx);
            if !self.connections.contains(conn_idx) {
                continue;
            }
            let conn = &mut self.connections[conn_idx];
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

        // Collect resumable coroutines
        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            if !self.connections.contains(conn_idx) {
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

        // Resume coroutines
        for conn_idx in to_resume {
            if let Some(pending) = self.yielded_coroutines.remove(&conn_idx) {
                match pending.thread.resume(()) {
                    Ok(mlua::Value::Nil) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                    Ok(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
                            conn.thread = None;
                        }
                    }
                    Err(_) => {
                        self.thread_pool.release(pending.thread);
                        self.request_pool.release(pending.ctx_idx);
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
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
