use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::{Bytes, RoverResponse};
use rover_types::emit_file_access_denied;

/// Maximum allowed path length to prevent abuse
const MAX_PATH_LENGTH: usize = 4096;

/// Cache headers by file class
const CACHE_CONTROL_DOCUMENT: &str = "no-cache";
const CACHE_CONTROL_ASSET: &str = "public, max-age=31536000, immutable";
const CACHE_CONTROL_DEFAULT: &str = "public, max-age=86400";

/// Serve a static file with traversal protection and cache headers
///
/// # Arguments
/// * `base_path` - The base directory to serve files from
/// * `requested_path` - The path requested by the client (e.g., "/css/style.css")
/// * `request_headers` - Optional request headers for conditional requests (If-None-Match, If-Modified-Since)
/// * `custom_headers` - Optional additional headers to include
///
/// # Returns
/// A `RoverResponse` with the file content or a 404/403/304 error
pub fn serve_static_file(
    base_path: &Path,
    requested_path: &str,
    request_headers: Option<&HashMap<String, String>>,
    custom_headers: Option<HashMap<String, String>>,
) -> RoverResponse {
    // Validate and sanitize the requested path
    let sanitized = match sanitize_path(base_path, requested_path) {
        Ok(path) => path,
        Err(e) => {
            if e == "Not found" {
                return not_found_response();
            }
            emit_file_access_denied(requested_path, &e);
            return forbidden_response(&e);
        }
    };

    // Check if the path is a directory (don't serve directories)
    // Scope: Directory index/listing support is explicitly out of scope for this release.
    // Requests to directory paths return 403 Forbidden to prevent information leakage.
    if sanitized.is_dir() {
        emit_file_access_denied(requested_path, "Directory listing not allowed");
        return forbidden_response("Directory listing not allowed");
    }

    // Read the file
    match std::fs::read(&sanitized) {
        Ok(content) => {
            let content_type = guess_content_type(&sanitized);
            let mut headers = custom_headers.unwrap_or_default();

            // Add cache headers
            add_cache_headers(&mut headers, &sanitized, &content);

            // Check conditional request headers for 304 response
            if let Some(req_headers) = request_headers
                && is_not_modified(&headers, req_headers)
            {
                return not_modified_response(headers);
            }

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
                let error_msg = format!("Access denied: {}", e);
                emit_file_access_denied(requested_path, &error_msg);
                forbidden_response(&error_msg)
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

    if requested_path.contains('%') && !is_valid_percent_encoding(requested_path) {
        return Err("Invalid path".to_string());
    }

    if has_traversal_attempt(requested_path) {
        return Err("Directory traversal not allowed".to_string());
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

fn has_traversal_attempt(path: &str) -> bool {
    if has_parent_dir_component(path) {
        return true;
    }

    if !path.contains('%') {
        return false;
    }

    let mut decoded = path.to_string();
    for _ in 0..2 {
        let next = match urlencoding::decode(&decoded) {
            Ok(value) => value.into_owned(),
            Err(_) => return false,
        };

        if next == decoded {
            return false;
        }

        if has_parent_dir_component(&next) {
            return true;
        }

        if !next.contains('%') {
            return false;
        }

        if !is_valid_percent_encoding(&next) {
            return false;
        }

        decoded = next;
    }

    false
}

fn has_parent_dir_component(path: &str) -> bool {
    Path::new(path.trim_start_matches('/'))
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn is_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }

        if i + 2 >= bytes.len() {
            return false;
        }

        let hi = bytes[i + 1];
        let lo = bytes[i + 2];
        if !hi.is_ascii_hexdigit() || !lo.is_ascii_hexdigit() {
            return false;
        }

        i += 3;
    }

    true
}

/// Guess the content type based on file extension
fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") | Some("mjs") | Some("cjs") => "application/javascript",
        Some("json") => "application/json",
        Some("map") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("xml") => "application/xml",
        Some("wasm") => "application/wasm",
        Some("webmanifest") => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

fn default_cache_control(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => CACHE_CONTROL_DOCUMENT,
        Some("css") | Some("js") | Some("mjs") | Some("cjs") | Some("map") | Some("png")
        | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("avif") | Some("svg")
        | Some("ico") | Some("woff") | Some("woff2") | Some("ttf") | Some("otf") | Some("eot")
        | Some("wasm") => CACHE_CONTROL_ASSET,
        Some("json") | Some("xml") | Some("webmanifest") => CACHE_CONTROL_DOCUMENT,
        _ => CACHE_CONTROL_DEFAULT,
    }
}

/// Add cache-related headers to the response
fn add_cache_headers(headers: &mut HashMap<String, String>, path: &Path, content: &[u8]) {
    // Add Cache-Control header
    headers
        .entry("Cache-Control".to_string())
        .or_insert_with(|| default_cache_control(path).to_string());

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

fn not_modified_response(headers: HashMap<String, String>) -> RoverResponse {
    RoverResponse {
        status: 304,
        body: Bytes::new(),
        content_type: "text/plain",
        headers: Some(headers),
    }
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

/// Strip encoding suffix from ETag (e.g., `"abc-gzip"` -> `"abc"`)
pub fn strip_etag_encoding_suffix(etag: &str) -> &str {
    let trimmed = etag.trim();
    if !trimmed.starts_with('"') || !trimmed.ends_with('"') {
        return trimmed;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let encodings = ["-gzip", "-deflate", "-br", "-xz"];
    for enc in encodings {
        if let Some(base) = inner.strip_suffix(enc) {
            return base;
        }
    }
    inner
}

/// Check if resource is not modified based on conditional headers
pub fn is_not_modified(
    response_headers: &HashMap<String, String>,
    request_headers: &HashMap<String, String>,
) -> bool {
    // Check If-None-Match header (ETag comparison)
    if let Some(if_none_match) = request_headers.get("If-None-Match") {
        if let Some(etag) = response_headers.get("ETag") {
            // Support both exact match and * wildcard
            if if_none_match == "*" {
                return true;
            }
            // Get base ETag without encoding suffix
            let base_etag = strip_etag_encoding_suffix(etag);
            // Handle quoted ETag values (may have multiple ETags separated by commas)
            for client_etag in if_none_match.split(',').map(|s| s.trim()) {
                if client_etag == etag {
                    return true;
                }
                // Also check if client's ETag matches after stripping encoding suffix
                // This handles cached compressed versions when content hasn't changed
                let client_base = strip_etag_encoding_suffix(client_etag);
                if client_base == base_etag {
                    return true;
                }
            }
        }
        // If If-None-Match is present but doesn't match, don't check Last-Modified
        return false;
    }

    // Check If-Modified-Since header (Last-Modified comparison)
    if let Some(if_modified_since) = request_headers.get("If-Modified-Since")
        && let Some(last_modified) = response_headers.get("Last-Modified")
    {
        // Parse both dates and compare
        // For simplicity, we do string comparison (dates should be in same format)
        // In production, you'd parse and compare timestamps
        return if_modified_since == last_modified;
    }

    false
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

        let response = serve_static_file(temp_dir.path(), "test.txt", None, None);

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
        let response = serve_static_file(temp_dir.path(), "../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_detect_directory_traversal_with_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "/../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_detect_directory_traversal_with_encoded_dotdot() {
        let temp_dir = TempDir::new().unwrap();

        let response = serve_static_file(temp_dir.path(), "%2e%2e/etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_detect_directory_traversal_with_double_encoded_dotdot() {
        let temp_dir = TempDir::new().unwrap();

        let response = serve_static_file(temp_dir.path(), "%252e%252e/etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_serve_file_with_leading_slash_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let response = serve_static_file(temp_dir.path(), "/test.txt", None, None);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, Bytes::from_static(b"Hello, World!"));
    }

    #[test]
    fn should_reject_path_with_null_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "test\0.txt", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_reject_very_long_paths() {
        let temp_dir = TempDir::new().unwrap();
        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        let response = serve_static_file(temp_dir.path(), &long_path, None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_reject_directory_listing() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let response = serve_static_file(temp_dir.path(), "subdir", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_return_404_for_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "nonexistent.txt", None, None);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn should_guess_content_type_html() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("page.html"), "<html></html>").unwrap();

        let response = serve_static_file(temp_dir.path(), "page.html", None, None);
        assert_eq!(response.content_type, "text/html");
    }

    #[test]
    fn should_guess_content_type_css() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("style.css"), "body {}").unwrap();

        let response = serve_static_file(temp_dir.path(), "style.css", None, None);
        assert_eq!(response.content_type, "text/css");
    }

    #[test]
    fn should_guess_content_type_js() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("app.js"), "console.log('hi')").unwrap();

        let response = serve_static_file(temp_dir.path(), "app.js", None, None);
        assert_eq!(response.content_type, "application/javascript");
    }

    #[test]
    fn should_guess_content_type_webp() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("image.webp"), "webp").unwrap();

        let response = serve_static_file(temp_dir.path(), "image.webp", None, None);
        assert_eq!(response.content_type, "image/webp");
    }

    #[test]
    fn should_guess_content_type_mjs() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("module.mjs"), "export const x = 1;").unwrap();

        let response = serve_static_file(temp_dir.path(), "module.mjs", None, None);
        assert_eq!(response.content_type, "application/javascript");
    }

    #[test]
    fn should_guess_content_type_sourcemap() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("app.js.map"), "{}").unwrap();

        let response = serve_static_file(temp_dir.path(), "app.js.map", None, None);
        assert_eq!(response.content_type, "application/json");
    }

    #[test]
    fn should_default_html_cache_control_to_no_cache() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("index.html"), "<html></html>").unwrap();

        let response = serve_static_file(temp_dir.path(), "index.html", None, None);
        let headers = response.headers.unwrap();
        assert_eq!(headers.get("Cache-Control"), Some(&"no-cache".to_string()));
    }

    #[test]
    fn should_default_static_asset_cache_control_to_immutable() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("app.js"), "console.log('hi')").unwrap();

        let response = serve_static_file(temp_dir.path(), "app.js", None, None);
        let headers = response.headers.unwrap();
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"public, max-age=31536000, immutable".to_string())
        );
    }

    #[test]
    fn should_keep_default_cache_for_unrecognized_extensions() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("blob.bin"), "x").unwrap();

        let response = serve_static_file(temp_dir.path(), "blob.bin", None, None);
        let headers = response.headers.unwrap();
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"public, max-age=86400".to_string())
        );
    }

    #[test]
    fn should_include_custom_headers() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut custom_headers = HashMap::new();
        custom_headers.insert("X-Custom".to_string(), "value".to_string());

        let response = serve_static_file(temp_dir.path(), "test.txt", None, Some(custom_headers));

        assert_eq!(response.status, 200);
        let headers = response.headers.unwrap();
        assert_eq!(headers.get("X-Custom"), Some(&"value".to_string()));
        assert!(headers.contains_key("Cache-Control"));
    }

    #[test]
    fn should_prevent_traversal_with_encoded_paths() {
        let temp_dir = TempDir::new().unwrap();
        // This tests the canonicalization check
        let response = serve_static_file(temp_dir.path(), "subdir/../../../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_allow_nested_valid_paths() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("css")).unwrap();
        std::fs::write(temp_dir.path().join("css/style.css"), "body {}").unwrap();

        let response = serve_static_file(temp_dir.path(), "css/style.css", None, None);
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

            let response = serve_static_file(temp_dir.path(), "link.txt", None, None);
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

            let response = serve_static_file(temp_dir.path(), "escape.txt", None, None);
            assert_eq!(response.status, 403);
        }
    }

    #[test]
    fn should_detect_traversal_with_curdir() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // ./test.txt should be allowed
        let response = serve_static_file(temp_dir.path(), "./test.txt", None, None);
        assert_eq!(response.status, 200);
    }

    #[test]
    fn should_return_304_when_etag_matches() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // First request to get ETag
        let first_response = serve_static_file(temp_dir.path(), "test.txt", None, None);
        assert_eq!(first_response.status, 200);
        let etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();

        // Second request with If-None-Match
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), etag);

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);
        assert!(response.body.is_empty());
    }

    #[test]
    fn should_return_200_when_etag_does_not_match() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), "\"wrong-etag\"".to_string());

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 200);
        assert!(!response.body.is_empty());
    }

    #[test]
    fn should_return_304_when_wildcard_etag() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), "*".to_string());

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);
    }

    #[test]
    fn should_return_200_when_file_modified_since_last_modified() {
        use std::thread;
        use std::time::{Duration, SystemTime};

        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // Wait a bit to ensure modification time differs
        thread::sleep(Duration::from_millis(100));

        // Get file's last modified time
        let metadata = std::fs::metadata(temp_dir.path().join("test.txt")).unwrap();
        let modified = metadata
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        // Request with an older date
        let older_date = format_http_date(modified.as_secs() - 3600); // 1 hour before
        let mut headers = HashMap::new();
        headers.insert("If-Modified-Since".to_string(), older_date);

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 200);
    }

    #[test]
    fn should_prioritize_if_none_match_over_if_modified_since() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // Request with both headers, but wrong ETag
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), "\"wrong-etag\"".to_string());
        headers.insert("If-Modified-Since".to_string(), format_http_date(0)); // Very old date

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 200); // ETag mismatch overrides If-Modified-Since
    }

    #[test]
    fn should_return_304_headers_in_response() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // Get ETag and Last-Modified from initial request
        let first_response = serve_static_file(temp_dir.path(), "test.txt", None, None);
        let etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();
        let last_modified = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("Last-Modified")
            .cloned()
            .unwrap();

        // Conditional request
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), etag.clone());

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);

        // 304 response should still include cache headers
        let response_headers = response.headers.unwrap();
        assert!(response_headers.contains_key("ETag"));
        assert!(response_headers.contains_key("Last-Modified"));
        assert!(response_headers.contains_key("Cache-Control"));
    }

    #[test]
    fn should_allow_multiple_etags_in_if_none_match() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let first_response = serve_static_file(temp_dir.path(), "test.txt", None, None);
        let etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();

        // Multiple ETags, one matches
        let mut headers = HashMap::new();
        headers.insert(
            "If-None-Match".to_string(),
            format!("\"etag1\", {}, \"etag2\"", etag),
        );

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);
    }

    #[test]
    fn should_return_304_when_etag_matches_with_gzip_suffix() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let first_response = serve_static_file(temp_dir.path(), "test.txt", None, None);
        let base_etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();

        // Client has cached gzip version with encoding-suffixed ETag
        let gzip_etag = format!("{}-gzip\"", &base_etag[..base_etag.len() - 1]);
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), gzip_etag.clone());

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);
    }

    #[test]
    fn should_return_304_when_etag_matches_with_deflate_suffix() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let first_response = serve_static_file(temp_dir.path(), "test.txt", None, None);
        let base_etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();

        // Client has cached deflate version with encoding-suffixed ETag
        let deflate_etag = format!("{}-deflate\"", &base_etag[..base_etag.len() - 1]);
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), deflate_etag);

        let response = serve_static_file(temp_dir.path(), "test.txt", Some(&headers), None);
        assert_eq!(response.status, 304);
    }

    #[test]
    fn should_strip_etag_encoding_suffix_correctly() {
        assert_eq!(strip_etag_encoding_suffix("\"abc-gzip\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-deflate\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-br\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc\""), "abc");
        assert_eq!(strip_etag_encoding_suffix("abc"), "abc");
        assert_eq!(strip_etag_encoding_suffix("\"abc-xz\""), "abc");
    }

    #[test]
    fn should_override_default_cache_with_custom_cache_control() {
        let temp_dir = TempDir::new().unwrap();
        // HTML files default to "no-cache"
        std::fs::write(temp_dir.path().join("index.html"), "<html></html>").unwrap();

        let mut custom_headers = HashMap::new();
        custom_headers.insert(
            "Cache-Control".to_string(),
            "public, max-age=3600".to_string(),
        );

        let response = serve_static_file(temp_dir.path(), "index.html", None, Some(custom_headers));
        let headers = response.headers.unwrap();
        assert_eq!(
            headers.get("Cache-Control"),
            Some(&"public, max-age=3600".to_string())
        );
    }

    #[test]
    fn should_override_asset_default_with_custom_cache_control() {
        let temp_dir = TempDir::new().unwrap();
        // JS files default to "public, max-age=31536000, immutable"
        std::fs::write(temp_dir.path().join("app.js"), "console.log('test');").unwrap();

        let mut custom_headers = HashMap::new();
        custom_headers.insert("Cache-Control".to_string(), "no-store".to_string());

        let response = serve_static_file(temp_dir.path(), "app.js", None, Some(custom_headers));
        let headers = response.headers.unwrap();
        assert_eq!(headers.get("Cache-Control"), Some(&"no-store".to_string()));
    }

    #[test]
    fn should_include_custom_cache_control_in_304_response() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let mut custom_headers = HashMap::new();
        custom_headers.insert(
            "Cache-Control".to_string(),
            "private, max-age=120".to_string(),
        );

        let first_response = serve_static_file(
            temp_dir.path(),
            "test.txt",
            None,
            Some(custom_headers.clone()),
        );
        let etag = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("ETag")
            .cloned()
            .unwrap();
        let cache_control = first_response
            .headers
            .as_ref()
            .unwrap()
            .get("Cache-Control")
            .cloned()
            .unwrap();
        assert_eq!(cache_control, "private, max-age=120");

        // Conditional request with matching ETag
        let mut headers = HashMap::new();
        headers.insert("If-None-Match".to_string(), etag);

        let response = serve_static_file(
            temp_dir.path(),
            "test.txt",
            Some(&headers),
            Some(custom_headers),
        );
        assert_eq!(response.status, 304);

        // 304 response should preserve the custom cache header
        let response_headers = response.headers.unwrap();
        assert_eq!(
            response_headers.get("Cache-Control"),
            Some(&"private, max-age=120".to_string())
        );
    }

    #[test]
    fn should_reject_directory_index_requests_with_403() {
        // Scope: Directory index support is explicitly out of scope for this release.
        // This test verifies that directory paths return 403 Forbidden.
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("public")).unwrap();
        std::fs::write(temp_dir.path().join("public/file.txt"), "content").unwrap();

        // Request to directory should return 403, not a listing
        let response = serve_static_file(temp_dir.path(), "public", None, None);
        assert_eq!(response.status, 403);
        assert!(String::from_utf8_lossy(&response.body).contains("Directory listing not allowed"));
    }
}
