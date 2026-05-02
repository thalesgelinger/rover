use rover_types::{Permission, PermissionsConfig};
use std::fs;
use std::path::Path;

#[test]
fn should_permissions_example_file_exist() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    assert!(
        example_path.exists(),
        "permissions_example.lua should exist at examples/permissions_example.lua"
    );
}

#[test]
fn should_permissions_example_contain_production_mode() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("mode = \"production\""),
        "Example should demonstrate production mode configuration"
    );
}

#[test]
fn should_permissions_example_contain_deny_by_default() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("allow = {"),
        "Example should demonstrate explicit allow list for production permissions"
    );
}

#[test]
fn should_permissions_example_contain_allowed_capability_path() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("\"env\""),
        "Example should demonstrate 'env' as an allowed capability"
    );
}

#[test]
fn should_permissions_example_contain_env_access_usage() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("rover.env"),
        "Example should demonstrate actual usage of allowed env capability"
    );
}

#[test]
fn should_production_deny_by_default() {
    let config = PermissionsConfig::production();

    assert!(
        !config.is_allowed(Permission::Fs),
        "fs should be denied by default in production"
    );
    assert!(
        !config.is_allowed(Permission::Net),
        "net should be denied by default in production"
    );
    assert!(
        !config.is_allowed(Permission::Env),
        "env should be denied by default in production"
    );
    assert!(
        !config.is_allowed(Permission::Process),
        "process should be denied by default in production"
    );
    assert!(
        !config.is_allowed(Permission::Ffi),
        "ffi should be denied by default in production"
    );
}

#[test]
fn should_production_mode_allow_explicit_env_capability() {
    let config = PermissionsConfig::production().allow(Permission::Env);

    assert!(
        config.is_allowed(Permission::Env),
        "env should be allowed when explicitly granted in production"
    );
    assert!(
        !config.is_allowed(Permission::Fs),
        "fs should still be denied without explicit permission"
    );
    assert!(
        !config.is_allowed(Permission::Net),
        "net should still be denied without explicit permission"
    );
    assert!(
        !config.is_allowed(Permission::Process),
        "process should still be denied without explicit permission"
    );
}

#[test]
fn should_demonstrate_multiple_allowed_capabilities() {
    let config = PermissionsConfig::production()
        .allow(Permission::Env)
        .allow(Permission::Net);

    assert!(config.is_allowed(Permission::Env), "env should be allowed");
    assert!(config.is_allowed(Permission::Net), "net should be allowed");
    assert!(
        !config.is_allowed(Permission::Fs),
        "fs should be denied without permission"
    );
    assert!(
        !config.is_allowed(Permission::Process),
        "process should be denied without permission"
    );
}

#[test]
fn should_permissions_example_contain_development_mode() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("mode = \"development\""),
        "Example should demonstrate development mode for comparison"
    );
}

#[test]
fn should_permissions_example_contain_process_denied_example() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("process") && content.contains("deny"),
        "Example should show process permission being denied or handled"
    );
}

#[test]
fn should_permissions_example_contain_error_handling() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("pcall") || content.contains("error"),
        "Example should demonstrate permission error handling"
    );
}

#[test]
fn should_runnable_example_document_execution_command() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("cargo run -p rover_cli") || content.contains("rover run"),
        "Example should document how to run it"
    );
}

#[test]
fn should_permissions_example_link_to_docs() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/permissions_example.lua");

    let content =
        fs::read_to_string(&example_path).expect("Failed to read permissions_example.lua");

    assert!(
        content.contains("permissions") && (content.contains("docs") || content.contains("/docs")),
        "Example should link to documentation"
    );
}
