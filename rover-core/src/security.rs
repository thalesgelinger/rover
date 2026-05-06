//! Path traversal protection utilities
//!
//! This module provides functions to prevent directory traversal attacks
//! by validating paths before they are used for file operations.

use std::path::{Component, Path, PathBuf};

/// Maximum allowed path length to prevent abuse
const MAX_PATH_LENGTH: usize = 4096;

/// Error type for path validation failures
#[derive(Debug, Clone, PartialEq)]
pub enum PathValidationError {
    PathTooLong,
    InvalidPath,
    DirectoryTraversal,
    AbsolutePathNotAllowed,
    NotFound,
}

impl std::fmt::Display for PathValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathValidationError::PathTooLong => write!(f, "Path too long"),
            PathValidationError::InvalidPath => write!(f, "Invalid path"),
            PathValidationError::DirectoryTraversal => write!(f, "Directory traversal not allowed"),
            PathValidationError::AbsolutePathNotAllowed => write!(f, "Absolute paths not allowed"),
            PathValidationError::NotFound => write!(f, "Not found"),
        }
    }
}

impl std::error::Error for PathValidationError {}

/// Validates that a path doesn't contain directory traversal attempts.
///
/// This function checks for:
/// - Path length limits
/// - Null byte injection
/// - Parent directory references (..)
/// - URL-encoded traversal attempts
/// - Absolute paths
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// * `Ok(())` if the path is safe
/// * `Err(PathValidationError)` if a traversal attempt is detected
///
/// # Examples
/// ```
/// use rover_core::security::validate_path;
///
/// // Safe paths
/// assert!(validate_path("file.txt").is_ok());
/// assert!(validate_path("subdir/file.txt").is_ok());
/// assert!(validate_path("./file.txt").is_ok());
///
/// // Traversal attempts are blocked
/// assert!(validate_path("../etc/passwd").is_err());
/// assert!(validate_path("%2e%2e/etc/passwd").is_err());
/// ```
pub fn validate_path(path: &str) -> Result<(), PathValidationError> {
    // Check path length
    if path.len() > MAX_PATH_LENGTH {
        return Err(PathValidationError::PathTooLong);
    }

    // Remove null bytes
    if path.contains('\0') {
        return Err(PathValidationError::InvalidPath);
    }

    // Reject control characters (tab, newline, carriage return)
    if path.contains('\t') || path.contains('\n') || path.contains('\r') {
        return Err(PathValidationError::InvalidPath);
    }

    // Check for invalid percent encoding
    if path.contains('%') && !is_valid_percent_encoding(path) {
        return Err(PathValidationError::InvalidPath);
    }

    // Check for traversal attempts
    if has_traversal_attempt(path) {
        return Err(PathValidationError::DirectoryTraversal);
    }

    // Check for absolute paths - any path starting with / is absolute
    if path.starts_with('/') {
        return Err(PathValidationError::AbsolutePathNotAllowed);
    }

    // Check for Windows-style absolute paths and prefixes
    let path_obj = Path::new(path);
    for component in path_obj.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(PathValidationError::DirectoryTraversal);
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PathValidationError::AbsolutePathNotAllowed);
            }
        }
    }

    Ok(())
}

/// Validates a path and returns the canonicalized path if valid.
///
/// This is a more strict version that also checks the path exists
/// and is within an allowed base directory.
///
/// # Arguments
/// * `base_path` - The base directory that must contain the target
/// * `requested_path` - The path requested by the user
///
/// # Returns
/// * `Ok(PathBuf)` with the canonicalized path if valid
/// * `Err(PathValidationError)` if validation fails
pub fn validate_and_canonicalize(
    base_path: &Path,
    requested_path: &str,
) -> Result<PathBuf, PathValidationError> {
    // First, do basic validation
    validate_path(requested_path)?;

    // Normalize URL-style leading slashes to a relative path
    let normalized_path = requested_path.trim_start_matches('/');
    let requested = Path::new(normalized_path);

    // Build the full path manually to avoid any automatic normalization
    let mut full_path = base_path.to_path_buf();
    for component in requested.components() {
        match component {
            Component::Normal(name) => {
                full_path.push(name);
            }
            Component::ParentDir => {
                return Err(PathValidationError::DirectoryTraversal);
            }
            Component::CurDir => {
                continue;
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PathValidationError::AbsolutePathNotAllowed);
            }
        }
    }

    // Canonicalize the base path
    let canonical_base = base_path
        .canonicalize()
        .map_err(|_| PathValidationError::InvalidPath)?;

    // Check if file exists before canonicalizing
    if !full_path.exists() {
        return Err(PathValidationError::NotFound);
    }

    // Canonicalize the requested path
    let canonical_requested = full_path
        .canonicalize()
        .map_err(|_| PathValidationError::InvalidPath)?;

    // Verify the canonical path is within the base directory
    if !canonical_requested.starts_with(&canonical_base) {
        return Err(PathValidationError::DirectoryTraversal);
    }

    Ok(canonical_requested)
}

