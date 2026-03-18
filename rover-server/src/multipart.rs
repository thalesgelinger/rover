use bytes::Bytes;
use std::collections::HashMap;

/// Default maximum number of parts in a multipart request
pub const DEFAULT_MAX_PARTS: usize = 100;

/// Default maximum file size (10MB)
pub const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Default maximum total size of all files (50MB)
pub const DEFAULT_MAX_TOTAL_FILE_SIZE: usize = 50 * 1024 * 1024;

/// Error types for multipart parsing
#[derive(Debug, Clone, PartialEq)]
pub enum MultipartError {
    MissingBoundary,
    InvalidBoundary,
    TooManyParts {
        max: usize,
        found: usize,
    },
    FileTooLarge {
        field_name: String,
        max: usize,
        found: usize,
    },
    TotalSizeExceeded {
        max: usize,
        found: usize,
    },
    InvalidContentType {
        field_name: String,
        allowed: Vec<String>,
        found: String,
    },
    MalformedPart(String),
    IoError(String),
}

impl std::fmt::Display for MultipartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultipartError::MissingBoundary => write!(f, "Missing boundary in Content-Type header"),
            MultipartError::InvalidBoundary => write!(f, "Invalid boundary format"),
            MultipartError::TooManyParts { max, found } => {
                write!(f, "Too many parts: {} exceeds maximum {}", found, max)
            }
            MultipartError::FileTooLarge {
                field_name,
                max,
                found,
            } => {
                write!(
                    f,
                    "File '{}' too large: {} bytes exceeds maximum {} bytes",
                    field_name, found, max
                )
            }
            MultipartError::TotalSizeExceeded { max, found } => {
                write!(
                    f,
                    "Total file size {} bytes exceeds maximum {} bytes",
                    found, max
                )
            }
            MultipartError::InvalidContentType {
                field_name,
                allowed,
                found,
            } => {
                write!(
                    f,
                    "File '{}' has invalid content type '{}'. Allowed: {}",
                    field_name,
                    found,
                    allowed.join(", ")
                )
            }
            MultipartError::MalformedPart(msg) => write!(f, "Malformed part: {}", msg),
            MultipartError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for MultipartError {}

/// Configuration for multipart parsing limits
#[derive(Debug, Clone)]
pub struct MultipartLimits {
    /// Maximum number of parts (files + fields)
    pub max_parts: usize,
    /// Maximum size per file in bytes
    pub max_file_size: usize,
    /// Maximum total size of all files in bytes
    pub max_total_file_size: usize,
    /// Allowed content types (empty = allow all)
    pub allowed_content_types: Vec<String>,
}

impl Default for MultipartLimits {
    fn default() -> Self {
        Self {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: DEFAULT_MAX_TOTAL_FILE_SIZE,
            allowed_content_types: vec![],
        }
    }
}

impl MultipartLimits {
    /// Create limits with custom values
    pub fn new(max_parts: usize, max_file_size: usize, max_total_file_size: usize) -> Self {
        Self {
            max_parts,
            max_file_size,
            max_total_file_size,
            allowed_content_types: vec![],
        }
    }

    /// Add allowed content type
    pub fn allow_content_type(mut self, content_type: &str) -> Self {
        self.allowed_content_types.push(content_type.to_string());
        self
    }

    /// Check if content type is allowed
    fn is_content_type_allowed(&self, content_type: &str) -> bool {
        if self.allowed_content_types.is_empty() {
            return true;
        }
        let normalized = content_type.to_lowercase();
        self.allowed_content_types
            .iter()
            .any(|allowed| normalized.contains(&allowed.to_lowercase()))
    }
}

/// Represents a single part in a multipart request
#[derive(Debug, Clone)]
pub struct Part {
    /// Field name from Content-Disposition header
    pub name: String,
    /// Original filename (for file uploads)
    pub filename: Option<String>,
    /// Content-Type header value
    pub content_type: Option<String>,
    /// Part data
    pub data: Bytes,
    /// Whether this part is a file (has filename)
    pub is_file: bool,
}

impl Part {
    /// Get data as string (for form fields)
    pub fn as_string(&self) -> Option<String> {
        String::from_utf8(self.data.to_vec()).ok()
    }

