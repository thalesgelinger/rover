use flate2::Compression;
use flate2::write::{DeflateEncoder, GzEncoder};
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    Gzip,
    Deflate,
}

impl std::fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionAlgorithm::Gzip => write!(f, "gzip"),
            CompressionAlgorithm::Deflate => write!(f, "deflate"),
        }
    }
}

pub fn negotiate_encoding(
    accept_encoding: &str,
    configured_algorithms: &[CompressionAlgorithm],
) -> Option<CompressionAlgorithm> {
    if configured_algorithms.is_empty() {
        return None;
    }

    let mut wildcard_q: Option<f32> = None;
    let mut gzip_q: Option<f32> = None;
    let mut deflate_q: Option<f32> = None;

    for part in accept_encoding.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, q) = if let Some((n, rest)) = trimmed.split_once(';') {
            let q_val = rest
                .trim()
                .strip_prefix("q=")
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(1.0);
            (n.trim(), q_val)
        } else {
            (trimmed, 1.0)
        };
        match name.to_ascii_lowercase().as_str() {
            "gzip" | "x-gzip" => {
                if gzip_q.is_none_or(|existing| q > existing) {
                    gzip_q = Some(q);
                }
            }
            "deflate" => {
                if deflate_q.is_none_or(|existing| q > existing) {
                    deflate_q = Some(q);
                }
            }
            "*" => {
                if wildcard_q.is_none_or(|existing| q > existing) {
                    wildcard_q = Some(q);
                }
            }
            _ => {}
        }
    }

    let mut best: Option<(CompressionAlgorithm, f32)> = None;
    for algo in configured_algorithms {
        let explicit_q = match algo {
            CompressionAlgorithm::Gzip => gzip_q,
            CompressionAlgorithm::Deflate => deflate_q,
        };
        let Some(q) = explicit_q.or(wildcard_q) else {
            continue;
        };

        if best.is_none_or(|(_, best_q)| q > best_q) {
            best = Some((*algo, q));
        }
    }

    best.map(|(algo, _)| algo)
}

pub fn compress(data: &[u8], algo: CompressionAlgorithm) -> Vec<u8> {
    match algo {
        CompressionAlgorithm::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        }
        CompressionAlgorithm::Deflate => {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_ALGOS: [CompressionAlgorithm; 2] =
        [CompressionAlgorithm::Gzip, CompressionAlgorithm::Deflate];

    fn negotiate(accept_encoding: &str) -> Option<CompressionAlgorithm> {
        negotiate_encoding(accept_encoding, &DEFAULT_ALGOS)
    }

    #[test]
    fn should_negotiate_gzip() {
        assert_eq!(negotiate("gzip"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_negotiate_deflate() {
        assert_eq!(negotiate("deflate"), Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_prefer_gzip_when_wildcard() {
        assert_eq!(negotiate("*"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_use_quality_values() {
        assert_eq!(
            negotiate("gzip;q=0.5, deflate;q=0.9"),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_default_quality_to_one() {
        assert_eq!(
            negotiate("gzip, deflate;q=0.5"),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_negotiate_case_insensitive() {
        assert_eq!(negotiate("GZIP"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_negotiate_x_gzip() {
        assert_eq!(negotiate("x-gzip"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_return_none_for_unsupported() {
        assert_eq!(negotiate("br"), None);
        assert_eq!(negotiate("identity"), None);
    }

    #[test]
    fn should_compress_with_gzip() {
        let data = b"hello worldhello worldhello worldhello worldhello worldhello worldhello worldhello world";
        let compressed = compress(data, CompressionAlgorithm::Gzip);
        assert!(
            compressed.len() < data.len(),
            "gzip should compress repeatable data"
        );
    }

    #[test]
    fn should_compress_with_deflate() {
        let data = b"hello worldhello worldhello worldhello worldhello worldhello worldhello worldhello world";
        let compressed = compress(data, CompressionAlgorithm::Deflate);
        assert!(
            compressed.len() < data.len(),
            "deflate should compress repeatable data"
        );
    }

    #[test]
    fn should_handle_multiple_encodings() {
        assert_eq!(
            negotiate("gzip, deflate, br"),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_handle_whitespace() {
        assert_eq!(
            negotiate("  gzip , deflate "),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_handle_empty_string() {
        assert_eq!(negotiate(""), None);
    }

    #[test]
    fn should_accept_zero_quality_as_valid_encoding() {
        assert_eq!(negotiate("gzip;q=0"), Some(CompressionAlgorithm::Gzip));
        assert_eq!(negotiate("gzip;q=0.0"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_prefer_higher_quality() {
        assert_eq!(
            negotiate("gzip;q=0.8, deflate;q=0.9"),
            Some(CompressionAlgorithm::Deflate)
        );
        assert_eq!(
            negotiate("deflate;q=0.1, gzip;q=0.9"),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_fallback_to_default_quality_on_invalid() {
        assert_eq!(
            negotiate("gzip;q=invalid"),
            Some(CompressionAlgorithm::Gzip)
        );
        assert_eq!(negotiate("gzip;q=abc"), Some(CompressionAlgorithm::Gzip));
    }

    #[test]
    fn should_pick_deflate_when_gzip_has_lower_quality() {
        assert_eq!(
            negotiate("gzip;q=0.1, deflate;q=0.9"),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_handle_quality_without_space() {
        assert_eq!(
            negotiate("gzip;q=0.5,deflate;q=0.9"),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_handle_multiple_commas() {
        assert_eq!(
            negotiate("gzip,,,deflate"),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_deflate_fallback_when_only_deflate() {
        assert_eq!(negotiate("deflate"), Some(CompressionAlgorithm::Deflate));
    }

    #[test]
    fn should_round_trip_quality_values() {
        assert_eq!(
            negotiate("gzip;q=0.500, deflate;q=0.900"),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_preserve_first_on_equal_quality() {
        assert_eq!(
            negotiate("deflate;q=0.8, gzip;q=0.8"),
            Some(CompressionAlgorithm::Gzip)
        );
    }

    #[test]
    fn should_use_configured_algorithm_order_for_wildcard() {
        let configured = [CompressionAlgorithm::Deflate, CompressionAlgorithm::Gzip];
        assert_eq!(
            negotiate_encoding("*", &configured),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_support_single_configured_algorithm() {
        let configured = [CompressionAlgorithm::Deflate];
        assert_eq!(
            negotiate_encoding("gzip, deflate", &configured),
            Some(CompressionAlgorithm::Deflate)
        );
    }

    #[test]
    fn should_return_none_when_no_algorithm_is_configured() {
        assert_eq!(negotiate_encoding("gzip, deflate", &[]), None);
    }
}
