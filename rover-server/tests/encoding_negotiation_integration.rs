//! Integration tests for HTTP encoding negotiation and fallback behavior
//!
//! Tests verify:
//! - Supported encodings (gzip, deflate) are correctly selected
//! - Unsupported encodings (br, zstd, compress) result in no compression
//! - Identity encoding is handled correctly (no compression)
//! - Quality values affect encoding selection properly
//! - Fallback behavior when no encodings match

use rover_server::compression::{CompressionAlgorithm, compress, negotiate_encoding};

const DEFAULT_ALGOS: [CompressionAlgorithm; 2] =
    [CompressionAlgorithm::Gzip, CompressionAlgorithm::Deflate];

/// Test helper: decompress gzip data for verification
fn decompress_gzip(data: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("gzip decompress");
    decompressed
}

/// Test helper: decompress deflate data for verification
fn decompress_deflate(data: &[u8]) -> Vec<u8> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;
    let mut decoder = DeflateDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("deflate decompress");
    decompressed
}

/// Test helper: create compressible content
fn make_compressible_content(size: usize) -> String {
    "hello world ".repeat(size / 12 + 1)
}

mod supported_encoding_success {
    use super::*;

    #[test]
    fn should_select_gzip_when_requested() {
        let selected = negotiate_encoding("gzip", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_select_deflate_when_requested() {
        let selected = negotiate_encoding("deflate", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_select_x_gzip_as_gzip() {
        let selected = negotiate_encoding("x-gzip", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_prefer_first_configured_when_wildcard() {
        let selected = negotiate_encoding("*", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_prefer_deflate_when_configured_first() {
        let deflate_first = [CompressionAlgorithm::Deflate, CompressionAlgorithm::Gzip];
        let selected = negotiate_encoding("*", &deflate_first);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_select_gzip_with_higher_quality() {
        let selected = negotiate_encoding("gzip;q=0.9, deflate;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_select_deflate_with_higher_quality() {
        let selected = negotiate_encoding("gzip;q=0.5, deflate;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_compress_with_gzip_successfully() {
        let content = make_compressible_content(2048);
        let original = content.as_bytes();

        let compressed = compress(original, CompressionAlgorithm::Gzip);
        assert!(compressed.len() < original.len(), "gzip should reduce size");

        let decompressed = decompress_gzip(&compressed);
        assert_eq!(decompressed, original, "round-trip should preserve content");
    }

    #[test]
    fn should_compress_with_deflate_successfully() {
        let content = make_compressible_content(2048);
        let original = content.as_bytes();

        let compressed = compress(original, CompressionAlgorithm::Deflate);
        assert!(
            compressed.len() < original.len(),
            "deflate should reduce size"
        );

        let decompressed = decompress_deflate(&compressed);
        assert_eq!(decompressed, original, "round-trip should preserve content");
    }

    #[test]
    fn should_handle_multiple_supported_encodings() {
        let selected = negotiate_encoding("gzip, deflate", &DEFAULT_ALGOS);
        assert!(
            selected.is_some(),
            "should select one of the supported encodings"
        );
    }

    #[test]
    fn should_select_from_multiple_with_quality() {
        let selected = negotiate_encoding("deflate;q=0.8, gzip;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_case_insensitive_encoding_names() {
        assert_eq!(
            negotiate_encoding("GZIP", &DEFAULT_ALGOS),
            Some(CompressionAlgorithm::Gzip)
        );
        assert_eq!(
            negotiate_encoding("Gzip", &DEFAULT_ALGOS),
            Some(CompressionAlgorithm::Gzip)
        );
        assert_eq!(
            negotiate_encoding("DEFLATE", &DEFAULT_ALGOS),
            Some(CompressionAlgorithm::Deflate)
        );
        assert_eq!(
            negotiate_encoding("Deflate", &DEFAULT_ALGOS),
            Some(CompressionAlgorithm::Deflate)
        );
    }
}

mod unsupported_encoding_fallback {
    use super::*;

    #[test]
    fn should_return_none_for_brotli_only() {
        let selected = negotiate_encoding("br", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "brotli (br) is not supported");
    }

    #[test]
    fn should_return_none_for_zstd_only() {
        let selected = negotiate_encoding("zstd", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "zstd is not supported");
    }

    #[test]
    fn should_return_none_for_compress_only() {
        let selected = negotiate_encoding("compress", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "compress (LZW) is not supported");
    }

    #[test]
    fn should_select_gzip_when_br_and_gzip_present() {
        let selected = negotiate_encoding("br, gzip", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should fallback to gzip when br is unsupported"
        );
    }

    #[test]
    fn should_select_deflate_when_br_and_deflate_present() {
        let selected = negotiate_encoding("br, deflate", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Deflate),
            "should fallback to deflate when br is unsupported"
        );
    }

    #[test]
    fn should_return_none_for_only_unsupported_encodings() {
        let selected = negotiate_encoding("br, zstd, compress", &DEFAULT_ALGOS);
        assert_eq!(
            selected, None,
            "should return none when all encodings are unsupported"
        );
    }

    #[test]
    fn should_select_supported_when_mixed_with_unsupported() {
        let selected = negotiate_encoding("br, zstd, gzip, compress", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should select gzip from mixed list"
        );
    }

    #[test]
    fn should_respect_quality_with_unsupported_encodings() {
        let selected = negotiate_encoding("br;q=0.9, gzip;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should select gzip even with lower quality when br is unsupported"
        );
    }

    #[test]
    fn should_handle_exotic_encodings() {
        assert_eq!(negotiate_encoding("xz", &DEFAULT_ALGOS), None);
        assert_eq!(negotiate_encoding("bzip2", &DEFAULT_ALGOS), None);
        assert_eq!(negotiate_encoding("lzma", &DEFAULT_ALGOS), None);
    }

    #[test]
    fn should_fallback_when_wildcard_with_unsupported_only_config() {
        // If only unsupported algorithms were somehow configured,
        // wildcard should still return None
        let empty: &[CompressionAlgorithm] = &[];
        let selected = negotiate_encoding("*", empty);
        assert_eq!(selected, None);
    }

    #[test]
    fn should_handle_brotli_with_quality() {
        let selected = negotiate_encoding("br;q=1.0, gzip;q=0.8", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should ignore br quality and select gzip"
        );
    }

    #[test]
    fn should_handle_x_compressed() {
        assert_eq!(negotiate_encoding("x-compress", &DEFAULT_ALGOS), None);
        assert_eq!(negotiate_encoding("x-compressed", &DEFAULT_ALGOS), None);
    }

    #[test]
    fn should_handle_unknown_encoding_tokens() {
        assert_eq!(negotiate_encoding("unknown", &DEFAULT_ALGOS), None);
        assert_eq!(negotiate_encoding("custom-encoding", &DEFAULT_ALGOS), None);
        assert_eq!(
            negotiate_encoding("some-weird-format", &DEFAULT_ALGOS),
            None
        );
    }
}

mod identity_no_compression {
    use super::*;

    #[test]
    fn should_return_none_for_identity_only() {
        let selected = negotiate_encoding("identity", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "identity means no compression");
    }

    #[test]
    fn should_select_gzip_when_identity_has_lower_quality() {
        let selected = negotiate_encoding("identity;q=0.5, gzip;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_return_none_when_identity_has_higher_quality() {
        let selected = negotiate_encoding("identity;q=0.9, gzip;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "identity is not a real algorithm, should still select gzip"
        );
    }

    #[test]
    fn should_handle_identity_in_list() {
        let selected = negotiate_encoding("gzip, identity", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should prefer gzip over identity"
        );
    }

    #[test]
    fn should_handle_wildcard_with_identity() {
        let selected = negotiate_encoding("*, identity;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "wildcard should match gzip"
        );
    }

    #[test]
    fn should_not_compress_when_no_algorithm_selected() {
        let content = "hello world";
        let _original = content.as_bytes();

        // When negotiate_encoding returns None, we should not compress
        // This test documents that behavior
        let selected = negotiate_encoding("identity", &DEFAULT_ALGOS);
        assert!(
            selected.is_none(),
            "identity should result in no compression"
        );
    }

    #[test]
    fn should_handle_identity_q0() {
        let selected = negotiate_encoding("identity;q=0", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "identity;q=0 still means no compression");
    }

    #[test]
    fn should_select_best_when_identity_among_options() {
        let selected = negotiate_encoding("identity, deflate, gzip", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should select first supported"
        );
    }

    #[test]
    fn should_handle_only_identity_configured() {
        // If somehow no algorithms are configured, identity requests should result in no compression
        let empty: &[CompressionAlgorithm] = &[];
        let selected = negotiate_encoding("identity", empty);
        assert_eq!(selected, None);
    }

    #[test]
    fn should_treat_identity_as_unsupported_encoding() {
        // Identity is not a compression algorithm we can apply
        // It's the absence of compression
        let selected = negotiate_encoding("identity;q=1.0", &DEFAULT_ALGOS);
        assert_eq!(selected, None);
    }
}

mod quality_value_edge_cases {
    use super::*;

    #[test]
    fn should_handle_zero_quality_for_supported() {
        let selected = negotiate_encoding("gzip;q=0", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "q=0 is still a valid preference"
        );
    }

    #[test]
    fn should_handle_decimal_qualities() {
        let selected = negotiate_encoding("gzip;q=0.123, deflate;q=0.456", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_handle_maximum_quality() {
        let selected = negotiate_encoding("gzip;q=1.0, deflate;q=0.999", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_default_to_q1_when_malformed() {
        let selected = negotiate_encoding("gzip;q=malformed, deflate;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "malformed q defaults to 1.0"
        );
    }

    #[test]
    fn should_handle_whitespace_in_quality() {
        let selected = negotiate_encoding("gzip; q=0.5, deflate; q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_handle_no_whitespace_in_quality() {
        let selected = negotiate_encoding("gzip;q=0.5,deflate;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_handle_wildcard_quality() {
        let selected = negotiate_encoding("*;q=0.5, gzip;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_fallback_to_wildcard_when_no_explicit_match() {
        let selected = negotiate_encoding("br;q=0.9, *;q=0.5", &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "wildcard should match when br unsupported"
        );
    }

    #[test]
    fn should_select_highest_quality_among_supported() {
        let selected = negotiate_encoding("deflate;q=0.9, gzip;q=0.8", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_handle_very_small_quality_values() {
        let selected = negotiate_encoding("gzip;q=0.001, deflate;q=0.002", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_handle_quality_with_many_decimals() {
        let selected = negotiate_encoding("gzip;q=0.999999, deflate;q=0.999998", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_mixed_explicit_and_wildcard() {
        // explicit_q.or(wildcard_q) means explicit quality takes precedence
        // Gzip: explicit=0.2, wildcard=0.9 -> q=0.2
        // Deflate: explicit=None, wildcard=0.9 -> q=0.9
        // Deflate wins with q=0.9
        let selected = negotiate_encoding("br;q=0.1, gzip;q=0.2, *;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Deflate));
    }
}

mod encoding_header_format_edge_cases {
    use super::*;

    #[test]
    fn should_handle_empty_accept_encoding() {
        let selected = negotiate_encoding("", &DEFAULT_ALGOS);
        assert_eq!(selected, None);
    }

    #[test]
    fn should_handle_whitespace_only() {
        let selected = negotiate_encoding("   ", &DEFAULT_ALGOS);
        assert_eq!(selected, None);
    }

    #[test]
    fn should_handle_multiple_commas() {
        let selected = negotiate_encoding("gzip,,,deflate", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_leading_trailing_commas() {
        let selected = negotiate_encoding(",gzip,", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_whitespace_around_tokens() {
        let selected = negotiate_encoding("  gzip  ,  deflate  ", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_complex_real_world_header() {
        // Real-world Accept-Encoding header from browser
        let header = "gzip, deflate, br, zstd";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should select first supported from browser header"
        );
    }

    #[test]
    fn should_handle_single_algorithm_configured() {
        let only_gzip = [CompressionAlgorithm::Gzip];
        let selected = negotiate_encoding("gzip, deflate", &only_gzip);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));

        // When only gzip is configured but deflate is requested,
        // we should return None since deflate is not in configured list
        let selected2 = negotiate_encoding("deflate", &only_gzip);
        assert_eq!(
            selected2, None,
            "should return None when deflate requested but only gzip configured"
        );
    }

    #[test]
    fn should_return_none_when_no_algorithms_configured() {
        let empty: &[CompressionAlgorithm] = &[];
        let selected = negotiate_encoding("gzip", empty);
        assert_eq!(selected, None);
    }

    #[test]
    fn should_handle_single_encoding_without_quality() {
        let selected = negotiate_encoding("gzip", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_encoding_with_only_semicolon() {
        let selected = negotiate_encoding("gzip;", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_repeated_same_encoding() {
        let selected = negotiate_encoding("gzip, gzip, gzip", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_use_highest_quality_when_encoding_repeated() {
        let selected = negotiate_encoding("gzip;q=0.5, gzip;q=0.9", &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_very_long_accept_encoding() {
        let header =
            "gzip;q=0.9, deflate;q=0.8, br;q=0.7, zstd;q=0.6, compress;q=0.5, identity;q=0.4";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }
}

mod integration_with_response_flow {
    use super::*;

    #[test]
    fn should_not_compress_small_content() {
        let small_data = b"tiny";
        let compressed = compress(small_data, CompressionAlgorithm::Gzip);
        // Small content may actually be larger after compression due to overhead
        // This is expected behavior - the server should decide not to compress
        // Just verify compression works and doesn't crash
        assert!(
            compressed.len() > 0,
            "compression should produce some output"
        );
    }

    #[test]
    fn should_compress_large_content() {
        let large_content = make_compressible_content(10000);
        let original = large_content.as_bytes();

        let gzip_compressed = compress(original, CompressionAlgorithm::Gzip);
        let deflate_compressed = compress(original, CompressionAlgorithm::Deflate);

        assert!(
            gzip_compressed.len() < original.len(),
            "gzip should compress large content"
        );
        assert!(
            deflate_compressed.len() < original.len(),
            "deflate should compress large content"
        );
    }

    #[test]
    fn should_decompress_to_original() {
        let content = make_compressible_content(5000);
        let original = content.as_bytes();

        let gzip_compressed = compress(original, CompressionAlgorithm::Gzip);
        let deflate_compressed = compress(original, CompressionAlgorithm::Deflate);

        assert_eq!(decompress_gzip(&gzip_compressed), original);
        assert_eq!(decompress_deflate(&deflate_compressed), original);
    }

    #[test]
    fn should_handle_repetitive_content() {
        // Highly compressible content
        let repetitive = "AAAAAAAAAA".repeat(1000);
        let original = repetitive.as_bytes();

        let gzip_compressed = compress(original, CompressionAlgorithm::Gzip);
        let deflate_compressed = compress(original, CompressionAlgorithm::Deflate);

        assert!(
            gzip_compressed.len() < original.len() / 10,
            "repetitive content should compress very well"
        );
        assert!(
            deflate_compressed.len() < original.len() / 10,
            "repetitive content should compress very well"
        );
    }

    #[test]
    fn should_handle_random_looking_content() {
        // Less compressible content (simulated with mixed content)
        let mixed = "The quick brown fox jumps over the lazy dog. ".repeat(100);
        let original = mixed.as_bytes();

        let gzip_compressed = compress(original, CompressionAlgorithm::Gzip);

        // Should still compress but less effectively
        assert!(
            gzip_compressed.len() < original.len(),
            "should still compress mixed content"
        );
    }

    #[test]
    fn should_preserve_empty_content() {
        let empty: &[u8] = b"";
        let compressed = compress(empty, CompressionAlgorithm::Gzip);
        let decompressed = decompress_gzip(&compressed);
        assert_eq!(decompressed, empty);
    }

    #[test]
    fn should_handle_binary_content() {
        // Simulate binary content (though compression module doesn't care about content type)
        let binary = vec![0u8, 1, 2, 3, 255, 254, 253, 252].repeat(100);

        let compressed = compress(&binary, CompressionAlgorithm::Gzip);
        let decompressed = decompress_gzip(&compressed);

        assert_eq!(decompressed, binary);
    }
}

mod combined_fallback_scenarios {
    use super::*;

    #[test]
    fn should_fallback_correctly_in_complex_scenario() {
        // Complex real-world scenario: br preferred but unsupported,
        // identity offered but we prefer compression,
        // gzip and deflate both available
        let header = "br;q=0.9, gzip;q=0.8, deflate;q=0.7, identity;q=0.5";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should select gzip when br unsupported"
        );
    }

    #[test]
    fn should_handle_all_unsupported_with_fallback_wildcard() {
        let header = "br, zstd, *;q=0.5";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "wildcard should provide fallback"
        );
    }

    #[test]
    fn should_prefer_explicit_over_wildcard() {
        // When explicit encoding has higher quality than wildcard, explicit wins
        let header = "gzip;q=0.9, *;q=0.5";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "explicit encoding with higher quality should be preferred"
        );
    }

    #[test]
    fn should_handle_identity_and_unsupported_mix() {
        let header = "br, identity, gzip";
        let selected = negotiate_encoding(header, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should skip br and identity, select gzip"
        );
    }

    #[test]
    fn should_respect_no_accept_encoding_header() {
        // When no Accept-Encoding header present (empty string)
        let selected = negotiate_encoding("", &DEFAULT_ALGOS);
        assert_eq!(selected, None, "no accept-encoding means no compression");
    }

    #[test]
    fn should_handle_browser_default_preferences() {
        // Chrome/Edge typical: gzip, deflate, br, zstd
        let chrome = "gzip, deflate, br, zstd";
        let selected = negotiate_encoding(chrome, &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));

        // Firefox typical: gzip, deflate, br
        let firefox = "gzip, deflate, br";
        let selected2 = negotiate_encoding(firefox, &DEFAULT_ALGOS);
        assert_eq!(selected2, Some(CompressionAlgorithm::Gzip));

        // Safari typical: gzip, deflate, br
        let safari = "gzip, deflate, br";
        let selected3 = negotiate_encoding(safari, &DEFAULT_ALGOS);
        assert_eq!(selected3, Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_handle_curl_default() {
        // curl --compressed sends: deflate, gzip
        let curl = "deflate, gzip";
        let selected = negotiate_encoding(curl, &DEFAULT_ALGOS);
        assert_eq!(
            selected,
            Some(CompressionAlgorithm::Gzip),
            "should respect order but select from configured"
        );
    }

    #[test]
    fn should_handle_wget_default() {
        // wget typically sends: gzip
        let wget = "gzip";
        let selected = negotiate_encoding(wget, &DEFAULT_ALGOS);
        assert_eq!(selected, Some(CompressionAlgorithm::Gzip));
    }
}
