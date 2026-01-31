/// Central WebSocket connection and subscription manager.
///
/// Owns endpoint configurations, topic pub/sub state, per-endpoint connection
/// tracking, and frame buffer pools. Single-threaded -- context fields are safe
/// because the mio event loop is non-preemptive.

use std::collections::HashMap;

use ahash::AHashMap;
use mlua::RegistryKey;
use slab::Slab;

use crate::connection::Connection;

/// Configuration extracted from a single `function api.x.ws(ws)` setup call.
/// Created once at server startup, immutable during runtime.
pub struct WsEndpointConfig {
    pub join_handler: Option<RegistryKey>,
    pub leave_handler: Option<RegistryKey>,
    /// Event name -> RegistryKey for listen handler functions. O(1) dispatch.
    pub event_handlers: AHashMap<String, RegistryKey>,
    /// The Lua ws table itself (needed at runtime for ws.send context).
    pub ws_table_key: RegistryKey,
}

/// Topic (pub/sub channel) state.
struct TopicState {
    #[allow(dead_code)]
    name: String,
    /// Connection indices subscribed to this topic.
    members: Vec<usize>,
}

const FRAME_BUF_POOL_SIZE: usize = 64;
const FRAME_BUF_INITIAL_CAP: usize = 256;

pub struct WsManager {
    /// Registered WS endpoint configurations (indexed by endpoint_idx).
    pub endpoints: Vec<WsEndpointConfig>,

    /// Per-endpoint connection tracking: endpoint_idx -> [conn_idx].
    /// Avoids full slab scan for :all broadcasts.
    endpoint_connections: Vec<Vec<usize>>,

    /// Topic name -> topic index for O(1) lookup.
    topic_index: AHashMap<String, u16>,
    /// Topic states indexed by topic_idx.
    topics: Vec<TopicState>,

    /// Frame buffer pool -- pre-allocated Vec<u8> for building outgoing frames.
    frame_bufs: Vec<Vec<u8>>,

    /// Per-handler-call context: which connection is currently executing.
    /// Safe: single-threaded, non-preemptive (set before Lua call, read during).
    pub current_conn_idx: usize,
    pub current_endpoint_idx: u16,
}

impl WsManager {
    pub fn new() -> Self {
        let mut frame_bufs = Vec::with_capacity(FRAME_BUF_POOL_SIZE);
        for _ in 0..FRAME_BUF_POOL_SIZE {
            frame_bufs.push(Vec::with_capacity(FRAME_BUF_INITIAL_CAP));
        }

        Self {
            endpoints: Vec::new(),
            endpoint_connections: Vec::new(),
            topic_index: AHashMap::new(),
            topics: Vec::new(),
            frame_bufs,
            current_conn_idx: 0,
            current_endpoint_idx: 0,
        }
    }

    /// Register a new WebSocket endpoint. Returns the endpoint index.
    pub fn register_endpoint(&mut self, config: WsEndpointConfig) -> u16 {
        let idx = self.endpoints.len() as u16;
        self.endpoints.push(config);
        self.endpoint_connections.push(Vec::new());
        idx
    }

    /// Track a new WS connection for the given endpoint.
    pub fn add_connection(&mut self, endpoint_idx: u16, conn_idx: usize) {
        if let Some(conns) = self.endpoint_connections.get_mut(endpoint_idx as usize) {
            conns.push(conn_idx);
        }
    }

    /// Remove a WS connection from endpoint tracking.
    pub fn remove_connection(&mut self, endpoint_idx: u16, conn_idx: usize) {
        if let Some(conns) = self.endpoint_connections.get_mut(endpoint_idx as usize) {
            if let Some(pos) = conns.iter().position(|&c| c == conn_idx) {
                conns.swap_remove(pos);
            }
        }
    }

    /// Subscribe a connection to a topic. Creates the topic if new.
    /// Returns the topic index.
    pub fn subscribe(&mut self, conn_idx: usize, topic: &str) -> u16 {
        if let Some(&idx) = self.topic_index.get(topic) {
            let state = &mut self.topics[idx as usize];
            if !state.members.contains(&conn_idx) {
                state.members.push(conn_idx);
            }
            idx
        } else {
            let idx = self.topics.len() as u16;
            self.topics.push(TopicState {
                name: topic.to_string(),
                members: vec![conn_idx],
            });
            self.topic_index.insert(topic.to_string(), idx);
            idx
        }
    }

    /// Unsubscribe a connection from a specific topic.
    pub fn unsubscribe(&mut self, conn_idx: usize, topic_idx: u16) {
        if let Some(state) = self.topics.get_mut(topic_idx as usize) {
            if let Some(pos) = state.members.iter().position(|&c| c == conn_idx) {
                state.members.swap_remove(pos);
            }
        }
    }

    /// Unsubscribe a connection from ALL its topics. Called during disconnect.
    pub fn unsubscribe_all(&mut self, conn_idx: usize, connections: &Slab<Connection>) {
        if let Some(conn) = connections.get(conn_idx) {
            if let Some(ref ws) = conn.ws_data {
                for &topic_idx in ws.subscriptions.iter() {
                    if let Some(state) = self.topics.get_mut(topic_idx as usize) {
                        if let Some(pos) = state.members.iter().position(|&c| c == conn_idx) {
                            state.members.swap_remove(pos);
                        }
                    }
                }
            }
        }
    }

    /// Get all connection indices subscribed to a topic.
    #[inline]
    pub fn get_topic_members(&self, topic: &str) -> Option<&[usize]> {
        self.topic_index
            .get(topic)
            .and_then(|&idx| self.topics.get(idx as usize))
            .map(|s| s.members.as_slice())
    }

    /// Get all connection indices for an endpoint (for :all broadcasts).
    #[inline]
    pub fn get_endpoint_connections(&self, endpoint_idx: u16) -> &[usize] {
        self.endpoint_connections
            .get(endpoint_idx as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get a pooled frame buffer for building outgoing frames.
    #[inline]
    pub fn get_frame_buf(&mut self) -> Vec<u8> {
        self.frame_bufs
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(FRAME_BUF_INITIAL_CAP))
    }

    /// Return a frame buffer to the pool.
    #[inline]
    pub fn return_frame_buf(&mut self, mut buf: Vec<u8>) {
        buf.clear();
        if self.frame_bufs.len() < FRAME_BUF_POOL_SIZE {
            self.frame_bufs.push(buf);
        }
    }

    /// Set per-handler-call context before invoking a Lua callback.
    #[inline]
    pub fn set_context(&mut self, conn_idx: usize, endpoint_idx: u16) {
        self.current_conn_idx = conn_idx;
        self.current_endpoint_idx = endpoint_idx;
    }
}
