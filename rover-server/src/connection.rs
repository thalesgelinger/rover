use crate::Bytes;
use crate::ws_frame;
use bytes::BytesMut;
use mio::net::TcpStream;
use mio::{Interest, Registry, Token};
use mlua::{RegistryKey, Thread};
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::io::{IoSlice, Read, Write};
use std::time::Instant;

const READ_BUF_SIZE: usize = 4096;
const MAX_HEADERS: usize = 32;

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    Reading,
    Writing,
    WsActive,
    WsClosed,
    Closed,
}

/// WebSocket-specific per-connection data. Only allocated after HTTP upgrade.
pub struct WsConnectionData {
    /// Which WsEndpoint this connection belongs to
    pub endpoint_idx: u16,
    /// Lua state returned by ws.join(), stored in registry
    pub state_key: Option<RegistryKey>,
    /// Pre-built frames waiting to be written (Bytes is ref-counted, clone is O(1))
    pub write_queue: VecDeque<Bytes>,
    /// Current write position within the front frame
    pub write_pos: usize,
    /// Fragment accumulator for multi-frame messages
    pub fragment_buf: Option<Vec<u8>>,
    /// Subscribed topic indices (inline for <=4 topics, no heap)
    pub subscriptions: SmallVec<[u16; 4]>,
    /// Whether a close frame has been sent
    pub close_sent: bool,
    /// Opcode of the first fragment (for continuation frames)
    pub fragment_opcode: Option<ws_frame::WsOpcode>,
}

pub struct Connection {
    pub socket: TcpStream,
    pub token: Token,
    pub state: ConnectionState,

    pub read_buf: BytesMut,
    pub parsed_buf: Bytes,
    pub read_pos: usize,

    pub write_buf: Vec<u8>,
    pub write_pos: usize,
    pub body_buf: Bytes,
    pub body_pos: usize,

    pub method_offset: Option<(usize, usize)>,
    pub path_offset: Option<(usize, usize)>,
    pub header_offsets: Vec<(usize, usize, usize, usize)>,
    pub body: Option<(usize, usize)>,
    pub content_length: usize,
    pub headers_complete: bool,
    pub keep_alive: bool,

    pub thread: Option<Thread>,

    yielded_at: Option<Instant>,
    request_ctx_idx: Option<usize>,

    /// WebSocket state -- only populated after upgrade (Option avoids cost for HTTP conns)
    pub ws_data: Option<Box<WsConnectionData>>,
    /// Pending WS upgrade: endpoint_idx set during 101 write, consumed after write completes
    pub pending_ws_upgrade: Option<u16>,
}

impl Connection {
    pub fn new(socket: TcpStream, token: Token) -> Self {
        Self {
            socket,
            token,
            state: ConnectionState::Reading,
            read_buf: BytesMut::with_capacity(READ_BUF_SIZE * 2),
            parsed_buf: Bytes::new(),
            read_pos: 0,
            write_buf: Vec::with_capacity(512),
            write_pos: 0,
            body_buf: Bytes::new(),
            body_pos: 0,
            method_offset: None,
            path_offset: None,
            header_offsets: Vec::with_capacity(16),
            body: None,
            content_length: 0,
            headers_complete: false,
            keep_alive: true,
            thread: None,
            yielded_at: None,
            request_ctx_idx: None,
            ws_data: None,
            pending_ws_upgrade: None,
        }
    }

    pub fn reregister(&mut self, registry: &Registry, interest: Interest) -> std::io::Result<()> {
        registry.reregister(&mut self.socket, self.token, interest)
    }

    pub fn method_str(&self) -> Option<&str> {
        let buf: &[u8] = if !self.parsed_buf.is_empty() {
            &self.parsed_buf
        } else {
            &self.read_buf
        };

        self.method_offset
            .map(|(off, len)| unsafe { std::str::from_utf8_unchecked(&buf[off..off + len]) })
    }

    pub fn path_str(&self) -> Option<&str> {
        let buf: &[u8] = if !self.parsed_buf.is_empty() {
            &self.parsed_buf
        } else {
            &self.read_buf
        };

        self.path_offset
            .map(|(off, len)| unsafe { std::str::from_utf8_unchecked(&buf[off..off + len]) })
    }

