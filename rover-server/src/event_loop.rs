use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use std::collections::HashMap;

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
                        Interest::READABLE | Interest::WRITABLE,
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

        let should_process = {
            let conn = &mut self.connections[conn_idx];

            match conn.state {
                ConnectionState::Reading if event.is_readable() => {
                    match conn.try_read() {
                        Ok(true) => true,
                        Ok(false) => false,
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            false
                        }
                    }
                }
                ConnectionState::Writing if event.is_writable() => {
                    match conn.try_write() {
                        Ok(true) => {
                            if conn.keep_alive {
                                conn.reset();
                            } else {
                                conn.state = ConnectionState::Closed;
                            }
                            false
                        }
                        Ok(false) => false,
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            false
                        }
                    }
                }
                _ => false,
            }
        };

        if matches!(self.connections.get(conn_idx).map(|c| &c.state), Some(ConnectionState::Closed)) {
            let mut conn = self.connections.remove(conn_idx);
            let _ = self.poll.registry().deregister(&mut conn.socket);
            return Ok(());
        }

        if should_process {
            self.start_request_coroutine(conn_idx)?;
        }

        Ok(())
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
            conn.set_response(200, html.as_bytes(), Some("text/html"));
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
                conn.set_response(400, error_msg.as_bytes(), Some("text/plain"));
                return Ok(());
            }
        };

        // Direct route lookup (no caching - FastRouter is fast enough)
        let (handler, params) = match self.router.match_route(http_method, path) {
            Some((h, p)) => (h, p),
            None => {
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(404, b"Route not found", Some("text/plain"));
                return Ok(());
            }
        };

        // Direct allocation for query parsing
        let query: Vec<(Bytes, Bytes)> = if let Some(qs) = query_str {
            form_urlencoded::parse(qs.as_bytes())
                .map(|(k, v)| (Bytes::copy_from_slice(k.as_bytes()), Bytes::copy_from_slice(v.as_bytes())))
                .collect()
        } else {
            Vec::new()
        };

        // Direct allocation for headers
        let header_count = conn.header_offsets.len();
        let mut headers = Vec::with_capacity(header_count);
        for (k, v) in conn.headers_iter() {
            headers.push((
                Bytes::copy_from_slice(k.as_bytes()),
                Bytes::copy_from_slice(v.as_bytes()),
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
                // No longer pooling headers
                if !query.is_empty() {
                    // No longer pooling query
                }

                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                // Use set_response_bytes for true zero-copy (body is already Bytes)

                conn.set_response_bytes(status, body, content_type);

            }
            Ok(CoroutineResponse::Yielded { thread, ctx_idx }) => {
                // Return buffers to pool
                // No longer pooling headers
                if !query.is_empty() {
                    // No longer pooling headers
                }

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
                // No longer pooling headers
                if !query.is_empty() {
                    // No longer pooling query
                }

                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(500, b"Internal server error", Some("text/plain"));
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
            conn.set_response(500, b"Coroutine timeout", Some("text/plain"));
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