    /// Get file size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Parse Content-Type header to extract boundary
fn extract_boundary(content_type: &str) -> Result<String, MultipartError> {
    let mut boundary = None;

    for param in content_type.split(';').skip(1) {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };

        if !key.trim().eq_ignore_ascii_case("boundary") {
            continue;
        }

        let value = value.trim();
        let parsed = if let Some(quoted) = value.strip_prefix('"') {
            quoted
                .strip_suffix('"')
                .ok_or(MultipartError::InvalidBoundary)?
                .trim()
                .to_string()
        } else {
            value.to_string()
        };

        boundary = Some(parsed);
        break;
    }

    let boundary = boundary.ok_or(MultipartError::MissingBoundary)?;

    if boundary.is_empty() {
        return Err(MultipartError::InvalidBoundary);
    }

    Ok(boundary)
}

/// Parse Content-Disposition header to extract field name and filename
fn parse_content_disposition(header: &str) -> Result<(String, Option<String>), MultipartError> {
    let mut name = None;
    let mut filename = None;

    // First, skip the disposition type (e.g., "form-data")
    let mut chars = header.chars().peekable();

    // Skip until we hit a semicolon or end
    while let Some(&ch) = chars.peek() {
        if ch == ';' {
            chars.next(); // consume semicolon
            break;
        }
        chars.next();
    }

    // Now parse parameters
    while chars.peek().is_some() {
        // Skip whitespace and semicolons
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() || ch == ';' {
                chars.next();
            } else {
                break;
            }
        }

        // Read key
        let mut key = String::new();
        while let Some(&ch) = chars.peek() {
            if ch == '=' || ch.is_whitespace() {
                break;
            }
            key.push(ch);
            chars.next();
        }

        if key.is_empty() {
            break;
        }

        // Skip whitespace
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        // Expect '='
        if chars.next() != Some('=') {
            continue;
        }

        // Skip whitespace
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        // Read value (quoted or unquoted)
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next(); // skip opening quote
            for ch in chars.by_ref() {
                if ch == '"' {
                    break;
                }
                value.push(ch);
            }
        } else {
            while let Some(&ch) = chars.peek() {
                if ch == ';' || ch.is_whitespace() {
                    break;
                }
                value.push(ch);
                chars.next();
            }
        }

        match key.as_str() {
            "name" => name = Some(value),
            "filename" => filename = Some(value),
            _ => {}
        }
    }

    name.ok_or_else(|| {
        MultipartError::MalformedPart("Missing 'name' in Content-Disposition header".to_string())
    })
    .map(|n| (n, filename))
}

/// Parse a single part from the multipart body
fn parse_part(
    part_data: &[u8],
    limits: &MultipartLimits,
    current_total_size: &mut usize,
) -> Result<Part, MultipartError> {
    // Find the empty line that separates headers from body
    let header_end = part_data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| {
            MultipartError::MalformedPart("Missing header-body separator".to_string())
        })?;

    let headers_bytes = &part_data[..header_end];
    let body_start = header_end + 4;
    let body = &part_data[body_start..];

    // Parse headers
    let headers_str = String::from_utf8_lossy(headers_bytes);
    let mut name = None;
    let mut filename = None;
    let mut content_type = None;

    for line in headers_str.lines() {
        if line.to_lowercase().starts_with("content-disposition:") {
            let value = line[20..].trim();
            let (n, f) = parse_content_disposition(value)?;
            name = Some(n);
            filename = f;
        } else if line.to_lowercase().starts_with("content-type:") {
            content_type = Some(line[13..].trim().to_string());
        }
    }

    let name = name.ok_or_else(|| {
        MultipartError::MalformedPart("Missing Content-Disposition header".to_string())
    })?;

    let is_file = filename.is_some();

    // Validate content type for files
    if is_file {
        if let Some(ref ct) = content_type {
            if !limits.is_content_type_allowed(ct) {
                return Err(MultipartError::InvalidContentType {
                    field_name: name.clone(),
                    allowed: limits.allowed_content_types.clone(),
                    found: ct.clone(),
                });
            }
        }
    }

    // Check file size limit
    if is_file && body.len() > limits.max_file_size {
        return Err(MultipartError::FileTooLarge {
            field_name: name.clone(),
            max: limits.max_file_size,
            found: body.len(),
        });
    }

    // Check total size limit
    if is_file {
        *current_total_size += body.len();
        if *current_total_size > limits.max_total_file_size {
            return Err(MultipartError::TotalSizeExceeded {
                max: limits.max_total_file_size,
                found: *current_total_size,
            });
        }
    }

    Ok(Part {
        name,
        filename,
        content_type,
        data: Bytes::copy_from_slice(body),
        is_file,
    })
}

