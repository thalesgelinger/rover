use mlua::{FromLua, Lua, Value};
use rover_server::ServerConfig;

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
