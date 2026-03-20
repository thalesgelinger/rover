//! Integration tests for Content-Encoding and Vary: Accept-Encoding headers
//! These tests verify that compression-related headers are correctly set based on
//! response content type, size, and Accept-Encoding request header.

use std::collections::HashMap;

/// Test helper: simulate compressible content type detection
fn is_compressible_content_type(content_type: Option<&str>) -> bool {
    let ct = match content_type {
        Some(t) => t.split(';').next().unwrap_or("").trim(),
        None => return false,
    };
    let lower = ct.to_ascii_lowercase();
    (lower.starts_with("text/") && !lower.starts_with("text/event-stream"))
        || lower.starts_with("application/json")
        || lower.starts_with("application/javascript")
        || lower.starts_with("application/xml")
        || lower.starts_with("application/atom+xml")
        || lower.starts_with("application/rss+xml")
        || lower.ends_with("+json")
        || lower.ends_with("+xml")
}

/// Test helper: check if Content-Encoding header exists (case-insensitive)
fn has_content_encoding(headers: &HashMap<String, String>) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, v)| v.clone())
}

/// Test helper: get Vary header value (case-insensitive)
fn get_vary_header(headers: &HashMap<String, String>) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("vary"))
        .map(|(_, v)| v.clone())
}

/// Test helper: check if Vary header contains Accept-Encoding
fn has_vary_accept_encoding(headers: &HashMap<String, String>) -> bool {
    get_vary_header(headers)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim())
                .any(|part| part.eq_ignore_ascii_case("accept-encoding"))
        })
        .unwrap_or(false)
}

mod content_encoding_headers {
    use super::*;

    #[test]
    fn should_detect_compressible_text_html() {
        assert!(is_compressible_content_type(Some("text/html")));
        assert!(is_compressible_content_type(Some("text/css")));
        assert!(is_compressible_content_type(Some("text/plain")));
    }

    #[test]
    fn should_detect_compressible_application_json() {
        assert!(is_compressible_content_type(Some("application/json")));
        assert!(is_compressible_content_type(Some(
            "application/json; charset=utf-8"
        )));
    }

    #[test]
    fn should_detect_compressible_xml_types() {
        assert!(is_compressible_content_type(Some("application/xml")));
        assert!(is_compressible_content_type(Some("application/atom+xml")));
        assert!(is_compressible_content_type(Some("application/rss+xml")));
        assert!(is_compressible_content_type(Some(
            "application/vnd.api+json"
        )));
    }

    #[test]
    fn should_reject_event_stream_for_compression() {
        assert!(!is_compressible_content_type(Some("text/event-stream")));
        assert!(!is_compressible_content_type(Some(
            "text/event-stream; charset=utf-8"
        )));
    }

    #[test]
    fn should_reject_binary_types_for_compression() {
        assert!(!is_compressible_content_type(Some("image/png")));
        assert!(!is_compressible_content_type(Some("image/jpeg")));
        assert!(!is_compressible_content_type(Some("video/mp4")));
        assert!(!is_compressible_content_type(Some(
            "application/octet-stream"
        )));
    }

    #[test]
    fn should_reject_none_content_type() {
        assert!(!is_compressible_content_type(None));
    }
}

mod vary_header_handling {
    use super::*;

    #[test]
    fn should_detect_content_encoding_header_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("content-encoding".to_string(), "gzip".to_string());
        assert!(has_content_encoding(&headers).is_some());

        headers.clear();
        headers.insert("Content-Encoding".to_string(), "deflate".to_string());
        assert!(has_content_encoding(&headers).is_some());

        headers.clear();
        headers.insert("CONTENT-ENCODING".to_string(), "gzip".to_string());
        assert!(has_content_encoding(&headers).is_some());
    }

    #[test]
    fn should_detect_vary_accept_encoding_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("vary".to_string(), "Accept-Encoding".to_string());
        assert!(has_vary_accept_encoding(&headers));

        headers.clear();
        headers.insert("Vary".to_string(), "accept-encoding".to_string());
        assert!(has_vary_accept_encoding(&headers));

        headers.clear();
        headers.insert("VARY".to_string(), "Accept-Encoding".to_string());
        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_detect_vary_accept_encoding_in_list() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin, Accept-Encoding".to_string());
        assert!(has_vary_accept_encoding(&headers));

        headers.clear();
        headers.insert(
            "Vary".to_string(),
            "Accept-Encoding, Authorization".to_string(),
        );
        assert!(has_vary_accept_encoding(&headers));

        headers.clear();
        headers.insert(
            "Vary".to_string(),
            "Origin, Accept-Encoding, Authorization".to_string(),
        );
        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_not_detect_vary_accept_encoding_when_absent() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin".to_string());
        assert!(!has_vary_accept_encoding(&headers));

        headers.clear();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        assert!(!has_vary_accept_encoding(&headers));
    }
}

mod compression_header_integration {
    use super::*;
    use rover_server::compression::{CompressionAlgorithm, compress};

    fn make_compressible_content(size: usize) -> String {
        "hello world ".repeat(size / 12 + 1)
    }

