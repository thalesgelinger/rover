use std::fmt;

use crate::{
    AuditEventType, Permission, emit_auth_denied, emit_capability_denied, emit_file_access_denied,
    emit_permission_denied, emit_rate_limit_exceeded,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeniedError {
    Permission {
        permission: Permission,
        operation: String,
    },
    Capability {
        capability: String,
        target: String,
    },
    FileAccess {
        path: String,
        reason: FileAccessReason,
    },
    Auth {
        operation: String,
        reason: AuthReason,
    },
    RateLimit {
        identifier: String,
        limit: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAccessReason {
    TraversalAttempt,
    NotFound,
    DirectoryListing,
    InvalidPath,
    PathTooLong,
    AbsolutePathNotAllowed,
    PermissionDenied,
    Other(String),
}

impl FileAccessReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TraversalAttempt => "Directory traversal not allowed",
            Self::NotFound => "Not found",
            Self::DirectoryListing => "Directory listing not allowed",
            Self::InvalidPath => "Invalid path",
            Self::PathTooLong => "Path too long",
            Self::AbsolutePathNotAllowed => "Absolute paths not allowed",
            Self::PermissionDenied => "Permission denied",
            Self::Other(_) => "Access denied",
        }
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthReason {
    MissingToken,
    InvalidTokenFormat,
    InvalidToken,
    ExpiredToken,
    MissingClaims,
    InsufficientRole,
    Other(String),
}

impl AuthReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingToken => "missing bearer token",
            Self::InvalidTokenFormat => "invalid token format",
            Self::InvalidToken => "invalid token",
            Self::ExpiredToken => "token expired",
            Self::MissingClaims => "missing required claims",
            Self::InsufficientRole => "insufficient role",
            Self::Other(_) => "authentication failed",
        }
    }
}

impl DeniedError {
    pub fn permission(permission: Permission, operation: impl Into<String>) -> Self {
        Self::Permission {
            permission,
            operation: operation.into(),
        }
    }

    pub fn capability(capability: impl Into<String>, target: impl Into<String>) -> Self {
        Self::Capability {
            capability: capability.into(),
            target: target.into(),
        }
    }

    pub fn file_access(path: impl Into<String>, reason: FileAccessReason) -> Self {
        Self::FileAccess {
            path: path.into(),
            reason,
        }
    }

    pub fn auth(operation: impl Into<String>, reason: AuthReason) -> Self {
        Self::Auth {
            operation: operation.into(),
            reason,
        }
    }

    pub fn rate_limit(identifier: impl Into<String>, limit: u64) -> Self {
        Self::RateLimit {
            identifier: identifier.into(),
            limit,
        }
    }

    pub fn emit(&self) {
        match self {
            Self::Permission {
                permission,
                operation,
            } => {
                emit_permission_denied(permission.as_str(), operation);
            }
            Self::Capability { capability, target } => {
                emit_capability_denied(capability, target);
            }
            Self::FileAccess { path, reason } => {
                emit_file_access_denied(path, reason.as_str());
            }
            Self::Auth { operation, reason } => {
                emit_auth_denied(operation, reason.as_str());
            }
            Self::RateLimit { identifier, limit } => {
                emit_rate_limit_exceeded(identifier, *limit);
            }
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Permission { .. } => "Permission denied".to_string(),
            Self::Capability { capability, .. } => format!("Capability '{}' denied", capability),
            Self::FileAccess { reason, .. } => match reason {
                FileAccessReason::NotFound => "Not found".to_string(),
                FileAccessReason::DirectoryListing => "Directory listing not allowed".to_string(),
                _ => "Access denied".to_string(),
            },
            Self::Auth { reason, .. } => {
                format!("Unauthorized: {}", reason.as_str())
            }
            Self::RateLimit { .. } => "Rate limit exceeded".to_string(),
        }
    }

    pub fn audit_event_type(&self) -> AuditEventType {
        match self {
            Self::Permission { .. } => AuditEventType::PermissionDenied,
            Self::Capability { .. } => AuditEventType::CapabilityDenied,
            Self::FileAccess { .. } => AuditEventType::FileAccessDenied,
            Self::Auth { .. } => AuditEventType::AuthDenied,
            Self::RateLimit { .. } => AuditEventType::RateLimitExceeded,
        }
    }
}

