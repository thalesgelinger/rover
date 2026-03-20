use std::collections::HashSet;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    Fs,
    Net,
    Env,
    Process,
    Ffi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Development,
    Production,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Fs => "fs",
            Permission::Net => "net",
            Permission::Env => "env",
            Permission::Process => "process",
            Permission::Ffi => "ffi",
        }
    }

    pub fn allowed_by_default(self, mode: PermissionMode) -> bool {
        match mode {
            PermissionMode::Development => match self {
                Permission::Fs => true,
                Permission::Net => true,
                Permission::Env => true,
                Permission::Process => false,
                Permission::Ffi => false,
            },
            PermissionMode::Production => false,
        }
    }
}

impl FromStr for Permission {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fs" => Ok(Permission::Fs),
            "net" => Ok(Permission::Net),
            "env" => Ok(Permission::Env),
            "process" => Ok(Permission::Process),
            "ffi" => Ok(Permission::Ffi),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PermissionsConfig {
    pub mode: PermissionMode,
    pub allow: HashSet<Permission>,
    pub deny: HashSet<Permission>,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionsConfig {
    pub fn new() -> Self {
        Self::new_with_mode(PermissionMode::Development)
    }

    pub fn production() -> Self {
        Self::new_with_mode(PermissionMode::Production)
    }

    pub fn new_with_mode(mode: PermissionMode) -> Self {
        Self {
            mode,
            allow: HashSet::new(),
            deny: HashSet::new(),
        }
    }

    pub fn allow(mut self, permission: Permission) -> Self {
        self.allow.insert(permission);
        self
    }

    pub fn deny(mut self, permission: Permission) -> Self {
        self.deny.insert(permission);
        self
    }

    pub fn is_allowed(&self, permission: Permission) -> bool {
        if self.deny.contains(&permission) {
            return false;
        }
        if self.allow.contains(&permission) {
            return true;
        }
        permission.allowed_by_default(self.mode)
    }

    pub fn allowed_permissions(&self) -> Vec<&Permission> {
        self.allow.iter().collect()
    }

    pub fn denied_permissions(&self) -> Vec<&Permission> {
        self.deny.iter().collect()
    }
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