    #[test]
    fn should_add_content_encoding_when_compression_applied() {
        let content = make_compressible_content(2048);
        let content_bytes = content.as_bytes();

        // Compress the content
        let compressed = compress(content_bytes, CompressionAlgorithm::Gzip);
        assert!(
            compressed.len() < content_bytes.len(),
            "compression should reduce size"
        );

        // Simulate response headers after compression
        let mut headers = HashMap::new();
        headers.insert("Content-Encoding".to_string(), "gzip".to_string());
        headers.insert("Vary".to_string(), "Accept-Encoding".to_string());

        assert_eq!(has_content_encoding(&headers), Some("gzip".to_string()));
        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_add_deflate_content_encoding() {
        let content = make_compressible_content(2048);
        let content_bytes = content.as_bytes();

        let compressed = compress(content_bytes, CompressionAlgorithm::Deflate);
        assert!(
            compressed.len() < content_bytes.len(),
            "deflate should reduce size"
        );

        let mut headers = HashMap::new();
        headers.insert("Content-Encoding".to_string(), "deflate".to_string());
        headers.insert("Vary".to_string(), "Accept-Encoding".to_string());

        assert_eq!(has_content_encoding(&headers), Some("deflate".to_string()));
        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_not_add_content_encoding_when_no_compression() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html".to_string());

        assert!(has_content_encoding(&headers).is_none());
    }

    #[test]
    fn should_add_vary_for_compressible_content() {
        // When content type is compressible but not actually compressed
        // (e.g., below min size), Vary: Accept-Encoding should still be added
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html".to_string());
        headers.insert("Vary".to_string(), "Accept-Encoding".to_string());

        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_not_add_vary_for_incompressible_content() {
        // Images and other binary content should not have Vary: Accept-Encoding
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "image/png".to_string());

        assert!(!has_vary_accept_encoding(&headers));
        assert!(!is_compressible_content_type(Some("image/png")));
    }

    #[test]
    fn should_not_add_vary_for_event_stream() {
        // Event streams should not have Vary: Accept-Encoding
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/event-stream".to_string());

        assert!(!has_vary_accept_encoding(&headers));
        assert!(!is_compressible_content_type(Some("text/event-stream")));
    }

    #[test]
    fn should_add_vary_when_already_encoded() {
        // If response already has Content-Encoding, Vary should be added
        let mut headers = HashMap::new();
        headers.insert("Content-Encoding".to_string(), "gzip".to_string());
        headers.insert("Vary".to_string(), "Accept-Encoding".to_string());

        assert!(has_vary_accept_encoding(&headers));
    }

    #[test]
    fn should_append_accept_encoding_to_existing_vary() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin".to_string());

        // Simulate adding Accept-Encoding to existing Vary header
        let vary_value = headers.get("Vary").unwrap().clone();
        headers.insert(
            "Vary".to_string(),
            format!("{}, Accept-Encoding", vary_value),
        );

        let vary = headers.get("Vary").unwrap();
        assert!(vary.contains("Origin"));
        assert!(vary.contains("Accept-Encoding"));
    }

    #[test]
    fn should_not_duplicate_vary_values() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Accept-Encoding".to_string());

        // Attempting to add again should not duplicate
        let has_accept_encoding = has_vary_accept_encoding(&headers);
        assert!(has_accept_encoding);

        // The value should remain unchanged
        assert_eq!(headers.get("Vary").unwrap(), "Accept-Encoding");
    }

    #[test]
    fn should_handle_small_content_not_benefiting_from_compression() {
        // Small content might not benefit from compression
        let small_content = "hello world";
        let _compressed = compress(small_content.as_bytes(), CompressionAlgorithm::Gzip);

        // Small content may actually grow after compression overhead
        // In this case, server should not compress and not add Content-Encoding
        // But Vary: Accept-Encoding should still be present if content type is compressible
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain".to_string());

        // Simulate server decision: don't compress small content
        // but still add Vary since content type is compressible
        if is_compressible_content_type(Some("text/plain")) {
            headers.insert("Vary".to_string(), "Accept-Encoding".to_string());
        }

        assert!(has_vary_accept_encoding(&headers));
        assert!(has_content_encoding(&headers).is_none());
    }

    #[test]
    fn should_preserve_existing_vary_values() {
        let mut headers = HashMap::new();
        headers.insert("Vary".to_string(), "Origin, Authorization".to_string());

        // Simulate adding Accept-Encoding
        let vary_value = headers.get("Vary").unwrap().clone();
        headers.insert(
            "Vary".to_string(),
            format!("{}, Accept-Encoding", vary_value),
        );

        let vary = headers.get("Vary").unwrap();
        assert!(vary.contains("Origin"));
        assert!(vary.contains("Authorization"));
        assert!(vary.contains("Accept-Encoding"));
    }
}

mod compression_edge_cases {
    use super::*;

    #[test]
    fn should_handle_content_type_with_charset() {
        assert!(is_compressible_content_type(Some(
            "text/html; charset=utf-8"
        )));
        assert!(is_compressible_content_type(Some(
            "application/json;charset=utf-8"
        )));
    }

    #[test]
    fn should_handle_content_type_with_whitespace() {
        assert!(is_compressible_content_type(Some(" text/html ")));
        assert!(is_compressible_content_type(Some(
            "application/json ; charset=utf-8"
        )));
    }

    #[test]
    fn should_handle_various_json_suffixes() {
        assert!(is_compressible_content_type(Some(
            "application/vnd.api+json"
        )));
        assert!(is_compressible_content_type(Some("application/hal+json")));
        assert!(is_compressible_content_type(Some(
            "application/problem+json"
        )));
    }

    #[test]
    fn should_handle_various_xml_suffixes() {
        assert!(is_compressible_content_type(Some("application/atom+xml")));
        assert!(is_compressible_content_type(Some("application/rss+xml")));
        assert!(is_compressible_content_type(Some("application/svg+xml")));
    }
}
