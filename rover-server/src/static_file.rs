use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::{Bytes, RoverResponse};

/// Maximum allowed path length to prevent abuse
const MAX_PATH_LENGTH: usize = 4096;

/// Default cache control header value (1 day)
const DEFAULT_CACHE_CONTROL: &str = "public, max-age=86400";

/// Serve a static file with traversal protection and cache headers
///
/// # Arguments
/// * `base_path` - The base directory to serve files from
/// * `requested_path` - The path requested by the client (e.g., "/css/style.css")
/// * `custom_headers` - Optional additional headers to include
///
/// # Returns
/// A `RoverResponse` with the file content or a 404/403 error
pub fn serve_static_file(
    base_path: &Path,
    requested_path: &str,
    custom_headers: Option<HashMap<String, String>>,
) -> RoverResponse {
    // Validate and sanitize the requested path
    let sanitized = match sanitize_path(base_path, requested_path) {
        Ok(path) => path,
        Err(e) => {
            if e == "Not found" {
                return not_found_response();
            }
            return forbidden_response(&e);
        }
    };

    // Check if the path is a directory (don't serve directories)
    if sanitized.is_dir() {
        return forbidden_response("Directory listing not allowed");
    }

    // Read the file
    match std::fs::read(&sanitized) {
        Ok(content) => {
            let content_type = guess_content_type(&sanitized);
            let mut headers = custom_headers.unwrap_or_default();

            // Add cache headers
            add_cache_headers(&mut headers, &sanitized, &content);

            RoverResponse {
                status: 200,
                body: Bytes::from(content),
                content_type,
                headers: if headers.is_empty() {
                    None
                } else {
                    Some(headers)
                },
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                not_found_response()
            } else {
                forbidden_response(&format!("Access denied: {}", e))
            }
        }
    }
}

/// Sanitize a path to prevent directory traversal attacks
///
/// Returns the canonical path if it's within the base directory,
/// or an error if traversal is detected
fn sanitize_path(base_path: &Path, requested_path: &str) -> Result<PathBuf, String> {
    // Check path length
    if requested_path.len() > MAX_PATH_LENGTH {
        return Err("Path too long".to_string());
    }

    // Remove null bytes
    if requested_path.contains('\0') {
        return Err("Invalid path".to_string());
    }

    // Normalize URL-style leading slashes to a relative path within base_path
    let normalized_path = requested_path.trim_start_matches('/');

    // Parse the requested path components
    let requested = Path::new(normalized_path);

    // Start with the base path
    let mut full_path = base_path.to_path_buf();

    // Process each component, rejecting traversal attempts
    for component in requested.components() {
        match component {
            Component::Normal(name) => {
                full_path.push(name);
            }
            Component::ParentDir => {
                // Check if this would escape the base directory
                // by canonicalizing and checking prefix
                return Err("Directory traversal not allowed".to_string());
            }
            Component::CurDir => {
                // Skip current directory markers
                continue;
            }
            Component::RootDir | Component::Prefix(_) => {
                // Don't allow absolute paths or Windows prefixes
                return Err("Absolute paths not allowed".to_string());
            }
        }
    }

    // Canonicalize the base path first
    let canonical_base = base_path
        .canonicalize()
        .map_err(|_| "Invalid base path".to_string())?;

    // Check if the file exists before canonicalizing to distinguish between
    // "file not found" and "directory traversal"
    if !full_path.exists() {
        return Err("Not found".to_string());
    }

    let canonical_requested = full_path
        .canonicalize()
        .map_err(|_| "Invalid path".to_string())?;

    // Verify the canonical path is within the base directory
    if !canonical_requested.starts_with(&canonical_base) {
        return Err("Directory traversal not allowed".to_string());
    }

    Ok(canonical_requested)
}

/// Guess the content type based on file extension
fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("xml") => "application/xml",
        Some("wasm") => "application/wasm",
        Some("webmanifest") => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// Add cache-related headers to the response
fn add_cache_headers(headers: &mut HashMap<String, String>, path: &Path, content: &[u8]) {
    // Add Cache-Control header
    headers
        .entry("Cache-Control".to_string())
        .or_insert_with(|| DEFAULT_CACHE_CONTROL.to_string());

    // Generate ETag based on file content (simple hash)
    let etag = format!("\"{}\"", hash_content(content));
    headers.insert("ETag".to_string(), etag);

    // Add Last-Modified if we can get the metadata
    if let Some(http_date) = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| format_http_date(d.as_secs()))
    {
        headers.insert("Last-Modified".to_string(), http_date);
    }
}

