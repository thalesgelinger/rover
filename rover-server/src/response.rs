use crate::Bytes;
use mlua::UserData;
use std::collections::HashMap;

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