impl fmt::Display for DeniedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for DeniedError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .try_init();
    }

    #[test]
    fn test_permission_error() {
        let err = DeniedError::permission(Permission::Fs, "file_read");
        assert_eq!(err.user_message(), "Permission denied");
        assert_eq!(err.audit_event_type(), AuditEventType::PermissionDenied);
    }

    #[test]
    fn test_capability_error() {
        let err = DeniedError::capability("tui", "web");
        assert_eq!(err.user_message(), "Capability 'tui' denied");
        assert_eq!(err.audit_event_type(), AuditEventType::CapabilityDenied);
    }

    #[test]
    fn test_file_access_error_traversal() {
        let err = DeniedError::file_access("/etc/passwd", FileAccessReason::TraversalAttempt);
        assert_eq!(err.user_message(), "Access denied");
        assert_eq!(err.audit_event_type(), AuditEventType::FileAccessDenied);
    }

    #[test]
    fn test_file_access_error_not_found() {
        let err = DeniedError::file_access("/missing/file.txt", FileAccessReason::NotFound);
        assert_eq!(err.user_message(), "Not found");
        assert!(matches!(err.user_message().as_str(), "Not found"));
    }

    #[test]
    fn test_auth_error_missing_token() {
        let err = DeniedError::auth("api_request", AuthReason::MissingToken);
        assert_eq!(err.user_message(), "Unauthorized: missing bearer token");
    }

    #[test]
    fn test_rate_limit_error() {
        let err = DeniedError::rate_limit("user_123", 100);
        assert_eq!(err.user_message(), "Rate limit exceeded");
    }

    #[test]
    fn test_emit_permission_denied() {
        init_tracing();
        let err = DeniedError::permission(Permission::Net, "http_request");
        err.emit();
    }

    #[test]
    fn test_file_access_reason_is_not_found() {
        assert!(FileAccessReason::NotFound.is_not_found());
        assert!(!FileAccessReason::TraversalAttempt.is_not_found());
    }

    #[test]
    fn test_auth_reason_as_str() {
        assert_eq!(AuthReason::MissingToken.as_str(), "missing bearer token");
        assert_eq!(
            AuthReason::InvalidTokenFormat.as_str(),
            "invalid token format"
        );
        assert_eq!(AuthReason::InvalidToken.as_str(), "invalid token");
        assert_eq!(AuthReason::ExpiredToken.as_str(), "token expired");
        assert_eq!(
            AuthReason::MissingClaims.as_str(),
            "missing required claims"
        );
        assert_eq!(AuthReason::InsufficientRole.as_str(), "insufficient role");
        assert_eq!(
            AuthReason::Other("custom".to_string()).as_str(),
            "authentication failed"
        );
    }

    #[test]
    fn test_file_access_reason_as_str() {
        assert_eq!(
            FileAccessReason::TraversalAttempt.as_str(),
            "Directory traversal not allowed"
        );
        assert_eq!(FileAccessReason::NotFound.as_str(), "Not found");
        assert_eq!(
            FileAccessReason::DirectoryListing.as_str(),
            "Directory listing not allowed"
        );
        assert_eq!(FileAccessReason::InvalidPath.as_str(), "Invalid path");
        assert_eq!(FileAccessReason::PathTooLong.as_str(), "Path too long");
        assert_eq!(
            FileAccessReason::AbsolutePathNotAllowed.as_str(),
            "Absolute paths not allowed"
        );
        assert_eq!(
            FileAccessReason::PermissionDenied.as_str(),
            "Permission denied"
        );
        assert_eq!(
            FileAccessReason::Other("custom".to_string()).as_str(),
            "Access denied"
        );
    }

    #[test]
    fn test_display_trait() {
        let err = DeniedError::rate_limit("user_123", 100);
        assert_eq!(format!("{}", err), "Rate limit exceeded");
    }
}
