use mlua::Lua;
pub use rover_types::{Permission, PermissionMode, PermissionsConfig};

pub fn has_permission(lua: &Lua, permission: Permission) -> mlua::Result<bool> {
    Ok(lua
        .app_data_ref::<PermissionsConfig>()
        .map(|config| config.is_allowed(permission))
        .unwrap_or_else(|| permission.allowed_by_default(PermissionMode::Development)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_as_str() {
        assert_eq!(Permission::Fs.as_str(), "fs");
        assert_eq!(Permission::Net.as_str(), "net");
        assert_eq!(Permission::Env.as_str(), "env");
        assert_eq!(Permission::Process.as_str(), "process");
        assert_eq!(Permission::Ffi.as_str(), "ffi");
    }

    #[test]
    fn test_permission_from_str() {
        assert_eq!("fs".parse::<Permission>(), Ok(Permission::Fs));
        assert_eq!("net".parse::<Permission>(), Ok(Permission::Net));
        assert_eq!("env".parse::<Permission>(), Ok(Permission::Env));
        assert_eq!("process".parse::<Permission>(), Ok(Permission::Process));
        assert_eq!("ffi".parse::<Permission>(), Ok(Permission::Ffi));
        assert_eq!("invalid".parse::<Permission>(), Err(()));
    }

    #[test]
    fn test_default_permissions() {
        let config = PermissionsConfig::new();

        assert!(config.is_allowed(Permission::Fs));
        assert!(config.is_allowed(Permission::Net));
        assert!(config.is_allowed(Permission::Env));
        assert!(!config.is_allowed(Permission::Process));
        assert!(!config.is_allowed(Permission::Ffi));
    }

    #[test]
    fn test_production_mode_is_deny_by_default() {
        let config = PermissionsConfig::production();

        assert!(!config.is_allowed(Permission::Fs));
        assert!(!config.is_allowed(Permission::Net));
        assert!(!config.is_allowed(Permission::Env));
        assert!(!config.is_allowed(Permission::Process));
        assert!(!config.is_allowed(Permission::Ffi));
    }

    #[test]
    fn test_production_mode_respects_explicit_allow() {
        let config = PermissionsConfig::production().allow(Permission::Env);

        assert!(config.is_allowed(Permission::Env));
        assert!(!config.is_allowed(Permission::Fs));
    }

    #[test]
    fn test_allow_permission() {
        let config = PermissionsConfig::new().allow(Permission::Process);

        assert!(config.is_allowed(Permission::Process));
    }

    #[test]
    fn test_deny_permission() {
        let config = PermissionsConfig::new().deny(Permission::Fs);

        assert!(!config.is_allowed(Permission::Fs));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let config = PermissionsConfig::new()
            .allow(Permission::Process)
            .deny(Permission::Process);

        assert!(!config.is_allowed(Permission::Process));
    }

    #[test]
    fn test_allowed_permissions_list() {
        let config = PermissionsConfig::new()
            .allow(Permission::Process)
            .allow(Permission::Ffi);

        let allowed = config.allowed_permissions();
        assert_eq!(allowed.len(), 2);
    }

    #[test]
    fn test_denied_permissions_list() {
        let config = PermissionsConfig::new()
            .deny(Permission::Fs)
            .deny(Permission::Net);

        let denied = config.denied_permissions();
        assert_eq!(denied.len(), 2);
    }
}