    pub fn try_read(&mut self) -> std::io::Result<bool> {
        if self.read_buf.len() < self.read_pos + 1024 {
            self.read_buf.resize(self.read_pos + READ_BUF_SIZE, 0);
        }

        loop {
            match self.socket.read(&mut self.read_buf[self.read_pos..]) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    self.read_pos += n;

                    if self.try_parse()? {
                        return Ok(true);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn try_parse(&mut self) -> std::io::Result<bool> {
        if !self.headers_complete {
            let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
            let mut req = httparse::Request::new(&mut headers);

            let parse_buf = &self.read_buf[..self.read_pos];
            match req.parse(parse_buf) {
                Ok(httparse::Status::Complete(header_len)) => {
                    self.headers_complete = true;

                    let base_ptr = parse_buf.as_ptr();
                    let buf_len = parse_buf.len();
                    let calc_offset = |ptr: *const u8, len: usize| -> Option<usize> {
                        let diff = unsafe { ptr.offset_from(base_ptr) };
                        if diff < 0 {
                            return None;
                        }
                        let offset = diff as usize;
                        if offset + len <= buf_len {
                            Some(offset)
                        } else {
                            None
                        }
                    };

                    self.method_offset = req.method.as_ref().and_then(|s| {
                        calc_offset(s.as_ptr(), s.len()).map(|start| (start, s.len()))
                    });
                    self.path_offset = req.path.as_ref().and_then(|s| {
                        calc_offset(s.as_ptr(), s.len()).map(|start| (start, s.len()))
                    });

                    if self.method_offset.is_none() || self.path_offset.is_none() {
                        self.state = ConnectionState::Closed;
                        return Ok(false);
                    }

                    self.header_offsets.clear();
                    for header in req.headers.iter() {
                        let Some(h_name_start) =
                            calc_offset(header.name.as_ptr(), header.name.len())
                        else {
                            self.state = ConnectionState::Closed;
                            return Ok(false);
                        };
                        let Some(h_val_start) =
                            calc_offset(header.value.as_ptr(), header.value.len())
                        else {
                            self.state = ConnectionState::Closed;
                            return Ok(false);
                        };

                        let h_name_len = header.name.len();
                        let h_val_len = header.value.len();
                        let name_str = unsafe {
                            std::str::from_utf8_unchecked(
                                &self.read_buf[h_name_start..h_name_start + h_name_len],
                            )
                        };
                        let value_bytes = &self.read_buf[h_val_start..h_val_start + h_val_len];
                        let value_str = unsafe { std::str::from_utf8_unchecked(value_bytes) };

                        if name_str.eq_ignore_ascii_case("content-length") {
                            self.content_length = value_str.trim().parse().unwrap_or(0);
                        } else if name_str.eq_ignore_ascii_case("connection") {
                            self.keep_alive = !value_str.trim().eq_ignore_ascii_case("close");
                        }

                        self.header_offsets.push((
                            h_name_start,
                            h_name_len,
                            h_val_start,
                            h_val_len,
                        ));
                    }

                    let body_start = header_len;
                    let body_received = self.read_pos - body_start;

                    if body_received >= self.content_length {
                        if self.content_length > 0 {
                            self.body = Some((body_start, self.content_length));
                        }
                        if self.parsed_buf.is_empty() && self.read_pos > 0 {
                            self.parsed_buf = self.read_buf.split_to(self.read_pos).freeze();
                            self.read_pos = 0;
                        }
                        return Ok(true);
                    }
                }
                Ok(httparse::Status::Partial) => {
                    return Ok(false);
                }
                Err(_) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
            }
        } else {
            if let Some(pos) = self.find_header_end() {
                let body_received = self.read_pos - pos;
                if body_received >= self.content_length {
                    if self.content_length > 0 {
                        self.body = Some((pos, self.content_length));
                    }
                    if self.parsed_buf.is_empty() && self.read_pos > 0 {
                        self.parsed_buf = self.read_buf.split_to(self.read_pos).freeze();
                        self.read_pos = 0;
                    }
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn find_header_end(&self) -> Option<usize> {
        for i in 0..self.read_pos.saturating_sub(3) {
            if &self.read_buf[i..i + 4] == b"\r\n\r\n" {
                return Some(i + 4);
            }
        }
        None
    }

    pub fn try_write(&mut self) -> std::io::Result<bool> {
        // Calculate remaining data in each buffer
        let header_remaining = self.write_buf.len().saturating_sub(self.write_pos);
        let body_remaining = self.body_buf.len().saturating_sub(self.body_pos);

        // Fast path: everything already written
        if header_remaining == 0 && body_remaining == 0 {
            return Ok(true);
        }

        // Try vectored I/O first (single syscall for header + body)
        if header_remaining > 0 && body_remaining > 0 {
            let slices = [
                IoSlice::new(&self.write_buf[self.write_pos..]),
                IoSlice::new(&self.body_buf[self.body_pos..]),
            ];

            match self.socket.write_vectored(&slices) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    // Distribute written bytes across buffers
                    if n <= header_remaining {
                        self.write_pos += n;
                    } else {
                        self.write_pos = self.write_buf.len();
                        self.body_pos += n - header_remaining;
                    }

                    // Check if done
                    if self.write_pos >= self.write_buf.len()
                        && self.body_pos >= self.body_buf.len()
                    {
                        return Ok(true);
                    }
                    // Continue in the loop for remaining data
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }

        // Finish writing headers if needed
        while self.write_pos < self.write_buf.len() {
            match self.socket.write(&self.write_buf[self.write_pos..]) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    self.write_pos += n;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }

        // Finish writing body if needed
        while self.body_pos < self.body_buf.len() {
            match self.socket.write(&self.body_buf[self.body_pos..]) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    self.body_pos += n;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(true)
    }

    pub fn set_response_with_buf(
        &mut self,
        status: u16,
        body: &[u8],
        content_type: Option<&str>,
        mut buf: Vec<u8>,
    ) {
        buf.clear();
        self.write_buf = buf;
        self.write_pos = 0;
        self.body_pos = 0;

        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        let ct = content_type.unwrap_or("text/plain");
        let conn = if self.keep_alive {
            "keep-alive"
        } else {
            "close"
        };

        // Build headers only (body stored separately for vectored I/O)
        write!(
            self.write_buf,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status,
            status_text,
            ct,
            body.len(),
            conn
        )
        .unwrap();

        // Store body separately (zero-copy via Bytes)
        self.body_buf = Bytes::copy_from_slice(body);
        self.state = ConnectionState::Writing;
    }

    /// Set response with pre-allocated Bytes body (true zero-copy)
    pub fn set_response_bytes_with_buf(
        &mut self,
        status: u16,
        body: Bytes,
        content_type: Option<&str>,
        mut buf: Vec<u8>,
    ) {
        buf.clear();
        self.write_buf = buf;
        self.write_pos = 0;
        self.body_pos = 0;

        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        let ct = content_type.unwrap_or("text/plain");
        let conn = if self.keep_alive {
            "keep-alive"
        } else {
            "close"
        };

        // Build headers only
        write!(
            self.write_buf,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status,
            status_text,
            ct,
            body.len(),
            conn
        )
        .unwrap();

        // Store body directly (true zero-copy - no slice copy)
        self.body_buf = body;
        self.state = ConnectionState::Writing;
    }

    pub fn reset(&mut self) {
        self.read_buf.clear();
        self.parsed_buf = Bytes::new();
        self.read_pos = 0;
        self.write_buf.clear();
        self.write_pos = 0;
        self.body_buf = Bytes::new();
        self.body_pos = 0;
        self.method_offset = None;
        self.path_offset = None;
        self.header_offsets.clear();
        self.body = None;
        self.content_length = 0;
        self.headers_complete = false;
        self.state = ConnectionState::Reading;
        self.thread = None;
        self.yielded_at = None;
        self.request_ctx_idx = None;
    }

    // ── WebSocket methods ──

    #[inline]
    pub fn is_websocket(&self) -> bool {
        self.ws_data.is_some()
    }

    /// Transition from HTTP to WebSocket after 101 response is fully written.
    /// Clears HTTP parsing state, initializes WsConnectionData.
    pub fn upgrade_to_ws(&mut self, endpoint_idx: u16) {
        self.read_buf.clear();
        self.parsed_buf = Bytes::new();
        self.read_pos = 0;
        self.write_buf.clear();
        self.write_pos = 0;
        self.body_buf = Bytes::new();
        self.body_pos = 0;
        self.method_offset = None;
        self.path_offset = None;
        self.header_offsets.clear();
        self.body = None;
        self.content_length = 0;
        self.headers_complete = false;
        self.thread = None;
        self.pending_ws_upgrade = None;

        self.ws_data = Some(Box::new(WsConnectionData {
            endpoint_idx,
            state_key: None,
            write_queue: VecDeque::with_capacity(8),
            write_pos: 0,
            fragment_buf: None,
            subscriptions: SmallVec::new(),
            close_sent: false,
            fragment_opcode: None,
        }));

        self.state = ConnectionState::WsActive;
    }

    /// Read data from the socket into read_buf for WebSocket frame parsing.
    /// Returns Ok(bytes_read) or Err. Returns Ok(0) on EOF.
    pub fn ws_read(&mut self) -> std::io::Result<usize> {
        if self.read_buf.len() < self.read_pos + 1024 {
            self.read_buf.resize(self.read_pos + READ_BUF_SIZE, 0);
        }

        let mut total = 0;
        loop {
            match self.socket.read(&mut self.read_buf[self.read_pos..]) {
                Ok(0) => return Ok(total),
                Ok(n) => {
                    self.read_pos += n;
                    total += n;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(total);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Queue a pre-built WebSocket frame for writing. Frame is ref-counted Bytes.
    #[inline]
    pub fn queue_ws_frame(&mut self, frame: Bytes) {
        if let Some(ref mut ws) = self.ws_data {
            ws.write_queue.push_back(frame);
        }
    }

    /// Drain the WebSocket write queue to the socket.
    /// Returns Ok(true) when queue is fully drained, Ok(false) on WouldBlock.
    pub fn try_write_ws(&mut self) -> std::io::Result<bool> {
        let ws = match self.ws_data {
            Some(ref mut ws) => ws,
            None => return Ok(true),
        };

        while let Some(front) = ws.write_queue.front() {
            let remaining = &front[ws.write_pos..];
            if remaining.is_empty() {
                ws.write_queue.pop_front();
                ws.write_pos = 0;
                continue;
            }

            match self.socket.write(remaining) {
                Ok(0) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    ws.write_pos += n;
                    if ws.write_pos >= front.len() {
                        ws.write_queue.pop_front();
                        ws.write_pos = 0;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(true) // queue fully drained
    }
}
