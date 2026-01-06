use std::io;
use std::net::SocketAddr;
use std::time::Instant;
use std::collections::HashMap;

use anyhow::Result;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use mlua::{Lua, Thread, ThreadStatus};
use slab::Slab;

use crate::connection::{Connection, ConnectionState};
use crate::fast_router::FastRouter;
use crate::{HttpMethod, Route, ServerConfig, Bytes};
use crate::http_task::{execute_handler_coroutine, CoroutineResponse};

const LISTENER: Token = Token(0);
const DEFAULT_COROUTINE_TIMEOUT_MS: u64 = 30000;

struct PendingCoroutine {
    thread: Thread,
    started_at: Instant,
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
        
        Ok(Self {
            poll,
            listener,
            connections: Slab::with_capacity(1024),
            lua,
            router,
            config,
            openapi_spec,
            yielded_coroutines: HashMap::with_capacity(1024),
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
            
            self.resume_yielded_coroutines()?;
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
                                self.poll.registry().reregister(
                                    &mut conn.socket,
                                    conn.token,
                                    Interest::READABLE,
                                )?;
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
        
        let (method_str, path_str, keep_alive) = {
            let conn = &self.connections[conn_idx];
            let method = conn.method.as_ref().map(|s| s.as_str()).unwrap_or_default();
            let full_path = conn.path.as_ref().map(|s| s.as_str()).unwrap_or_default();
            let (path, _) = if let Some(pos) = full_path.find('?') {
                (&full_path[..pos], Some(&full_path[pos+1..]))
            } else {
                (full_path, None)
            };
            (method.to_string(), path.to_string(), conn.keep_alive)
        };

        if self.config.docs && path_str == "/docs" && self.openapi_spec.is_some() {
            let html = rover_openapi::scalar_html(self.openapi_spec.as_ref().unwrap());
            let conn = &mut self.connections[conn_idx];
            conn.keep_alive = keep_alive;
            conn.set_response(200, html.as_bytes(), Some("text/html"));
            self.poll.registry().reregister(
                &mut conn.socket,
                conn.token,
                Interest::WRITABLE,
            )?;
            return Ok(());
        }

        let http_method = match HttpMethod::from_str(&method_str) {
            Some(m) => m,
            None => {
                let error_msg = format!(
                    "Invalid HTTP method '{}'. Valid methods: {}",
                    method_str,
                    HttpMethod::valid_methods().join(", ")
                );
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(400, error_msg.as_bytes(), Some("text/plain"));
                self.poll.registry().reregister(
                    &mut conn.socket,
                    conn.token,
                    Interest::WRITABLE,
                )?;
                return Ok(());
            }
        };

        let (handler, params) = match self.router.match_route(http_method, &path_str) {
            Some((h, p)) => (h.clone(), p),
            None => {
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(404, b"Route not found", Some("text/plain"));
                self.poll.registry().reregister(
                    &mut conn.socket,
                    conn.token,
                    Interest::WRITABLE,
                )?;
                return Ok(());
            }
        };

        let conn = &self.connections[conn_idx];
        let method_str_ref = conn.method.as_ref().map(|s| s.as_str()).unwrap_or_default();
        let full_path = conn.path.as_ref().map(|s| s.as_str()).unwrap_or_default();
        let (path, query_str) = if let Some(pos) = full_path.find('?') {
            (&full_path[..pos], Some(&full_path[pos+1..]))
        } else {
            (full_path, None)
        };

        let query: Vec<(Bytes, Bytes)> = if let Some(qs) = query_str {
            form_urlencoded::parse(qs.as_bytes())
                .map(|(k, v)| (Bytes::from(k.into_owned()), Bytes::from(v.into_owned())))
                .collect()
        } else {
            Vec::new()
        };

        let headers: Vec<(Bytes, Bytes)> = conn.headers
            .iter()
            .map(|(k, v)| (Bytes::from(k.clone()), Bytes::from(v.clone())))
            .collect();

        let body = conn.get_body();

        match execute_handler_coroutine(
            &self.lua,
            &handler,
            method_str_ref,
            path,
            &headers,
            &query,
            &params,
            body,
            started_at,
        ) {
            Ok(CoroutineResponse::Ready { status, body, content_type }) => {
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(status, &body, content_type.as_ref().map(|s| s.as_str()));
                
                self.poll.registry().reregister(
                    &mut conn.socket,
                    conn.token,
                    Interest::WRITABLE,
                )?;
            }
            Ok(CoroutineResponse::Yielded { thread }) => {
                let conn = &mut self.connections[conn_idx];
                conn.thread = Some(thread.clone());

                self.yielded_coroutines.insert(conn_idx, PendingCoroutine {
                    thread,
                    started_at: Instant::now(),
                });

                self.poll.registry().reregister(
                    &mut conn.socket,
                    conn.token,
                    Interest::READABLE | Interest::WRITABLE,
                )?;
            }
            Err(_) => {
                let conn = &mut self.connections[conn_idx];
                conn.keep_alive = keep_alive;
                conn.set_response(500, b"Internal server error", Some("text/plain"));
                self.poll.registry().reregister(
                    &mut conn.socket,
                    conn.token,
                    Interest::WRITABLE,
                )?;
            }
        }

        Ok(())
    }

    fn resume_yielded_coroutines(&mut self) -> Result<()> {
        let mut to_resume = Vec::new();
        let mut to_timeout = Vec::new();

        // Collect keys to process to avoid borrowing issues
        for (&conn_idx, pending) in self.yielded_coroutines.iter() {
            if !self.connections.contains(conn_idx) {
                to_resume.push(conn_idx);
                continue;
            }

            if pending.started_at.elapsed().as_millis() as u64 > DEFAULT_COROUTINE_TIMEOUT_MS {
                to_timeout.push(conn_idx);
                continue;
            }

            match pending.thread.status() {
                ThreadStatus::Resumable => {
                    to_resume.push(conn_idx);
                }
                _ => {}
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
            self.poll.registry().reregister(
                &mut conn.socket,
                conn.token,
                Interest::WRITABLE,
            )?;
        }

        // Resume coroutines
        for conn_idx in to_resume {
            if let Some(pending) = self.yielded_coroutines.remove(&conn_idx) {
                match pending.thread.resume(()) {
                    Ok(mlua::Value::Nil) => {
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
                            conn.thread = None;
                            conn.state = ConnectionState::Closed;
                        }
                    }
                    Ok(_) => {
                        if let Some(conn) = self.connections.get_mut(conn_idx) {
                            conn.thread = None;

                            self.poll.registry().reregister(
                                &mut conn.socket,
                                conn.token,
                                Interest::WRITABLE,
                            )?;
                        }
                    }
                    Err(_) => {
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
