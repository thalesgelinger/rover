use std::path::Path;

const MAX_PATH_SEGMENTS: usize = 3;
const MAX_IDENTIFIER_LENGTH: usize = 20;

pub fn sanitize_path(path: &str) -> String {
    if path.is_empty() {
        return "<empty>".to_string();
    }

    let is_absolute = path.starts_with('/');

    let p = Path::new(path);

    let segments: Vec<_> = p
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            std::path::Component::CurDir => None,
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect();

    let total_segments = segments.len();

    if total_segments == 0 {
        return "<empty>".to_string();
    }

    let sanitized_segments: Vec<String> = if total_segments <= MAX_PATH_SEGMENTS {
        segments.iter().map(|s| sanitize_path_segment(s)).collect()
    } else {
        let first: Vec<String> = segments
            .iter()
            .take(2)
            .map(|s| sanitize_path_segment(s))
            .collect();
        let last = sanitize_path_segment(segments[total_segments - 1]);

        let mut result = first;
        result.push("...".to_string());
        result.push(last);
        result
    };

    if is_absolute {
        format!("/{}", sanitized_segments.join("/"))
    } else {
        sanitized_segments.join("/")
    }
}

fn sanitize_path_segment(segment: &str) -> String {
    let hidden = hide_secrets_in_segment(segment);
    if hidden != segment {
        return hidden;
    }

    if segment.len() <= 16 {
        segment.to_string()
    } else {
        let start = &segment[..8];
        let end = &segment[segment.len() - 4..];
        format!("{}...{}", start, end)
    }
}

fn hide_secrets_in_segment(segment: &str) -> String {
    let sensitive_patterns = [
        "password",
        "passwd",
        "secret",
        "token",
        "key",
        "credential",
        "api_key",
        "apikey",
        "auth",
        "private",
    ];

    let lower = segment.to_lowercase();
    for pattern in sensitive_patterns.iter() {
        if lower.contains(pattern) {
            return "***".to_string();
        }
    }

    segment.to_string()
}

pub fn sanitize_identifier(identifier: &str) -> String {
    if identifier.is_empty() {
        return "<empty>".to_string();
    }

    if identifier.len() <= MAX_IDENTIFIER_LENGTH {
        hide_secrets_in_segment(identifier)
    } else {
        format!(
            "{}...{}",
            &identifier[..8],
            &identifier[identifier.len() - 4..]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_absolute_path() {
        // paths with sensitive keywords are sanitized
        assert_eq!(sanitize_path("/etc/passwd"), "/etc/***");
        // long paths are truncated - 4 segments exceeds limit of 3
        assert_eq!(
            sanitize_path("/home/user/.ssh/id_rsa"),
            "/home/user/.../id_rsa"
        );
    }

    #[test]
    fn test_sanitize_long_path() {
        let long_path = "/very/long/path/with/many/segments/file.txt";
        // 6 segments: very, long, path, with, many, segments, file.txt
        // should show first 2, then "...", then last: /very/long/.../file.txt
        assert_eq!(sanitize_path(long_path), "/very/long/.../file.txt");
    }

    #[test]
    fn test_sanitize_hides_secrets() {
        // "secrets" contains "secret" so it gets sanitized
        assert_eq!(sanitize_path("/secrets/password.txt"), "/***/***");
        // "api_key" gets sanitized
        assert_eq!(sanitize_path("/config/api_key.json"), "/config/***");
        // "private" gets sanitized
        assert_eq!(sanitize_path("/app/private_data"), "/app/***");
    }

    #[test]
    fn test_sanitize_hides_tokens() {
        // "tokens" contains "token" so it gets sanitized
        // Both segments have sensitive keywords: "tokens" -> "***" (token), "auth" -> "***" (auth)
        assert_eq!(sanitize_path("/tokens/auth"), "/***/***");
        // "secret" gets sanitized to "***"
        assert_eq!(sanitize_path("/keys/secret"), "/***/***");
    }

    #[test]
    fn test_sanitize_long_segment() {
        let segment = "very_long_filename_with_many_characters.txt";
        // 39 chars, first 8 = "very_lon", last 4 = ".txt"
        // But wait, last 4 chars of the segment are "s.txt" not ".txt"
        assert_eq!(sanitize_path_segment(segment), "very_lon....txt");
    }

    #[test]
    fn test_sanitize_simple_path() {
        assert_eq!(sanitize_path("file.txt"), "file.txt");
        assert_eq!(sanitize_path("dir/file.txt"), "dir/file.txt");
    }

    #[test]
    fn test_sanitize_identifier_short() {
        assert_eq!(sanitize_identifier("user123"), "user123");
        assert_eq!(sanitize_identifier("short"), "short");
    }

    #[test]
    fn test_sanitize_identifier_long() {
        let long_id = "user_with_very_long_identifier_name_12345";
        assert_eq!(sanitize_identifier(long_id), "user_wit...2345");
    }

    #[test]
    fn test_sanitize_identifier_hides_secrets() {
        assert_eq!(sanitize_identifier("api_key_12345"), "***");
        assert_eq!(sanitize_identifier("password_secret"), "***");
        assert_eq!(sanitize_identifier("auth_token"), "***");
    }

    #[test]
    fn test_sanitize_identifier_empty() {
        assert_eq!(sanitize_identifier(""), "<empty>");
    }

    #[test]
    fn test_sanitize_path_with_curdir() {
        assert_eq!(sanitize_path("./file.txt"), "file.txt");
        assert_eq!(sanitize_path("./dir/file.txt"), "dir/file.txt");
    }

    #[test]
    fn test_sanitize_path_preserves_reasonable_paths() {
        assert_eq!(sanitize_path("css/style.css"), "css/style.css");
        assert_eq!(sanitize_path("js/app.js"), "js/app.js");
        assert_eq!(sanitize_path("assets/logo.png"), "assets/logo.png");
    }

    #[test]
    fn test_sanitize_path_with_extension() {
        assert_eq!(sanitize_path("index.html"), "index.html");
        assert_eq!(sanitize_path("/public/index.html"), "/public/index.html");
    }

    #[test]
    fn test_hide_secrets_in_segment_preserves_normal() {
        assert_eq!(hide_secrets_in_segment("normal"), "normal");
        assert_eq!(hide_secrets_in_segment("file"), "file");
        assert_eq!(hide_secrets_in_segment("document"), "document");
    }

    #[test]
    fn test_hide_secrets_in_segment_hides_sensitive() {
        assert_eq!(hide_secrets_in_segment("password"), "***");
        assert_eq!(hide_secrets_in_segment("my_secret"), "***");
        assert_eq!(hide_secrets_in_segment("API_KEY"), "***");
        assert_eq!(hide_secrets_in_segment("credentials"), "***");
    }
}