/// Checks if a path contains directory traversal attempts.
fn has_traversal_attempt(path: &str) -> bool {
    if has_parent_dir_component(path) {
        return true;
    }

    if !path.contains('%') {
        return false;
    }

    // Try to decode the path and check again
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

/// Checks if a path contains parent directory components.
fn has_parent_dir_component(path: &str) -> bool {
    Path::new(path.trim_start_matches('/'))
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

/// Validates percent encoding in a string.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_allow_safe_relative_paths() {
        assert!(validate_path("file.txt").is_ok());
        assert!(validate_path("subdir/file.txt").is_ok());
        assert!(validate_path("deep/nested/path/file.txt").is_ok());
    }

    #[test]
    fn should_allow_current_dir_references() {
        assert!(validate_path("./file.txt").is_ok());
        assert!(validate_path("./subdir/file.txt").is_ok());
    }

    #[test]
    fn should_reject_parent_dir_references() {
        assert!(validate_path("../file.txt").is_err());
        assert!(validate_path("../etc/passwd").is_err());
        assert!(validate_path("foo/../../etc/passwd").is_err());
        assert!(validate_path("foo/bar/../../../etc/passwd").is_err());
    }

    #[test]
    fn should_reject_encoded_parent_dir() {
        assert!(validate_path("%2e%2e/file.txt").is_err());
        assert!(validate_path("%2e%2e%2fetc%2fpasswd").is_err());
        assert!(validate_path("..%2f..%2fetc%2fpasswd").is_err());
    }

    #[test]
    fn should_reject_double_encoded_parent_dir() {
        assert!(validate_path("%252e%252e/etc/passwd").is_err());
    }

    #[test]
    fn should_reject_absolute_paths() {
        assert!(validate_path("/etc/passwd").is_err());
        assert!(validate_path("/absolute/path").is_err());
    }

    #[test]
    fn should_reject_paths_with_leading_parent_dir() {
        assert!(validate_path("/../etc/passwd").is_err());
    }

    #[test]
    fn should_reject_null_bytes() {
        assert!(validate_path("file\0.txt").is_err());
        assert!(validate_path("\0/etc/passwd").is_err());
    }

    #[test]
    fn should_reject_very_long_paths() {
        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        assert!(validate_path(&long_path).is_err());
    }

    #[test]
    fn should_reject_invalid_percent_encoding() {
        assert!(validate_path("file%ZZ.txt").is_err());
        assert!(validate_path("file%G.txt").is_err());
        assert!(validate_path("file%.txt").is_err());
    }

    #[test]
    fn should_allow_valid_percent_encoding() {
        assert!(validate_path("file%20name.txt").is_ok());
        assert!(validate_path("path%2Ffile.txt").is_ok());
    }

    #[test]
    fn should_reject_mixed_case_encoding() {
        assert!(validate_path("%2E%2e%2F%2f/file.txt").is_err());
    }

    #[test]
    fn should_reject_tab_injection() {
        assert!(validate_path("..\t/file.txt").is_err());
    }

    #[test]
    fn should_reject_newline_injection() {
        assert!(validate_path("..\n/file.txt").is_err());
        assert!(validate_path("..\r/file.txt").is_err());
    }

    #[test]
    fn should_reject_paths_with_fragments() {
        assert!(validate_path("file.txt#../../../etc/passwd").is_err());
    }

    #[test]
    fn should_reject_paths_with_query_strings() {
        assert!(validate_path("file.txt?../../etc/passwd").is_err());
    }

    #[test]
    fn should_allow_safe_encoded_characters() {
        assert!(validate_path("file%20name.txt").is_ok());
        assert!(validate_path("test%2Dfile.txt").is_ok());
    }

    #[test]
    fn should_reject_backslash_traversal() {
        // Backslashes in paths are treated as normal characters on Unix
        // but could be used for traversal on Windows
        assert!(validate_path("..\\..\\etc/passwd").is_ok()); // Backslash is normal char
    }
}
