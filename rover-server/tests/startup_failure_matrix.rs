use mlua::{FromLua, Lua, Value};
use rover_server::ServerConfig;
use rover_types::Permission;

fn parse_config(lua_src: &str) -> mlua::Result<ServerConfig> {
    let lua = Lua::new();
    let value: Value = lua.load(lua_src).eval()?;
    ServerConfig::from_lua(value, &lua)
}

#[test]
fn should_reject_public_bind_when_strict_mode_enabled() {
    let err = parse_config("{ host = '0.0.0.0', body_size_limit = 1024 }")
        .expect_err("strict mode should reject public bind");
    assert!(err.to_string().contains("allow_public_bind = true"));
}

#[test]
fn should_reject_unbounded_body_when_strict_mode_enabled() {
    let err = parse_config("{ body_size_limit = 0 }")
        .expect_err("strict mode should reject unbounded body");
    assert!(err.to_string().contains("allow_unbounded_body = true"));
}

#[test]
fn should_reject_wildcard_cors_credentials_when_strict_mode_enabled() {
    let err =
        parse_config("{ cors_origin = '*', cors_credentials = true, body_size_limit = 1024 }")
            .expect_err("strict mode should reject wildcard CORS credentials");
    assert!(
        err.to_string()
            .contains("allow_wildcard_cors_credentials = true")
    );
}

#[test]
fn should_report_all_strict_mode_startup_violations() {
    let err = parse_config(
        "{ host = '0.0.0.0', body_size_limit = 0, cors_origin = '*', cors_credentials = true }",
    )
    .expect_err("strict mode should report every violation");
    let text = err.to_string();
    assert!(text.contains("allow_public_bind = true"));
    assert!(text.contains("allow_unbounded_body = true"));
    assert!(text.contains("allow_wildcard_cors_credentials = true"));
}

#[test]
fn should_allow_failure_matrix_when_strict_mode_disabled() {
    let config = parse_config(
        "{ strict_mode = false, host = '0.0.0.0', body_size_limit = 0, cors_origin = '*', cors_credentials = true }",
    )
    .expect("strict mode opt-out should allow startup");

    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.body_size_limit, None);
    assert!(config.cors_credentials);
}

#[test]
fn should_reject_invalid_permission_in_allow() {
    let err = parse_config("{ permissions = { allow = { 'invalid_perm' } } }")
        .expect_err("should reject invalid permission in allow");
    assert!(
        err.to_string()
            .contains("permissions.allow contains invalid permission 'invalid_perm'")
    );
}

#[test]
fn should_reject_invalid_permission_in_deny() {
    let err = parse_config("{ permissions = { deny = { 'unknown_perm' } } }")
        .expect_err("should reject invalid permission in deny");
    assert!(
        err.to_string()
            .contains("permissions.deny contains invalid permission 'unknown_perm'")
    );
}

#[test]
fn should_reject_ambiguous_permissions_in_both_allow_and_deny() {
    let err = parse_config("{ permissions = { allow = { 'fs', 'net' }, deny = { 'fs' } } }")
        .expect_err("should reject ambiguous permissions");
    assert!(err.to_string().contains(
        "permissions contains ambiguous permissions that appear in both allow and deny: fs"
    ));
}

#[test]
fn should_reject_multiple_ambiguous_permissions() {
    let err = parse_config(
        "{ permissions = { allow = { 'fs', 'net', 'env' }, deny = { 'fs', 'env' } } }",
    )
    .expect_err("should reject multiple ambiguous permissions");
    let err_text = err.to_string();
    assert!(err_text.contains("ambiguous permissions"));
    assert!(err_text.contains("fs"));
    assert!(err_text.contains("env"));
}

#[test]
fn should_accept_valid_permissions_config() {
    let config =
        parse_config("{ permissions = { allow = { 'fs', 'net' }, deny = { 'process', 'ffi' } }, strict_mode = false }")
            .expect("should accept valid permissions config");

    assert!(config.permissions.is_allowed(Permission::Fs));
    assert!(config.permissions.is_allowed(Permission::Net));
    assert!(!config.permissions.is_allowed(Permission::Process));
    assert!(!config.permissions.is_allowed(Permission::Ffi));
}

#[test]
fn should_accept_empty_permissions_config() {
    let config = parse_config("{ strict_mode = false }")
        .expect("should accept empty config and use defaults");

    // Should use default development mode permissions
    assert!(config.permissions.is_allowed(Permission::Fs));
    assert!(config.permissions.is_allowed(Permission::Net));
    assert!(config.permissions.is_allowed(Permission::Env));
    assert!(!config.permissions.is_allowed(Permission::Process));
    assert!(!config.permissions.is_allowed(Permission::Ffi));
}

#[test]
fn should_apply_production_permission_mode_defaults() {
    let config = parse_config("{ permissions = { mode = 'production' }, strict_mode = false }")
        .expect("should accept production permission mode");

    assert!(!config.permissions.is_allowed(Permission::Fs));
    assert!(!config.permissions.is_allowed(Permission::Net));
    assert!(!config.permissions.is_allowed(Permission::Env));
    assert!(!config.permissions.is_allowed(Permission::Process));
    assert!(!config.permissions.is_allowed(Permission::Ffi));
}

#[test]
fn should_accept_permission_mode_aliases() {
    let dev_alias = parse_config("{ permissions = { mode = 'dev' }, strict_mode = false }")
        .expect("should accept dev mode alias");
    let prod_alias =
        parse_config("{ permissions = { mode = 'prod', allow = { 'env' } }, strict_mode = false }")
            .expect("should accept prod mode alias");

    assert!(dev_alias.permissions.is_allowed(Permission::Fs));
    assert!(!dev_alias.permissions.is_allowed(Permission::Process));

    assert!(prod_alias.permissions.is_allowed(Permission::Env));
    assert!(!prod_alias.permissions.is_allowed(Permission::Fs));
}