/// Parse multipart/form-data body
pub fn parse_multipart(
    body: &[u8],
    content_type: &str,
    limits: &MultipartLimits,
) -> Result<Vec<Part>, MultipartError> {
    let boundary = extract_boundary(content_type)?;
    let boundary_marker = format!("--{}", boundary);
    let boundary_end_marker = format!("--{}--", boundary);

    let mut parts = Vec::new();
    let mut current_total_size = 0usize;

    // Find the first boundary
    let body_str = String::from_utf8_lossy(body);

    // Skip preamble (content before first boundary)
    let mut pos = if let Some(first_boundary) = body_str.find(&boundary_marker) {
        first_boundary + boundary_marker.len()
    } else {
        return Err(MultipartError::MalformedPart(
            "No boundary found in body".to_string(),
        ));
    };

    // Skip trailing CRLF or LF after first boundary
    if body.get(pos..pos + 2) == Some(b"\r\n") {
        pos += 2;
    } else if body.get(pos..pos + 1) == Some(b"\n") {
        pos += 1;
    }

    loop {
        // Find next boundary
        let remaining = &body_str[pos..];
        let next_boundary_pos = remaining.find(&boundary_marker);

        if next_boundary_pos.is_none() {
            // Check for final boundary
            if let Some(end_pos) = remaining.find(&boundary_end_marker) {
                if end_pos == 0 || remaining[..end_pos].trim().is_empty() {
                    break; // Normal end of multipart
                }

                // Extract last part before final boundary
                let part_data = &body[pos..pos + end_pos];
                // Remove trailing CRLF or LF
                let part_data = if part_data.ends_with(b"\r\n") {
                    &part_data[..part_data.len() - 2]
                } else if part_data.ends_with(b"\n") {
                    &part_data[..part_data.len() - 1]
                } else {
                    part_data
                };

                if !part_data.is_empty() {
                    if parts.len() >= limits.max_parts {
                        return Err(MultipartError::TooManyParts {
                            max: limits.max_parts,
                            found: parts.len() + 1,
                        });
                    }

                    let part = parse_part(part_data, limits, &mut current_total_size)?;
                    parts.push(part);
                }
            }
            break;
        }

        let next_boundary_pos = next_boundary_pos.unwrap();

        // Extract part data
        let part_data = &body[pos..pos + next_boundary_pos];

        // Remove trailing CRLF or LF
        let part_data = if part_data.ends_with(b"\r\n") {
            &part_data[..part_data.len() - 2]
        } else if part_data.ends_with(b"\n") {
            &part_data[..part_data.len() - 1]
        } else {
            part_data
        };

        if !part_data.is_empty() {
            if parts.len() >= limits.max_parts {
                return Err(MultipartError::TooManyParts {
                    max: limits.max_parts,
                    found: parts.len() + 1,
                });
            }

            let part = parse_part(part_data, limits, &mut current_total_size)?;
            parts.push(part);
        }

        // Move past this boundary
        pos += next_boundary_pos + boundary_marker.len();

        // Skip trailing CRLF or LF after boundary
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        } else if body.get(pos..pos + 1) == Some(b"\n") {
            pos += 1;
        }

        // Check if this was the final boundary
        if body_str[pos..].starts_with("--") {
            break;
        }
    }

    Ok(parts)
}

/// Result of multipart parsing with fields and files separated
#[derive(Debug, Clone)]
pub struct MultipartData {
    /// Form fields (non-file parts)
    pub fields: HashMap<String, String>,
    /// Uploaded files
    pub files: HashMap<String, Vec<FileUpload>>,
}

