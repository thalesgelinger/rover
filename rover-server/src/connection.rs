use crate::Bytes;
use crate::ws_frame;
use bytes::BytesMut;
use mio::net::TcpStream;
use mio::{Interest, Registry, Token};
use mlua::{RegistryKey, Thread};
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::io::{IoSlice, Read, Write};
use std::sync::Arc;
use std::time::Instant;

const READ_BUF_SIZE: usize = 4096;
const MAX_HEADERS: usize = 32;

fn trim_ows_bytes(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|b| !matches!(b, b' ' | b'\t'))
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &value[start..end]
}

fn parse_content_length(value: &[u8]) -> Option<usize> {
    let trimmed = trim_ows_bytes(value);
    if trimmed.is_empty() || trimmed.iter().any(|b| !b.is_ascii_digit()) {
        return None;
    }

    let mut parsed: usize = 0;
    for digit in trimmed {
        parsed = parsed.checked_mul(10)?;
        parsed = parsed.checked_add((digit - b'0') as usize)?;
    }
    Some(parsed)
}

fn header_value_has_token(value: &[u8], token: &str) -> bool {
    value.split(|b| *b == b',').any(|part| {
        let trimmed = trim_ows_bytes(part);
        trimmed.eq_ignore_ascii_case(token.as_bytes())
    })
}

