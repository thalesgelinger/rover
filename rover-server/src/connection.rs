use std::io::{Read, Write, IoSlice};
use mio::net::TcpStream;
use mio::Token;
use mlua::Thread;
use crate::Bytes;

const READ_BUF_SIZE: usize = 4096;
const MAX_HEADERS: usize = 32;

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    Reading,
    Writing,
    Closed,
}

pub struct Connection {
    pub socket: TcpStream,
    pub token: Token,
    pub state: ConnectionState,
    
    pub read_buf: Vec<u8>,
    pub read_pos: usize,
    
    // Vectored I/O: separate header and body buffers
    pub write_buf: Vec<u8>,  // Headers only
    pub write_pos: usize,    // Position in headers
    pub body_buf: Bytes,     // Body (zero-copy from response)
    pub body_pos: usize,     // Position in body
    
    pub method: Option<String>,
    pub path: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Option<(usize, usize)>,
    pub content_length: usize,
    pub headers_complete: bool,
    pub keep_alive: bool,
    
    pub thread: Option<Thread>,
}

impl Connection {
    pub fn new(socket: TcpStream, token: Token) -> Self {
        Self {
            socket,
            token,
            state: ConnectionState::Reading,
            read_buf: Vec::with_capacity(READ_BUF_SIZE * 2),
            read_pos: 0,
            write_buf: Vec::with_capacity(512),  // Headers are typically small
            write_pos: 0,
            body_buf: Bytes::new(),
            body_pos: 0,
            method: None,
            path: None,
            headers: Vec::with_capacity(16),
            body: None,
            content_length: 0,
            headers_complete: false,
            keep_alive: true,
            thread: None,
        }
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
            
            match req.parse(&self.read_buf[..self.read_pos]) {
                Ok(httparse::Status::Complete(header_len)) => {
                    self.headers_complete = true;
                    self.method = req.method.map(|s| s.to_string());
                    self.path = req.path.map(|s| s.to_string());
                    
                    self.headers.clear();
                    for header in req.headers.iter() {
                        let name = header.name.to_string();
                        let value = String::from_utf8_lossy(header.value).to_string();
                        
                        if name.eq_ignore_ascii_case("content-length") {
                            self.content_length = value.parse().unwrap_or(0);
                        }
                        if name.eq_ignore_ascii_case("connection") {
                            self.keep_alive = !value.eq_ignore_ascii_case("close");
                        }
                        
                        self.headers.push((name, value));
                    }
                    
                    let body_start = header_len;
                    let body_received = self.read_pos - body_start;
                    
                    if body_received >= self.content_length {
                        if self.content_length > 0 {
                            self.body = Some((body_start, self.content_length));
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
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }

    fn find_header_end(&self) -> Option<usize> {
        for i in 0..self.read_pos.saturating_sub(3) {
            if &self.read_buf[i..i+4] == b"\r\n\r\n" {
                return Some(i + 4);
            }
        }
        None
    }

    pub fn get_body(&self) -> Option<&[u8]> {
        if let Some((start, len)) = self.body {
            Some(&self.read_buf[start..start + len])
        } else {
            None
        }
    }

    pub fn get_body_bytes(&self) -> Option<Bytes> {
        if let Some((start, len)) = self.body {
            Some(Bytes::copy_from_slice(&self.read_buf[start..start + len]))
        } else {
            None
        }
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
                    if self.write_pos >= self.write_buf.len() && self.body_pos >= self.body_buf.len() {
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

    pub fn set_response(&mut self, status: u16, body: &[u8], content_type: Option<&str>) {
        self.write_buf.clear();
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
        let conn = if self.keep_alive { "keep-alive" } else { "close" };

        // Build headers only (body stored separately for vectored I/O)
        write!(self.write_buf, "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status, status_text, ct, body.len(), conn).unwrap();

        // Store body separately (zero-copy via Bytes)
        self.body_buf = Bytes::copy_from_slice(body);
        self.state = ConnectionState::Writing;
    }

    /// Set response with pre-allocated Bytes body (true zero-copy)
    pub fn set_response_bytes(&mut self, status: u16, body: Bytes, content_type: Option<&str>) {
        self.write_buf.clear();
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
        let conn = if self.keep_alive { "keep-alive" } else { "close" };

        // Build headers only
        write!(self.write_buf, "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status, status_text, ct, body.len(), conn).unwrap();

        // Store body directly (true zero-copy - no slice copy)
        self.body_buf = body;
        self.state = ConnectionState::Writing;
    }

    pub fn reset(&mut self) {
        // Keep capacity to avoid reallocations (zero-copy optimization)
        self.read_buf.clear();
        self.read_pos = 0;
        self.write_buf.clear();
        self.write_pos = 0;
        self.body_buf = Bytes::new();
        self.body_pos = 0;
        self.method = None;
        self.path = None;
        self.headers.clear();
        self.body = None;
        self.content_length = 0;
        self.headers_complete = false;
        self.state = ConnectionState::Reading;
        self.thread = None;
    }
}