/// Represents an uploaded file
#[derive(Debug, Clone)]
pub struct FileUpload {
    /// Original filename
    pub filename: String,
    /// Content type
    pub content_type: Option<String>,
    /// File data
    pub data: Bytes,
    /// File size in bytes
    pub size: usize,
}

impl MultipartData {
    /// Create empty MultipartData
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            files: HashMap::new(),
        }
    }

    /// Get a single field value
    pub fn get_field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(|s| s.as_str())
    }

    /// Get all files for a field
    pub fn get_files(&self, name: &str) -> Option<&Vec<FileUpload>> {
        self.files.get(name)
    }

    /// Get a single file (first one) for a field
    pub fn get_file(&self, name: &str) -> Option<&FileUpload> {
        self.files.get(name).and_then(|files| files.first())
    }
}

impl Default for MultipartData {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse multipart body and separate fields from files
pub fn parse_multipart_data(
    body: &[u8],
    content_type: &str,
    limits: &MultipartLimits,
) -> Result<MultipartData, MultipartError> {
    let parts = parse_multipart(body, content_type, limits)?;
    let mut data = MultipartData::new();

    for part in parts {
        if part.is_file {
            let upload = FileUpload {
                filename: part.filename.unwrap_or_default(),
                content_type: part.content_type,
                size: part.data.len(),
                data: part.data,
            };
            data.files.entry(part.name).or_default().push(upload);
        } else {
            // Try to convert to string for form fields
            if let Ok(text) = String::from_utf8(part.data.to_vec()) {
                data.fields.insert(part.name, text);
            }
        }
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_extract_boundary_from_content_type() {
        let ct = "multipart/form-data; boundary=----WebKitFormBoundaryABC123";
        let boundary = extract_boundary(ct).unwrap();
        assert_eq!(boundary, "----WebKitFormBoundaryABC123");
    }

    #[test]
    fn should_extract_quoted_boundary() {
        let ct = r#"multipart/form-data; boundary="----WebKitFormBoundaryXYZ789""#;
        let boundary = extract_boundary(ct).unwrap();
        assert_eq!(boundary, "----WebKitFormBoundaryXYZ789");
    }

    #[test]
    fn should_extract_boundary_with_whitespace_around_equals() {
        let ct = "multipart/form-data; boundary = ----WebKitFormBoundaryABC123";
        let boundary = extract_boundary(ct).unwrap();
        assert_eq!(boundary, "----WebKitFormBoundaryABC123");
    }

    #[test]
    fn should_extract_boundary_when_not_first_parameter() {
        let ct = "multipart/form-data; charset=utf-8; boundary=----WebKitFormBoundaryABC123";
        let boundary = extract_boundary(ct).unwrap();
        assert_eq!(boundary, "----WebKitFormBoundaryABC123");
    }

    #[test]
    fn should_error_on_missing_boundary() {
        let ct = "multipart/form-data";
        let result = extract_boundary(ct);
        assert!(matches!(result, Err(MultipartError::MissingBoundary)));
    }

    #[test]
    fn should_parse_content_disposition() {
        let header = r#"form-data; name="field1""#;
        let (name, filename) = parse_content_disposition(header).unwrap();
        assert_eq!(name, "field1");
        assert_eq!(filename, None);
    }

    #[test]
    fn should_parse_content_disposition_with_filename() {
        let header = r#"form-data; name="file"; filename="test.txt""#;
        let (name, filename) = parse_content_disposition(header).unwrap();
        assert_eq!(name, "file");
        assert_eq!(filename, Some("test.txt".to_string()));
    }

    #[test]
    fn should_parse_simple_multipart() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
            value1\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field2\"\r\n\r\n\
            value2\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits::default();

        let parts = parse_multipart(body, ct, &limits).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "field1");
        assert_eq!(parts[0].as_string(), Some("value1".to_string()));
        assert_eq!(parts[1].name, "field2");
        assert_eq!(parts[1].as_string(), Some("value2".to_string()));
    }

    #[test]
    fn should_parse_file_upload() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            Hello World\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits::default();

