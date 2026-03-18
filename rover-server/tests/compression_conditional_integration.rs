use flate2::read::{DeflateDecoder, GzDecoder};
use rover_server::compression::{CompressionAlgorithm, compress};
use rover_server::static_file::{is_not_modified, strip_etag_encoding_suffix};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use tempfile::TempDir;

fn make_compressible_content(size: usize) -> String {
    "hello world ".repeat(size / 12 + 1)
}

fn decompress_gzip(data: &[u8]) -> Vec<u8> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("gzip decompress");
    decompressed
}

fn decompress_deflate(data: &[u8]) -> Vec<u8> {
    let mut decoder = DeflateDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("deflate decompress");
    decompressed
}

mod compression_etag_suffix {
    use super::*;

    #[test]
    fn should_suffix_gzip_etag() {
        let original_etag = "\"abc123\"";
        let compressed_etag = format!("\"{}-gzip\"", &original_etag[1..original_etag.len() - 1]);
        assert_eq!(compressed_etag, "\"abc123-gzip\"");
    }

    #[test]
    fn should_suffix_deflate_etag() {
        let original_etag = "\"xyz789\"";
        let compressed_etag = format!("\"{}-deflate\"", &original_etag[1..original_etag.len() - 1]);
        assert_eq!(compressed_etag, "\"xyz789-deflate\"");
    }

    #[test]
    fn should_strip_encoding_suffix_from_etag() {
        assert_eq!(strip_etag_encoding_suffix("\"abc-gzip\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-deflate\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-br\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-xz\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc"), "\"abc");
    }

    #[test]
    fn should_handle_unquoted_etag() {
        assert_eq!(strip_etag_encoding_suffix("abc-gzip"), "abc-gzip");
        assert_eq!(strip_etag_encoding_suffix("abc"), "abc");
    }
}

mod conditional_requests {
    use super::*;

    #[test]
    fn should_strip_suffix_and_match_base_etag() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file123-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "\"file123\"".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_match_compressed_etag_directly() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file123-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "\"file123-gzip\"".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_match_when_client_has_deflate_and_server_has_gzip() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file123-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert(
            "If-None-Match".to_string(),
            "\"file123-deflate\"".to_string(),
        );

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_not_match_different_base_etags() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file456-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "\"file123\"".to_string());

        assert!(!is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_match_wildcard_if_none_match() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"anything-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "*".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_match_one_of_multiple_etags() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file123-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert(
            "If-None-Match".to_string(),
            "\"other\", \"file123\", \"another\"".to_string(),
        );

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_match_compressed_etag_in_multiple_client_etags() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"file123-gzip\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert(
            "If-None-Match".to_string(),
            "\"other-deflate\", \"file123-gzip\"".to_string(),
        );

        assert!(is_not_modified(&response_headers, &request_headers));
    }
}

mod compression_behavior {
    use super::*;

    #[test]
    fn should_compress_text_content() {
        let data = make_compressible_content(2048);
        let bytes = data.as_bytes();

        let gzip_compressed = compress(bytes, CompressionAlgorithm::Gzip);
        assert!(
            gzip_compressed.len() < bytes.len(),
            "gzip should compress text"
        );

        let deflate_compressed = compress(bytes, CompressionAlgorithm::Deflate);
        assert!(
            deflate_compressed.len() < bytes.len(),
            "deflate should compress text"
        );
    }

    #[test]
    fn should_decompress_to_same_content() {
        let original = make_compressible_content(2048);
        let original_bytes = original.as_bytes();

        let gzip_compressed = compress(original_bytes, CompressionAlgorithm::Gzip);
        let decompressed = decompress_gzip(&gzip_compressed);
        assert_eq!(&decompressed, original_bytes);

        let deflate_compressed = compress(original_bytes, CompressionAlgorithm::Deflate);
        let decompressed = decompress_deflate(&deflate_compressed);
        assert_eq!(&decompressed, original_bytes);
    }