fn extract_content_length(headers: &[httparse::Header<'_>]) -> Option<usize> {
    let mut content_length = None;
    let mut has_transfer_encoding = false;

    for header in headers {
        if header.name.eq_ignore_ascii_case("transfer-encoding") {
            has_transfer_encoding = true;
            continue;
        }

        if header.name.eq_ignore_ascii_case("content-length") {
            let parsed = parse_content_length(header.value)?;
            if let Some(existing) = content_length {
                if existing != parsed {
                    return None;
                }
            } else {
                content_length = Some(parsed);
            }
        }
    }

    if has_transfer_encoding {
        return None;
    }

    Some(content_length.unwrap_or(0))
}

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    Reading,
    Writing,
    /// Streaming response: writing headers
    StreamingHeaders,
    /// Streaming response: writing chunk data
    StreamingBody,
    /// SSE connection: active event stream
    SseActive,
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

/// SSE-specific per-connection data. Only allocated after SSE endpoint starts.
pub struct SseConnectionData {
    /// The event producer registry key
    pub event_producer: Arc<mlua::RegistryKey>,
    /// Reconnect hint in milliseconds
    pub retry_ms: u32,
    /// Whether the reconnect hint still needs to be sent
    pub retry_pending: bool,
    /// Keepalive interval in milliseconds (0 = disabled)
    pub keepalive_ms: u32,
    /// Time of last write (for keepalive)
    pub last_write: Option<Instant>,
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

    /// SSE (Server-Sent Events) state
    pub sse_data: Option<Box<SseConnectionData>>,

    /// Streaming response state
    /// Queue of chunks to write (for streaming responses)
    pub stream_chunks: VecDeque<Bytes>,
    /// Position within current chunk being written
    pub stream_chunk_pos: usize,
    /// Whether the final chunk (0\r\n\r\n) has been queued
    pub stream_final_sent: bool,
    /// Registry key for chunk producer function (Lua)
    pub stream_producer: Option<Arc<mlua::RegistryKey>>,
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
            sse_data: None,
            stream_chunks: VecDeque::with_capacity(16),
            stream_chunk_pos: 0,
            stream_final_sent: false,
            stream_producer: None,
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
                    let Some(content_length) = extract_content_length(req.headers) else {
                        self.state = ConnectionState::Closed;
                        return Ok(false);
                    };

                    self.headers_complete = true;
                    self.content_length = content_length;

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

                        if name_str.eq_ignore_ascii_case("connection") {
                            self.keep_alive =
                                !header_value_has_token(value_str.as_bytes(), "close");
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
        } else if let Some(pos) = self.find_header_end() {
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
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
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
        buf: Vec<u8>,
    ) {
        self.set_response_bytes_with_headers(status, body, content_type, None, buf)
    }

    pub fn set_response_bytes_with_headers(
        &mut self,
        status: u16,
        body: Bytes,
        content_type: Option<&str>,
        custom_headers: Option<&std::collections::HashMap<String, String>>,
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
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
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

        // Build headers
        write!(
            self.write_buf,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}",
            status,
            status_text,
            ct,
            body.len(),
            conn
        )
        .unwrap();

        if let Some(headers) = custom_headers {
            for (name, value) in headers {
                write!(self.write_buf, "\r\n{}: {}", name, value).unwrap();
            }
        }

        write!(self.write_buf, "\r\n\r\n").unwrap();

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
        self.sse_data = None;
        self.stream_chunks.clear();
        self.stream_chunk_pos = 0;
        self.stream_final_sent = false;
        self.stream_producer = None;
    }

    /// Prepare this connection for server shutdown.
    ///
    /// Returns true when the connection can be closed immediately.
    pub fn prepare_for_shutdown(&mut self) -> bool {
        self.keep_alive = false;

        match self.state {
            ConnectionState::Reading | ConnectionState::Closed => {
                self.state = ConnectionState::Closed;
                true
            }
            ConnectionState::Writing => false,
            ConnectionState::StreamingHeaders | ConnectionState::StreamingBody => {
                self.sse_data = None;
                self.stream_producer = None;
                self.queue_stream_end();
                false
            }
            ConnectionState::SseActive => {
                self.sse_data = None;
                self.state = ConnectionState::Closed;
                true
            }
            ConnectionState::WsActive | ConnectionState::WsClosed => false,
        }
    }

    // ── Streaming methods ──

    /// Set up a streaming response with chunked transfer encoding headers.
    /// After this, call queue_stream_chunk() to add body chunks.
    pub fn set_streaming_headers(
        &mut self,
        status: u16,
        content_type: &str,
        custom_headers: Option<&std::collections::HashMap<String, String>>,
        mut buf: Vec<u8>,
    ) {
        buf.clear();
        self.write_buf = buf;
        self.write_pos = 0;
        self.stream_chunks.clear();
        self.stream_chunk_pos = 0;
        self.stream_final_sent = false;

        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        let conn = if self.keep_alive {
            "keep-alive"
        } else {
            "close"
        };

        // Build headers with Transfer-Encoding: chunked (no Content-Length)
        write!(
            self.write_buf,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nTransfer-Encoding: chunked\r\nConnection: {}",
            status, status_text, content_type, conn
        )
        .unwrap();

        if let Some(headers) = custom_headers {
            for (name, value) in headers {
                write!(self.write_buf, "\r\n{}: {}", name, value).unwrap();
            }
        }

        write!(self.write_buf, "\r\n\r\n").unwrap();
        self.state = ConnectionState::StreamingHeaders;
    }

    /// Queue a chunk for streaming. Chunks are encoded with chunked transfer encoding.
    pub fn queue_stream_chunk(&mut self, chunk: Bytes) {
        if chunk.is_empty() {
            return;
        }
        // Format: <hex-size>\r\n<data>\r\n
        let size = chunk.len();
        let hex_size = format!("{:x}", size);
        let mut encoded = Vec::with_capacity(hex_size.len() + 2 + size + 2);
        encoded.extend_from_slice(hex_size.as_bytes());
        encoded.extend_from_slice(b"\r\n");
        encoded.extend_from_slice(&chunk);
        encoded.extend_from_slice(b"\r\n");
        self.stream_chunks.push_back(Bytes::from(encoded));
    }

    /// Queue the final chunk marker (0\r\n\r\n)
    pub fn queue_stream_end(&mut self) {
        if !self.stream_final_sent {
            self.stream_chunks
                .push_back(Bytes::from_static(b"0\r\n\r\n"));
            self.stream_final_sent = true;
        }
    }

    /// Write streaming data: first headers, then chunks.
    /// Returns Ok(true) when fully complete, Ok(false) on WouldBlock.
    pub fn try_write_stream(&mut self) -> std::io::Result<bool> {
        // Phase 1: Write headers
        if matches!(self.state, ConnectionState::StreamingHeaders) {
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
            // Headers fully written, move to body phase
            self.state = ConnectionState::StreamingBody;
            self.write_pos = 0;
        }

        // Phase 2: Write chunks
        if matches!(self.state, ConnectionState::StreamingBody) {
            while let Some(front) = self.stream_chunks.front() {
                let remaining = &front[self.stream_chunk_pos..];
                if remaining.is_empty() {
                    self.stream_chunks.pop_front();
                    self.stream_chunk_pos = 0;
                    continue;
                }

                match self.socket.write(remaining) {
                    Ok(0) => {
                        self.state = ConnectionState::Closed;
                        return Ok(false);
                    }
                    Ok(n) => {
                        self.stream_chunk_pos += n;
                        if self.stream_chunk_pos >= front.len() {
                            self.stream_chunks.pop_front();
                            self.stream_chunk_pos = 0;
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        return Ok(false);
                    }
                    Err(e) => return Err(e),
                }
            }

            // All chunks written, check if final marker sent
            if self.stream_final_sent && self.stream_chunks.is_empty() {
                if self.keep_alive {
                    self.state = ConnectionState::Reading;
                } else {
                    self.state = ConnectionState::Closed;
                }
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if there are pending chunks to write
    #[inline]
    pub fn has_pending_chunks(&self) -> bool {
        !self.stream_chunks.is_empty() || !self.stream_final_sent
    }

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

#[cfg(test)]
mod tests {
    use super::{
        Connection, ConnectionState, SseConnectionData, extract_content_length,
        header_value_has_token, parse_content_length,
    };
    use std::io::Read;

    fn new_test_connection() -> Connection {
        use mio::Token;
        use mio::net::TcpStream;
        use std::net::{TcpListener, TcpStream as StdTcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");
        let client = StdTcpStream::connect(addr).expect("connect client");
        let (_server, _) = listener.accept().expect("accept client");
        client.set_nonblocking(true).expect("set nonblocking");

        Connection::new(TcpStream::from_std(client), Token(1))
    }

    fn new_test_connection_pair() -> (Connection, std::net::TcpStream) {
        use mio::Token;
        use mio::net::TcpStream;
        use std::net::{TcpListener, TcpStream as StdTcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("listener addr");
        let client = StdTcpStream::connect(addr).expect("connect client");
        let (server, _) = listener.accept().expect("accept client");
        client.set_nonblocking(true).expect("set nonblocking");

        (
            Connection::new(TcpStream::from_std(client), Token(1)),
            server,
        )
    }

    #[test]
    fn should_parse_content_length_with_ows() {
        assert_eq!(parse_content_length(b" \t123\t "), Some(123));
    }

    #[test]
    fn should_reject_invalid_content_length() {
        assert_eq!(parse_content_length(b"12a"), None);
        assert_eq!(parse_content_length(b""), None);
        assert_eq!(parse_content_length(b"1,2"), None);
    }

    #[test]
    fn should_accept_identical_duplicate_content_length_headers() {
        let headers = [
            httparse::Header {
                name: "Content-Length",
                value: b"10",
            },
            httparse::Header {
                name: "content-length",
                value: b"10",
            },
        ];

        assert_eq!(extract_content_length(&headers), Some(10));
    }

    #[test]
    fn should_reject_mismatched_duplicate_content_length_headers() {
        let headers = [
            httparse::Header {
                name: "Content-Length",
                value: b"10",
            },
            httparse::Header {
                name: "Content-Length",
                value: b"11",
            },
        ];

        assert_eq!(extract_content_length(&headers), None);
    }

    #[test]
    fn should_reject_transfer_encoding_requests() {
        let headers = [httparse::Header {
            name: "Transfer-Encoding",
            value: b"chunked",
        }];

        assert_eq!(extract_content_length(&headers), None);
    }

    #[test]
    fn should_match_connection_close_token() {
        assert!(header_value_has_token(b"keep-alive, close", "close"));
        assert!(!header_value_has_token(b"keep-alive", "close"));
    }

    #[test]
    fn should_queue_stream_chunk() {
        use bytes::Bytes;

        let mut conn = new_test_connection();
        conn.queue_stream_chunk(Bytes::from_static(b"Hello"));

        assert_eq!(conn.stream_chunks.len(), 1);
        assert_eq!(
            conn.stream_chunks.front().unwrap().as_ref(),
            b"5\r\nHello\r\n"
        );
    }

    #[test]
    fn should_format_stream_chunk_with_hex_size() {
        use bytes::Bytes;

        let mut conn = new_test_connection();
        conn.queue_stream_chunk(Bytes::from(vec![b'x'; 16]));

        assert_eq!(
            conn.stream_chunks.front().unwrap().as_ref(),
            b"10\r\nxxxxxxxxxxxxxxxx\r\n"
        );
    }

    #[test]
    fn should_final_chunk_format() {
        let mut conn = new_test_connection();

        conn.queue_stream_end();
        conn.queue_stream_end();

        assert!(conn.stream_final_sent);
        assert_eq!(conn.stream_chunks.len(), 1);
        assert_eq!(conn.stream_chunks.front().unwrap().as_ref(), b"0\r\n\r\n");
    }

    #[test]
    fn should_streaming_state_transitions() {
        use std::collections::HashMap;

        let mut conn = new_test_connection();

        conn.set_streaming_headers(200, "text/plain", Some(&HashMap::new()), Vec::new());

        assert_eq!(conn.state, ConnectionState::StreamingHeaders);
        assert_ne!(
            ConnectionState::StreamingHeaders,
            ConnectionState::StreamingBody
        );
        assert_ne!(ConnectionState::StreamingHeaders, ConnectionState::Writing);
        assert_ne!(ConnectionState::StreamingBody, ConnectionState::Writing);
        assert!(ConnectionState::StreamingHeaders as u8 > ConnectionState::Reading as u8);
        assert!(ConnectionState::StreamingBody as u8 > ConnectionState::StreamingHeaders as u8);
    }

    #[test]
    fn should_write_stream_and_return_to_reading() {
        use bytes::Bytes;
        use std::collections::HashMap;
        use std::io::ErrorKind;

        let (mut conn, mut server) = new_test_connection_pair();
        server
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("set read timeout");

        conn.set_streaming_headers(200, "text/plain", Some(&HashMap::new()), Vec::new());
        conn.queue_stream_chunk(Bytes::from_static(b"hello"));
        conn.queue_stream_end();

        assert!(conn.try_write_stream().expect("write stream"));
        assert_eq!(conn.state, ConnectionState::Reading);
        assert!(!conn.has_pending_chunks());

        let mut written = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            match server.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => written.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => panic!("read stream bytes: {err}"),
            }
        }

        let expected = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/plain\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: keep-alive\r\n\r\n",
            "5\r\nhello\r\n",
            "0\r\n\r\n"
        );
        assert_eq!(written, expected.as_bytes());
    }

    #[test]
    fn should_clear_sse_state_on_reset() {
        use mlua::{Lua, Value};
        use std::sync::Arc;

        let lua = Lua::new();
        let producer = lua
            .create_function(|_lua, ()| Ok(Value::Nil))
            .expect("create SSE producer");
        let producer_key = lua
            .create_registry_value(producer)
            .expect("store producer in registry");

        let mut conn = new_test_connection();
        conn.sse_data = Some(Box::new(SseConnectionData {
            event_producer: Arc::new(producer_key),
            retry_ms: 1000,
            retry_pending: true,
            keepalive_ms: 0,
            last_write: None,
        }));

        conn.reset();

        assert!(conn.sse_data.is_none());
    }

    #[test]
    fn should_close_idle_connection_on_shutdown_prepare() {
        let mut conn = new_test_connection();
        assert!(conn.prepare_for_shutdown());
        assert_eq!(conn.state, ConnectionState::Closed);
        assert!(!conn.keep_alive);
    }

    #[test]
    fn should_finish_stream_and_drop_producers_on_shutdown_prepare() {
        use bytes::Bytes;
        use mlua::{Lua, Value};
        use std::collections::HashMap;
        use std::sync::Arc;

        let lua = Lua::new();
        let producer = lua
            .create_function(|_lua, ()| Ok(Value::Nil))
            .expect("create SSE producer");
        let producer_key = lua
            .create_registry_value(producer)
            .expect("store producer in registry");

        let mut conn = new_test_connection();
        conn.set_streaming_headers(200, "text/event-stream", Some(&HashMap::new()), Vec::new());
        conn.state = ConnectionState::StreamingBody;
        conn.queue_stream_chunk(Bytes::from_static(b"hello"));
        conn.sse_data = Some(Box::new(SseConnectionData {
            event_producer: Arc::new(producer_key),
            retry_ms: 1000,
            retry_pending: true,
            keepalive_ms: 0,
            last_write: None,
        }));

        assert!(!conn.prepare_for_shutdown());
        assert_eq!(conn.state, ConnectionState::StreamingBody);
        assert!(conn.stream_final_sent);
        assert!(conn.sse_data.is_none());
        assert!(conn.stream_producer.is_none());
        assert!(!conn.keep_alive);
    }
}
