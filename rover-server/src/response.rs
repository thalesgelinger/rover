use crate::Bytes;
use mlua::UserData;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static SSE_EVENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn generate_sse_event_id() -> String {
    let id = SSE_EVENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}", id)
}

/// Pre-serialized HTTP response - zero-cost abstraction for Lua
#[derive(Clone)]
pub struct RoverResponse {
    pub status: u16,
    pub body: Bytes,
    pub content_type: &'static str,
    pub headers: Option<HashMap<String, String>>,
}

impl RoverResponse {
    pub fn json(status: u16, body: Bytes, headers: Option<HashMap<String, String>>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
            headers,
        }
    }

    pub fn json_bytes(status: u16, body: Bytes, headers: Option<HashMap<String, String>>) -> Self {
        Self {
            status,
            body,
            content_type: "application/json",
            headers,
        }
    }

    pub fn text(status: u16, body: Bytes, headers: Option<HashMap<String, String>>) -> Self {
        Self {
            status,
            body,
            content_type: "text/plain",
            headers,
        }
    }

    pub fn html(status: u16, body: Bytes, headers: Option<HashMap<String, String>>) -> Self {
        Self {
            status,
            body,
            content_type: "text/html",
            headers,
        }
    }

    pub fn redirect(status: u16, location: String) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Location".to_string(), location);

        Self {
            status,
            body: Bytes::new(),
            content_type: "text/plain",
            headers: Some(headers),
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: Bytes::new(),
            content_type: "text/plain",
            headers: None,
        }
    }

    pub fn raw(status: u16, body: Bytes, headers: Option<HashMap<String, String>>) -> Self {
        Self {
            status,
            body,
            content_type: "application/octet-stream",
            headers,
        }
    }
}

impl UserData for RoverResponse {}

/// A chunk producer for streaming responses
/// This is stored per-connection and called repeatedly to get chunks
pub struct ChunkProducer {
    /// The Lua function that produces chunks
    /// Returns: string (chunk data) or nil (end of stream)
    producer: mlua::RegistryKey,
}

impl ChunkProducer {
    pub fn new(producer: mlua::RegistryKey) -> Self {
        Self { producer }
    }

    pub fn producer(&self) -> &mlua::RegistryKey {
        &self.producer
    }
}

/// Streaming response with chunked transfer encoding
#[derive(Clone)]
pub struct StreamingResponse {
    pub status: u16,
    pub content_type: String,
    pub headers: Option<HashMap<String, String>>,
    /// The chunk producer stored in Lua registry
    pub chunk_producer: Arc<mlua::RegistryKey>,
}

impl StreamingResponse {
    pub fn new(
        status: u16,
        content_type: String,
        headers: Option<HashMap<String, String>>,
        chunk_producer: Arc<mlua::RegistryKey>,
    ) -> Self {
        Self {
            status,
            content_type,
            headers,
            chunk_producer,
        }
    }
}

impl UserData for StreamingResponse {}

/// SSE (Server-Sent Events) response.
/// The producer returns tables/strings for events, or nil to end the stream.
/// SSE format: "event: name\ndata: payload\nid: id\n\n"
#[derive(Clone)]
pub struct SseResponse {
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
    /// The event producer function stored in Lua registry
    /// Returns: table with event/data/id fields, or string (data-only), or nil (end)
    pub event_producer: Arc<mlua::RegistryKey>,
    /// Initial reconnect hint in milliseconds
    pub retry_ms: Option<u32>,
}

impl SseResponse {
    pub fn new(
        status: u16,
        headers: Option<HashMap<String, String>>,
        event_producer: Arc<mlua::RegistryKey>,
        retry_ms: Option<u32>,
    ) -> Self {
        Self {
            status,
            headers,
            event_producer,
            retry_ms,
        }
    }
}

impl UserData for SseResponse {}

/// SSE event formatting utilities
pub struct SseWriter;

