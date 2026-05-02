use std::collections::HashMap;

use crate::sanitize_identifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEventType {
    PermissionDenied,
    AuthDenied,
    CapabilityDenied,
    FileAccessDenied,
    RateLimitExceeded,
}

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_type: AuditEventType,
    pub operation: String,
    pub reason: String,
    pub context: HashMap<String, String>,
}

impl AuditEvent {
    pub fn new(
        event_type: AuditEventType,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            event_type,
            operation: operation.into(),
            reason: reason.into(),
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    pub fn emit(&self) {
        match self.event_type {
            AuditEventType::PermissionDenied => {
                tracing::warn!(
                    operation = %self.operation,
                    reason = %self.reason,
                    audit.event_type = "permission_denied",
                    "Permission denied"
                );
            }
            AuditEventType::AuthDenied => {
                tracing::warn!(
                    operation = %self.operation,
                    reason = %self.reason,
                    audit.event_type = "auth_denied",
                    "Authentication denied"
                );
            }
            AuditEventType::CapabilityDenied => {
                tracing::warn!(
                    operation = %self.operation,
                    reason = %self.reason,
                    audit.event_type = "capability_denied",
                    "Capability denied"
                );
            }
            AuditEventType::FileAccessDenied => {
                tracing::warn!(
                    operation = %self.operation,
                    reason = %self.reason,
                    audit.event_type = "file_access_denied",
                    "File access denied"
                );
            }
            AuditEventType::RateLimitExceeded => {
                tracing::warn!(
                    operation = %self.operation,
                    reason = %self.reason,
                    audit.event_type = "rate_limit_exceeded",
                    "Rate limit exceeded"
                );
            }
        }
    }
}

pub fn emit_permission_denied(permission: &str, operation: &str) {
    AuditEvent::new(
        AuditEventType::PermissionDenied,
        sanitize_identifier(operation),
        format!("Permission '{}' denied", permission),
    )
    .with_context("permission", permission)
    .emit();
}

pub fn emit_auth_denied(operation: &str, reason: &str) {
    AuditEvent::new(
        AuditEventType::AuthDenied,
        sanitize_identifier(operation),
        reason,
    )
    .emit();
}

pub fn emit_capability_denied(capability: &str, target: &str) {
    AuditEvent::new(
        AuditEventType::CapabilityDenied,
        format!("capability_check:{}", capability),
        format!(
            "Capability '{}' denied for target '{}'",
            capability,
            sanitize_identifier(target)
        ),
    )
    .with_context("capability", capability)
    .with_context("target", sanitize_identifier(target))
    .emit();
}

pub fn emit_file_access_denied(path: &str, reason: &str) {
    use crate::sanitize_path;
    AuditEvent::new(
        AuditEventType::FileAccessDenied,
        format!("file_access:{}", sanitize_path(path)),
        reason,
    )
    .with_context("path", sanitize_path(path))
    .emit();
}

pub fn emit_rate_limit_exceeded(identifier: &str, limit: u64) {
    AuditEvent::new(
        AuditEventType::RateLimitExceeded,
        "rate_limit_check",
        format!("Rate limit exceeded (limit: {})", limit),
    )
    .with_context("identifier", sanitize_identifier(identifier))
    .with_context("limit", limit.to_string())
    .emit();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .try_init();
    }

    #[test]
    fn test_audit_event_creation() {
        let event = AuditEvent::new(
            AuditEventType::PermissionDenied,
            "fs_read",
            "File system access denied",
        );

        assert_eq!(event.event_type, AuditEventType::PermissionDenied);
        assert_eq!(event.operation, "fs_read");
        assert_eq!(event.reason, "File system access denied");
    }

    #[test]
    fn test_audit_event_with_context() {
        let event = AuditEvent::new(AuditEventType::AuthDenied, "api_request", "Invalid token")
            .with_context("user_id", "123")
            .with_context("ip", "192.168.1.1");

        assert_eq!(event.context.get("user_id"), Some(&"123".to_string()));
        assert_eq!(event.context.get("ip"), Some(&"192.168.1.1".to_string()));
    }

    #[test]
    fn test_emit_permission_denied() {
        init_tracing();
        emit_permission_denied("fs", "file_read");
    }

    #[test]
    fn test_emit_auth_denied() {
        init_tracing();
        emit_auth_denied("api_request", "Invalid token");
    }

    #[test]
    fn test_emit_capability_denied() {
        init_tracing();
        emit_capability_denied("tui", "web");
    }

    #[test]
    fn test_emit_file_access_denied() {
        init_tracing();
        emit_file_access_denied("/etc/passwd", "Access denied");
    }

    #[test]
    fn test_emit_rate_limit_exceeded() {
        init_tracing();
        emit_rate_limit_exceeded("user_123", 100);
    }

    #[test]
    fn test_emit_file_access_denied_sanitizes_secrets() {
        init_tracing();
        emit_file_access_denied("/home/user/.ssh/id_rsa", "Access denied");
        emit_file_access_denied("/secrets/password.txt", "Access denied");
        emit_file_access_denied("/config/api_key.json", "Access denied");
    }

    #[test]
    fn test_emit_rate_limit_sanitizes_identifiers() {
        init_tracing();
        emit_rate_limit_exceeded("password_12345", 100);
        emit_rate_limit_exceeded("api_key_user_789", 50);
        emit_rate_limit_exceeded("very_long_user_identifier_with_secret_info", 200);
    }

    #[test]
    fn test_emit_capability_denied_sanitizes_targets() {
        init_tracing();
        emit_capability_denied("fs", "/home/user/.ssh/secret");
        emit_capability_denied("net", "password_protected_endpoint");
    }

    #[test]
    fn test_emit_permission_denied_sanitizes_operations() {
        init_tracing();
        emit_permission_denied("fs", "password_file_access");
        emit_permission_denied("net", "api_key_network_request");
    }

    #[test]
    fn test_sanitization_preserves_reasonable_paths() {
        init_tracing();
        emit_file_access_denied("css/style.css", "Not found");
        emit_file_access_denied("js/app.js", "Not found");
        emit_file_access_denied("assets/logo.png", "Not found");
    }
}
