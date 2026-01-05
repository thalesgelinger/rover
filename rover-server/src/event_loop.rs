use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::Instant;

use anyhow::Result;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use mlua::{Lua, Function, Table, Value};
use slab::Slab;
use smallvec::SmallVec;
use tracing::{debug, info, warn};

use crate::connection::{Connection, ConnectionState};
use crate::fast_router::FastRouter;
use crate::{HttpMethod, Route, ServerConfig, Bytes};
use crate::http_task::execute_handler;

const LISTENER: Token = Token(0);
const MAX_CONNECTIONS: usize = 10_000;

pub struct EventLoop {
    poll: Poll,
    listener: TcpListener,
    connections: Slab<Connection>,
    lua: Lua,
    router: FastRouter,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>,
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
        }
    }

    fn accept_connections(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut socket, _addr)) => {
                    if self.connections.len() >= MAX_CONNECTIONS {
                        // Drop connection - at capacity
                        drop(socket);
                        continue;
                    }
                    
                    let entry = self.connections.vacant_entry();
                    let token = Token(entry.key() + 1); // +1 because 0 is LISTENER
                    
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
        let conn_idx = token.0 - 1; // -1 because 0 is LISTENER
        
        if !self.connections.contains(conn_idx) {
            return Ok(());
        }

        // Handle based on current state
        let should_process = {
            let conn = &mut self.connections[conn_idx];
            
            match conn.state {
                ConnectionState::Reading if event.is_readable() => {
                    match conn.try_read() {
                        Ok(true) => true, // Request complete, process it
                        Ok(false) => false, // Need more data or closed
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            false
                        }
                    }
                }
                ConnectionState::Writing if event.is_writable() => {
                    match conn.try_write() {
                        Ok(true) => {
                            // Write complete
                            if conn.keep_alive {
                                conn.reset();
                                // Re-register for reading
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
                        Ok(false) => false, // More to write
                        Err(_) => {
                            conn.state = ConnectionState::Closed;
                            false
                        }
                    }
                }
                _ => false,
            }
        };

        // Check if connection should be removed
        if matches!(self.connections.get(conn_idx).map(|c| &c.state), Some(ConnectionState::Closed)) {
            let mut conn = self.connections.remove(conn_idx);
            let _ = self.poll.registry().deregister(&mut conn.socket);
            return Ok(());
        }

        // Process complete request
        if should_process {
            self.process_request(conn_idx)?;
        }

        Ok(())
    }

    fn process_request(&mut self, conn_idx: usize) -> Result<()> {
        let started_at = Instant::now();
        
        // Extract request data
        let (method_str, path_str, headers, query, body, keep_alive) = {
            let conn = &self.connections[conn_idx];
            let method = conn.method.clone().unwrap_or_default();
            let full_path = conn.path.clone().unwrap_or_default();
            
            // Parse path and query
            let (path, query_str) = if let Some(pos) = full_path.find('?') {
                (&full_path[..pos], Some(&full_path[pos+1..]))
            } else {
                (full_path.as_str(), None)
            };
            
            // Parse query string
            let query: SmallVec<[(Bytes, Bytes); 8]> = if let Some(qs) = query_str {
                form_urlencoded::parse(qs.as_bytes())
                    .map(|(k, v)| (Bytes::from(k.into_owned()), Bytes::from(v.into_owned())))
                    .collect()
            } else {
                SmallVec::new()
            };
            
            // Convert headers
            let headers: SmallVec<[(Bytes, Bytes); 8]> = conn.headers
                .iter()
                .map(|(k, v)| (Bytes::from(k.clone()), Bytes::from(v.clone())))
                .collect();
            
            (method, path.to_string(), headers, query, conn.body.clone(), conn.keep_alive)
        };

        // Handle special routes
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

        // Parse HTTP method
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

        // Route matching
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

        // Execute handler
        let method_bytes = Bytes::from(method_str);
        let path_bytes = Bytes::from(path_str);
        let body_bytes = body.map(Bytes::from);

        let response = execute_handler(
            &self.lua,
            &handler,
            method_bytes,
            path_bytes,
            headers,
            query,
            params,
            body_bytes,
            started_at,
        )?;

        // Set response
        let conn = &mut self.connections[conn_idx];
        conn.keep_alive = keep_alive;
        conn.set_response(
            response.status,
            &response.body,
            response.content_type.as_deref(),
        );
        
        self.poll.registry().reregister(
            &mut conn.socket,
            conn.token,
            Interest::WRITABLE,
        )?;

        Ok(())
    }
}
