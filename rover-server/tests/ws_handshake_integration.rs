use rover_server::ws_handshake::{compute_accept_key, validate_upgrade_headers};

#[test]
fn should_validate_upgrade_headers_with_mixed_case_connection_tokens() {
    let raw = b"GET /ws HTTP/1.1\r\nUPGRADE: websocket\r\nConnection: keep-alive, UpGrAdE\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";

    let headers = vec![
        (18, 7, 27, 9),
        (38, 10, 50, 19),
        (71, 17, 90, 24),
        (116, 21, 139, 2),
    ];

    let key = validate_upgrade_headers(raw, &headers).expect("valid ws upgrade headers");
    assert_eq!(key, "dGhlIHNhbXBsZSBub25jZQ==");
}

#[test]
fn should_keep_accept_hash_stable() {
    let key = compute_accept_key("dGhlIHNhbXBsZSBub25jZQ==");
    assert_eq!(key, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}