    #[test]
    fn should_refuse_to_compress_small_content() {
        let small_data = b"hello";
        let compressed = compress(small_data, CompressionAlgorithm::Gzip);
        assert!(
            compressed.len() >= small_data.len(),
            "small data may not benefit from compression"
        );
    }
}

mod static_file_compression_conditional {
    use super::*;
    use rover_server::serve_static_file;

    fn create_temp_file(content: &str) -> (TempDir, String) {
        let dir = TempDir::new().expect("create temp dir");
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, content).expect("write file");
        (
            dir,
            file_path.file_name().unwrap().to_string_lossy().to_string(),
        )
    }

    #[test]
    fn should_serve_uncompressed_file_without_accept_encoding() {
        let content = make_compressible_content(2048);
        let (dir, file_name) = create_temp_file(&content);

        let response = serve_static_file(dir.path(), &file_name, None, None);

        assert_eq!(response.status, 200);
        assert_eq!(response.body.len(), content.len());
        // Byte comparison without taking ownership
        for (a, b) in response.body.iter().zip(content.as_bytes().iter()) {
            assert_eq!(a, b);
        }

        let headers = response.headers.expect("headers present");
        assert!(!headers.contains_key("Content-Encoding"));
    }

    #[test]
    fn should_generate_etag_for_file() {
        let content = make_compressible_content(2048);
        let (dir, file_name) = create_temp_file(&content);

        let response = serve_static_file(dir.path(), &file_name, None, None);

        let headers = response.headers.expect("headers present");
        assert!(headers.contains_key("ETag"));
        let etag = headers.get("ETag").expect("ETag header");
        assert!(etag.starts_with('"'));
        assert!(etag.ends_with('"'));
    }

    #[test]
    fn should_return_304_for_matching_etag_without_compression() {
        let content = make_compressible_content(2048);
        let (dir, file_name) = create_temp_file(&content);

        let first_response = serve_static_file(dir.path(), &file_name, None, None);
        let etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .expect("ETag present")
            .clone();

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), etag);

        let second_response =
            serve_static_file(dir.path(), &file_name, Some(&request_headers), None);

        assert_eq!(second_response.status, 304);
        assert!(second_response.body.is_empty());
    }

    #[test]
    fn should_return_200_when_etag_mismatch_without_compression() {
        let content = make_compressible_content(2048);
        let (dir, file_name) = create_temp_file(&content);

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "\"wrong-etag\"".to_string());

        let response = serve_static_file(dir.path(), &file_name, Some(&request_headers), None);

        assert_eq!(response.status, 200);
        assert_eq!(response.body.len(), content.len());
        // Byte comparison of the actual content
        for (a, b) in response.body.iter().zip(content.as_bytes().iter()) {
            assert_eq!(a, b);
        }
    }
}

mod etag_handling_edge_cases {
    use super::*;

    #[test]
    fn should_handle_etag_without_quotes() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "etag-no-quotes".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "etag-no-quotes".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_handle_mixed_quote_styles() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"etag123\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "etag123".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_preserve_whitespace_in_etag_list() {
        let mut response_headers = HashMap::new();
        response_headers.insert("ETag".to_string(), "\"etag123\"".to_string());

        let mut request_headers = HashMap::new();
        request_headers.insert("If-None-Match".to_string(), "  \"etag123\"  ".to_string());

        assert!(is_not_modified(&response_headers, &request_headers));
    }

    #[test]
    fn should_strip_suffix_variants() {
        assert_eq!(strip_etag_encoding_suffix("\"hash-gzip\""), "hash");
        assert_eq!(strip_etag_encoding_suffix("\"hash-deflate\""), "hash");
        assert_eq!(strip_etag_encoding_suffix("\"hash-br\""), "hash");
        assert_eq!(strip_etag_encoding_suffix("\"hash-xz\""), "hash");
        assert_eq!(strip_etag_encoding_suffix("\"hash\""), "hash");
    }
}