        let parts = parse_multipart(body, ct, &limits).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "file");
        assert_eq!(parts[0].filename, Some("test.txt".to_string()));
        assert_eq!(parts[0].content_type, Some("text/plain".to_string()));
        assert!(parts[0].is_file);
        assert_eq!(parts[0].as_string(), Some("Hello World".to_string()));
    }

    #[test]
    fn should_enforce_max_parts_limit() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
            value1\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field2\"\r\n\r\n\
            value2\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field3\"\r\n\r\n\
            value3\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: 2,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: DEFAULT_MAX_TOTAL_FILE_SIZE,
            allowed_content_types: vec![],
        };

        let result = parse_multipart(body, ct, &limits);
        assert!(matches!(
            result,
            Err(MultipartError::TooManyParts { max: 2, found: 3 })
        ));
    }

    #[test]
    fn should_enforce_file_size_limit() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"large.bin\"\r\n\
            Content-Type: application/octet-stream\r\n\r\n\
            This is more than 10 bytes of content\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: 10,
            max_total_file_size: DEFAULT_MAX_TOTAL_FILE_SIZE,
            allowed_content_types: vec![],
        };

        let result = parse_multipart(body, ct, &limits);
        assert!(matches!(
            result,
            Err(MultipartError::FileTooLarge {
                field_name,
                max: 10,
                ..
            }) if field_name == "file"
        ));
    }

    #[test]
    fn should_enforce_content_type_restriction() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"test.exe\"\r\n\
            Content-Type: application/x-msdownload\r\n\r\n\
            MZ\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: DEFAULT_MAX_TOTAL_FILE_SIZE,
            allowed_content_types: vec!["image/".to_string(), "text/".to_string()],
        };

        let result = parse_multipart(body, ct, &limits);
        assert!(matches!(
            result,
            Err(MultipartError::InvalidContentType {
                field_name,
                ..
            }) if field_name == "file"
        ));
    }

    #[test]
    fn should_allow_allowed_content_type() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"test.png\"\r\n\
            Content-Type: image/png\r\n\r\n\
            PNG\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: DEFAULT_MAX_TOTAL_FILE_SIZE,
            allowed_content_types: vec!["image/".to_string()],
        };

        let parts = parse_multipart(body, ct, &limits).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].filename, Some("test.png".to_string()));
    }

    #[test]
    fn should_parse_multipart_data() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"username\"\r\n\r\n\
            john_doe\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"avatar\"; filename=\"avatar.png\"\r\n\
            Content-Type: image/png\r\n\r\n\
            fake_image_data\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits::default();

        let data = parse_multipart_data(body, ct, &limits).unwrap();

        assert_eq!(data.get_field("username"), Some("john_doe"));
        assert!(data.get_file("avatar").is_some());

        let avatar = data.get_file("avatar").unwrap();
        assert_eq!(avatar.filename, "avatar.png");
        assert_eq!(avatar.content_type, Some("image/png".to_string()));
        assert_eq!(avatar.size, 15);
    }

    #[test]
    fn should_handle_multiple_files_same_field() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"attachments\"; filename=\"file1.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            Content 1\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"attachments\"; filename=\"file2.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            Content 2\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits::default();

        let data = parse_multipart_data(body, ct, &limits).unwrap();

        let files = data.get_files("attachments").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].filename, "file1.txt");
        assert_eq!(files[1].filename, "file2.txt");
    }

    #[test]
    fn should_enforce_total_size_limit() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file1\"; filename=\"file1.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            Content that is more than 5 bytes\r\n\
            ------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file2\"; filename=\"file2.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            More content\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: 5, // Very small limit
            allowed_content_types: vec![],
        };

        let result = parse_multipart(body, ct, &limits);
        assert!(matches!(
            result,
            Err(MultipartError::TotalSizeExceeded { max: 5, .. })
        ));
    }

    #[test]
    fn should_report_total_size_violation_with_found_size() {
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"file.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            123456\r\n\
            ------WebKitFormBoundary--\r\n";

        let ct = "multipart/form-data; boundary=----WebKitFormBoundary";
        let limits = MultipartLimits {
            max_parts: DEFAULT_MAX_PARTS,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_total_file_size: 5,
            allowed_content_types: vec![],
        };

        let result = parse_multipart(body, ct, &limits);
        assert!(matches!(
            result,
            Err(MultipartError::TotalSizeExceeded { max: 5, found: 6 })
        ));
    }
}
