/// WebSocket handshake (RFC 6455 sec 4.2).
///
/// Validates HTTP Upgrade headers using the connection's offset-based header access,
/// computes the Sec-WebSocket-Accept key, and builds the 101 Switching Protocols response.

use sha1::{Digest, Sha1};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// RFC 6455 magic GUID for Sec-WebSocket-Accept computation.
const WS_MAGIC_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-5AB5C0F84F11";

#[derive(Debug)]
pub enum HandshakeError {
    MissingUpgradeHeader,
    MissingConnectionHeader,
    MissingKey,
    UnsupportedVersion,
}

impl HandshakeError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::MissingUpgradeHeader | Self::MissingConnectionHeader | Self::MissingKey => 400,
            Self::UnsupportedVersion => 426,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingUpgradeHeader => "Missing or invalid Upgrade header",
            Self::MissingConnectionHeader => "Missing or invalid Connection header",
            Self::MissingKey => "Missing Sec-WebSocket-Key header",
            Self::UnsupportedVersion => "Unsupported WebSocket version (requires 13)",
        }
    }
}

/// Validate the HTTP upgrade request headers from a parsed HTTP request.
///
/// `buf` is the raw HTTP buffer, `header_offsets` are (name_off, name_len, val_off, val_len).
/// Returns the Sec-WebSocket-Key value on success.
pub fn validate_upgrade_headers(
    buf: &[u8],
    header_offsets: &[(usize, usize, usize, usize)],
) -> Result<&str, HandshakeError> {
    let mut has_upgrade = false;
    let mut has_connection = false;
    let mut ws_key: Option<&str> = None;
    let mut version_ok = false;

    for &(name_off, name_len, val_off, val_len) in header_offsets {
        let name = unsafe { std::str::from_utf8_unchecked(&buf[name_off..name_off + name_len]) };
        let val = unsafe { std::str::from_utf8_unchecked(&buf[val_off..val_off + val_len]) };

        if name.eq_ignore_ascii_case("upgrade") {
            if val.eq_ignore_ascii_case("websocket") {
                has_upgrade = true;
            }
        } else if name.eq_ignore_ascii_case("connection") {
            // Connection header may contain multiple values: "keep-alive, Upgrade"
            for part in val.split(',') {
                if part.trim().eq_ignore_ascii_case("upgrade") {
                    has_connection = true;
                    break;
                }
            }
        } else if name.eq_ignore_ascii_case("sec-websocket-key") {
            ws_key = Some(val.trim());
        } else if name.eq_ignore_ascii_case("sec-websocket-version") {
            if val.trim() == "13" {
                version_ok = true;
            }
        }
    }

    if !has_upgrade {
        return Err(HandshakeError::MissingUpgradeHeader);
    }
    if !has_connection {
        return Err(HandshakeError::MissingConnectionHeader);
    }

    let key = ws_key.ok_or(HandshakeError::MissingKey)?;

    if !version_ok {
        return Err(HandshakeError::UnsupportedVersion);
    }

    Ok(key)
}

/// Compute the Sec-WebSocket-Accept value from the client's key.
/// SHA-1(key + magic GUID) -> Base64.
#[inline]
pub fn compute_accept_key(client_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(client_key.as_bytes());
    hasher.update(WS_MAGIC_GUID);
    let digest = hasher.finalize();
    BASE64.encode(digest)
}

/// Write the 101 Switching Protocols response into `buf`.
/// Response is ~130 bytes, fits comfortably in a pooled 512-byte buffer.
#[inline]
pub fn build_upgrade_response(accept_key: &str, buf: &mut Vec<u8>) {
    use std::fmt::Write;
    let _ = write!(
        buf,
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_key
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_accept_key() {
        // RFC 6455 sec 4.2.2 example
        let key = compute_accept_key("dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(key, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn test_build_upgrade_response() {
        let mut buf = Vec::new();
        build_upgrade_response("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", &mut buf);
        let response = std::str::from_utf8(&buf).unwrap();
        assert!(response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(response.contains("Upgrade: websocket\r\n"));
        assert!(response.contains("Connection: Upgrade\r\n"));
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
        assert!(response.ends_with("\r\n\r\n"));
    }

    #[test]
    fn test_validate_upgrade_headers() {
        // Simulate a raw HTTP buffer with headers
        let raw = b"GET /chat HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
        // Manually compute header offsets (name_off, name_len, val_off, val_len)
        let headers = vec![
            (20, 7, 29, 9),   // Upgrade: websocket
            (40, 10, 52, 7),  // Connection: Upgrade
            (61, 17, 80, 24), // Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
            (106, 21, 129, 2), // Sec-WebSocket-Version: 13
        ];
        let result = validate_upgrade_headers(raw, &headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "dGhlIHNhbXBsZSBub25jZQ==");
    }
}
