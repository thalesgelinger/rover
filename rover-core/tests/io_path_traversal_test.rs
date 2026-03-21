//! Tests for IO module path traversal protection

use mlua::Lua;
use rover_core::io::create_io_module;
use std::sync::Mutex;
use tempfile::TempDir;

// Static mutex to ensure tests that change directory run serially
static DIR_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn should_block_io_open_with_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("../etc/passwd", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with traversal path");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_open_with_encoded_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("%2e%2e/etc/passwd", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with encoded traversal path");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_open_with_absolute_path() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("/etc/passwd", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with absolute path");
    assert!(
        err.contains("Path validation failed") || err.contains("Absolute path"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_open_with_null_byte() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("file\0.txt", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with null byte in path");
    assert!(
        err.contains("Path validation failed") || err.contains("Invalid path"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_allow_io_open_with_safe_relative_path() {
    let _lock = DIR_MUTEX.lock().unwrap(); // Ensure serial execution

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, "Hello, World!").unwrap();

    // Ensure file exists
    assert!(file_path.exists(), "Test file should exist");

    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    // Change to temp directory so relative path works
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    // Verify we're in the right directory and file exists
    assert!(
        std::path::Path::new("test.txt").exists(),
        "File should exist in current dir"
    );

    let (ok, content): (bool, String) = lua
        .load(
            r#"
            local ok, file_or_err = pcall(function()
                return io.open("test.txt", "r")
            end)
            if not ok then
                return false, tostring(file_or_err)
            end
            local content = file_or_err:read("*a")
            file_or_err:close()
            return true, content
        "#,
        )
        .eval()
        .unwrap();

    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
    drop(_lock); // Release the lock

    assert!(ok, "Should succeed with safe relative path: {}", content);
    assert_eq!(content, "Hello, World!");
}

#[test]
fn should_allow_io_open_with_nested_safe_path() {
    let _lock = DIR_MUTEX.lock().unwrap(); // Ensure serial execution

    let temp_dir = TempDir::new().unwrap();
    let subdir = temp_dir.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let file_path = subdir.join("nested.txt");
    std::fs::write(&file_path, "Nested content").unwrap();

    // Ensure file exists
    assert!(file_path.exists(), "Test file should exist");

    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    // Change to temp directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    // Verify we're in the right directory and file exists
    assert!(
        std::path::Path::new("subdir/nested.txt").exists(),
        "File should exist in current dir"
    );

    let (ok, content): (bool, String) = lua
        .load(
            r#"
            local ok, file_or_err = pcall(function()
                return io.open("subdir/nested.txt", "r")
            end)
            if not ok then
                return false, tostring(file_or_err)
            end
            local content = file_or_err:read("*a")
            file_or_err:close()
            return true, content
        "#,
        )
        .eval()
        .unwrap();

    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
    drop(_lock); // Release the lock

    assert!(ok, "Should succeed with nested safe path: {}", content);
    assert_eq!(content, "Nested content");
}

#[test]
fn should_block_io_open_with_deep_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("foo/bar/../../../etc/passwd", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with deep traversal path");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_open_with_double_encoded_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open("%252e%252e/etc/passwd", "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with double encoded traversal path");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_lines_with_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                for line in io.lines("../etc/passwd") do
                    print(line)
                end
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with traversal path in io.lines");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_input_with_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                io.input("../etc/passwd")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with traversal path in io.input");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_io_output_with_traversal() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                io.output("../etc/passwd")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with traversal path in io.output");
    assert!(
        err.contains("Path validation failed") || err.contains("Directory traversal"),
        "Error should indicate path validation failure: {}",
        err
    );
}

#[test]
fn should_block_very_long_path() {
    let lua = Lua::new();
    let io_module = create_io_module(&lua).unwrap();
    lua.globals().set("io", io_module).unwrap();

    let long_path = "a".repeat(5000);
    lua.globals().set("long_path", long_path).unwrap();

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
                return io.open(long_path, "r")
            end)
            return ok, tostring(err)
        "#,
        )
        .eval()
        .unwrap();

    assert!(!ok, "Should fail with very long path");
    assert!(
        err.contains("Path validation failed") || err.contains("Path too long"),
        "Error should indicate path validation failure: {}",
        err
    );
}