impl SseWriter {
    pub fn format_event<W: std::io::Write>(
        writer: &mut W,
        event: Option<&str>,
        data: &str,
        id: Option<&str>,
    ) {
        if let Some(id) = id {
            writer.write_all(b"id:").unwrap();
            writer.write_all(id.as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
        }
        if let Some(event_name) = event {
            writer.write_all(b"event:").unwrap();
            writer.write_all(event_name.as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
        }
        for line in data.split('\n') {
            writer.write_all(b"data:").unwrap();
            writer.write_all(line.as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
        }
        writer.write_all(b"\n").unwrap();
    }

    pub fn format_retry<W: std::io::Write>(writer: &mut W, retry_ms: u32) {
        writer.write_all(b"retry:").unwrap();
        writer.write_all(retry_ms.to_string().as_bytes()).unwrap();
        writer.write_all(b"\n\n").unwrap();
    }

    pub fn format_comment<W: std::io::Write>(writer: &mut W, comment: &str) {
        writer.write_all(b":").unwrap();
        writer.write_all(comment.as_bytes()).unwrap();
        writer.write_all(b"\n\n").unwrap();
    }
}

/// Response enum that can be either immediate or streaming
#[derive(Clone)]
pub enum HttpResponse {
    Immediate(RoverResponse),
    Streaming(StreamingResponse),
}

/// Write a chunk in HTTP chunked encoding format
/// Format: <size-hex>\r\n<data>\r\n
pub fn write_chunk_header(size: usize) -> Vec<u8> {
    let mut header = Vec::with_capacity(16);
    let hex = format!("{:x}", size);
    header.extend_from_slice(hex.as_bytes());
    header.extend_from_slice(b"\r\n");
    header
}

/// Write the final chunk (zero-length) to end chunked encoding
pub fn write_final_chunk() -> Vec<u8> {
    b"0\r\n\r\n".to_vec()
}

thread_local! {
    /// Thread-local storage for pending chunks to be written
    /// Used to communicate between Lua producer and connection writer
    static PENDING_CHUNKS: RefCell<Vec<Bytes>> = const { RefCell::new(Vec::new()) };
}

/// Push a chunk to the thread-local pending queue
pub fn push_chunk(chunk: Bytes) {
    PENDING_CHUNKS.with(|pc| {
        pc.borrow_mut().push(chunk);
    });
}

/// Take all pending chunks from the thread-local queue
pub fn take_pending_chunks() -> Vec<Bytes> {
    PENDING_CHUNKS.with(|pc| {
        let mut chunks = pc.borrow_mut();
        std::mem::take(&mut *chunks)
    })
}

/// Clear pending chunks
pub fn clear_pending_chunks() {
    PENDING_CHUNKS.with(|pc| {
        pc.borrow_mut().clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_chunk_header() {
        let header = write_chunk_header(10);
        assert_eq!(&header, b"a\r\n");

        let header = write_chunk_header(256);
        assert_eq!(&header, b"100\r\n");

        let header = write_chunk_header(4096);
        assert_eq!(&header, b"1000\r\n");
    }

    #[test]
    fn test_write_final_chunk() {
        let final_chunk = write_final_chunk();
        assert_eq!(&final_chunk, b"0\r\n\r\n");
    }

    #[test]
    fn test_rover_response_json() {
        let response = RoverResponse::json(200, Bytes::from_static(b"{\"ok\":true}"), None);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "application/json");
        assert_eq!(&response.body[..], b"{\"ok\":true}");
    }

    #[test]
    fn test_rover_response_text() {
        let response = RoverResponse::text(200, Bytes::from_static(b"Hello"), None);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "text/plain");
        assert_eq!(&response.body[..], b"Hello");
    }

    #[test]
    fn test_rover_response_html() {
        let response = RoverResponse::html(200, Bytes::from_static(b"<html></html>"), None);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "text/html");
    }

    #[test]
    fn test_rover_response_redirect() {
        let response = RoverResponse::redirect(302, "https://example.com".to_string());
        assert_eq!(response.status, 302);
        assert!(response.headers.is_some());
        assert_eq!(
            response.headers.as_ref().unwrap().get("Location"),
            Some(&"https://example.com".to_string())
        );
    }

    #[test]
    fn test_rover_response_empty() {
        let response = RoverResponse::empty(204);
        assert_eq!(response.status, 204);
        assert!(response.body.is_empty());
    }

    #[test]
    fn test_rover_response_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        let response = RoverResponse::json(200, Bytes::new(), Some(headers));
        assert_eq!(response.status, 200);
        assert!(response.headers.is_some());
        assert_eq!(
            response.headers.as_ref().unwrap().get("X-Custom"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_pending_chunks() {
        clear_pending_chunks();

        push_chunk(Bytes::from_static(b"chunk1"));
        push_chunk(Bytes::from_static(b"chunk2"));

        let chunks = take_pending_chunks();
        assert_eq!(chunks.len(), 2);
        assert_eq!(&chunks[0][..], b"chunk1");
        assert_eq!(&chunks[1][..], b"chunk2");

        // After taking, should be empty
        let chunks2 = take_pending_chunks();
        assert!(chunks2.is_empty());

        clear_pending_chunks();
    }

    #[test]
    fn should_format_sse_event_with_multiline_data() {
        let mut buf = Vec::new();
        SseWriter::format_event(&mut buf, Some("token"), "hello\nworld", Some("evt-1"));

        assert_eq!(buf, b"id:evt-1\nevent:token\ndata:hello\ndata:world\n\n");
    }

    #[test]
    fn should_format_sse_retry_hint() {
        let mut buf = Vec::new();
        SseWriter::format_retry(&mut buf, 5000);

        assert_eq!(buf, b"retry:5000\n\n");
    }
}
