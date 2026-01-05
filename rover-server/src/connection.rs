use std::io::{Read, Write};
use mio::net::TcpStream;
use mio::{Interest, Token};

const READ_BUF_SIZE: usize = 4096;
const MAX_HEADERS: usize = 32;

#[derive(Debug)]
pub enum ConnectionState {
    Reading,
    Writing,
    Closed,
}

pub struct Connection {
    pub socket: TcpStream,
    pub token: Token,
    pub state: ConnectionState,
    
    // Read buffer
    pub read_buf: Vec<u8>,
    pub read_pos: usize,
    
    // Write buffer  
    pub write_buf: Vec<u8>,
    pub write_pos: usize,
    
    // Parsed request data (valid after complete read)
    pub method: Option<String>,
    pub path: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub content_length: usize,
    pub headers_complete: bool,
    pub keep_alive: bool,
}

impl Connection {
    pub fn new(socket: TcpStream, token: Token) -> Self {
        Self {
            socket,
            token,
            state: ConnectionState::Reading,
            read_buf: Vec::with_capacity(READ_BUF_SIZE),
            read_pos: 0,
            write_buf: Vec::new(),
            write_pos: 0,
            method: None,
            path: None,
            headers: Vec::with_capacity(8),
            body: None,
            content_length: 0,
            headers_complete: false,
            keep_alive: true,
        }
    }

    /// Try to read data from socket. Returns true if request is complete.
    pub fn try_read(&mut self) -> std::io::Result<bool> {
        // Ensure buffer has space
        if self.read_buf.len() < self.read_pos + 1024 {
            self.read_buf.resize(self.read_pos + READ_BUF_SIZE, 0);
        }

        loop {
            match self.socket.read(&mut self.read_buf[self.read_pos..]) {
                Ok(0) => {
                    // Connection closed
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
                Ok(n) => {
                    self.read_pos += n;
                    
                    // Try to parse
                    if self.try_parse()? {
                        return Ok(true);
                    }
                    // Continue reading if not complete
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No more data available
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Try to parse the buffered data. Returns true if request is complete.
    fn try_parse(&mut self) -> std::io::Result<bool> {
        if !self.headers_complete {
            let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
            let mut req = httparse::Request::new(&mut headers);
            
            match req.parse(&self.read_buf[..self.read_pos]) {
                Ok(httparse::Status::Complete(header_len)) => {
                    self.headers_complete = true;
                    self.method = req.method.map(|s| s.to_string());
                    self.path = req.path.map(|s| s.to_string());
                    
                    // Extract headers
                    self.headers.clear();
                    for header in req.headers.iter() {
                        let name = header.name.to_string();
                        let value = String::from_utf8_lossy(header.value).to_string();
                        
                        // Check for Content-Length
                        if name.eq_ignore_ascii_case("content-length") {
                            self.content_length = value.parse().unwrap_or(0);
                        }
                        // Check for Connection header
                        if name.eq_ignore_ascii_case("connection") {
                            self.keep_alive = !value.eq_ignore_ascii_case("close");
                        }
                        
                        self.headers.push((name, value));
                    }
                    
                    // Check if we have the full body
                    let body_start = header_len;
                    let body_received = self.read_pos - body_start;
                    
                    if body_received >= self.content_length {
                        // Request complete
                        if self.content_length > 0 {
                            self.body = Some(self.read_buf[body_start..body_start + self.content_length].to_vec());
                        }
                        return Ok(true);
                    }
                }
                Ok(httparse::Status::Partial) => {
                    // Need more data
                    return Ok(false);
                }
                Err(_) => {
                    self.state = ConnectionState::Closed;
                    return Ok(false);
                }
            }
        } else {
            // Headers already parsed, waiting for body
            // Find where headers ended
            if let Some(pos) = self.find_header_end() {
                let body_received = self.read_pos - pos;
                if body_received >= self.content_length {
                    if self.content_length > 0 {
                        self.body = Some(self.read_buf[pos..pos + self.content_length].to_vec());
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

    /// Try to write response. Returns true if write is complete.
    pub fn try_write(&mut self) -> std::io::Result<bool> {
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
        
        // Write complete
        Ok(true)
    }

    /// Set response to write
    pub fn set_response(&mut self, status: u16, body: &[u8], content_type: Option<&str>) {
        self.write_buf.clear();
        self.write_pos = 0;
        
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
        
        // Build response
        use std::fmt::Write;
        let mut header = String::with_capacity(256);
        write!(
            &mut header,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status, status_text, ct, body.len(), conn
        ).unwrap();
        
        self.write_buf.extend_from_slice(header.as_bytes());
        self.write_buf.extend_from_slice(body);
        
        self.state = ConnectionState::Writing;
    }

    /// Reset for next request (keep-alive)
    pub fn reset(&mut self) {
        self.read_buf.clear();
        self.read_pos = 0;
        self.write_buf.clear();
        self.write_pos = 0;
        self.method = None;
        self.path = None;
        self.headers.clear();
        self.body = None;
        self.content_length = 0;
        self.headers_complete = false;
        self.state = ConnectionState::Reading;
    }

    pub fn interest(&self) -> Interest {
        match self.state {
            ConnectionState::Reading => Interest::READABLE,
            ConnectionState::Writing => Interest::WRITABLE,
            ConnectionState::Closed => Interest::READABLE, // Will be removed
        }
    }
}