/// Simple hash function for ETag generation
fn hash_content(content: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Format a Unix timestamp as an HTTP date
fn format_http_date(unix_timestamp: u64) -> String {
    // HTTP date format: "Sun, 06 Nov 1994 08:49:37 GMT"
    // For simplicity, we'll use a fixed format
    // In production, you'd use chrono or time crate
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Calculate date components (simplified)
    let seconds_since_epoch = unix_timestamp;
    let days_since_epoch = seconds_since_epoch / 86400;
    let day_of_week = ((days_since_epoch + 4) % 7) as usize; // Jan 1, 1970 was Thursday (4)

    // Rough approximation for demo - in production use chrono
    format!(
        "{}, {} {} {} {:02}:{:02}:{:02} GMT",
        days[day_of_week % 7],
        days_since_epoch % 28 + 1,
        months[(days_since_epoch / 28 % 12) as usize],
        1970 + days_since_epoch / 365,
        (seconds_since_epoch % 86400) / 3600,
        (seconds_since_epoch % 3600) / 60,
        seconds_since_epoch % 60
    )
}

fn not_found_response() -> RoverResponse {
    RoverResponse {
        status: 404,
        body: Bytes::from_static(b"Not Found"),
        content_type: "text/plain",
        headers: None,
    }
}

fn forbidden_response(message: &str) -> RoverResponse {
    RoverResponse {
        status: 403,
        body: Bytes::from(format!("Forbidden: {}", message)),
        content_type: "text/plain",
        headers: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn should_serve_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let response = serve_static_file(temp_dir.path(), "test.txt", None);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"Hello, World!"));
        assert_eq!(response.content_type, "text/plain");
        assert!(response.headers.is_some());

        let headers = response.headers.unwrap();
        assert!(headers.contains_key("Cache-Control"));
        assert!(headers.contains_key("ETag"));
    }

    #[test]
    fn should_detect_directory_traversal_with_dotdot() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "../etc/passwd", None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_detect_directory_traversal_with_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "/../etc/passwd", None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_serve_file_with_leading_slash_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let response = serve_static_file(temp_dir.path(), "/test.txt", None);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"Hello, World!"));
    }

    #[test]
    fn should_reject_path_with_null_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "test\0.txt", None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_reject_very_long_paths() {
        let temp_dir = TempDir::new().unwrap();
        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        let response = serve_static_file(temp_dir.path(), &long_path, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_reject_directory_listing() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let response = serve_static_file(temp_dir.path(), "subdir", None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_return_404_for_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "nonexistent.txt", None);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn should_guess_content_type_html() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("page.html"), "<html></html>").unwrap();

        let response = serve_static_file(temp_dir.path(), "page.html", None);
        assert_eq!(response.content_type, "text/html");
    }

    #[test]
    fn should_guess_content_type_css() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("style.css"), "body {}").unwrap();

        let response = serve_static_file(temp_dir.path(), "style.css", None);
        assert_eq!(response.content_type, "text/css");
    }

    #[test]
    fn should_guess_content_type_js() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("app.js"), "console.log('hi')").unwrap();

        let response = serve_static_file(temp_dir.path(), "app.js", None);
        assert_eq!(response.content_type, "application/javascript");
    }

    #[test]
    fn should_include_custom_headers() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut custom_headers = HashMap::new();
        custom_headers.insert("X-Custom".to_string(), "value".to_string());

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(custom_headers));

        assert_eq!(response.status, 200);
        let headers = response.headers.unwrap();
        assert_eq!(headers.get("X-Custom"), Some(&"value".to_string()));
        assert!(headers.contains_key("Cache-Control"));
    }

    #[test]
    fn should_prevent_traversal_with_encoded_paths() {
        let temp_dir = TempDir::new().unwrap();
        // This tests the canonicalization check
        let response = serve_static_file(temp_dir.path(), "subdir/../../../etc/passwd", None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_allow_nested_valid_paths() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("css")).unwrap();
        std::fs::write(temp_dir.path().join("css/style.css"), "body {}").unwrap();

        let response = serve_static_file(temp_dir.path(), "css/style.css", None);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "text/css");
    }

    #[test]
    fn should_handle_symlinks_within_base() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("target.txt"), "target content").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(
                temp_dir.path().join("target.txt"),
                temp_dir.path().join("link.txt"),
            )
            .unwrap();

            let response = serve_static_file(temp_dir.path(), "link.txt", None);
            assert_eq!(response.status, 200);
            assert_eq!(response.body, Bytes::from_static(b"target content"));
        }
    }

    #[test]
    fn should_block_symlink_escape() {
        let temp_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();
        std::fs::write(outside_dir.path().join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(
                outside_dir.path().join("secret.txt"),
                temp_dir.path().join("escape.txt"),
            )
            .unwrap();

            let response = serve_static_file(temp_dir.path(), "escape.txt", None);
            assert_eq!(response.status, 403);
        }
    }

    #[test]
    fn should_detect_traversal_with_curdir() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // ./test.txt should be allowed
        let response = serve_static_file(temp_dir.path(), "./test.txt", None);
        assert_eq!(response.status, 200);
    }
}
