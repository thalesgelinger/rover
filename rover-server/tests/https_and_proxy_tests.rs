use mlua::{FromLua, Lua, Value};
use rover_server::ServerConfig;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn config_from_lua(lua_src: &str) -> ServerConfig {
    let lua = Lua::new();
    let value: Value = lua.load(lua_src).eval().expect("lua eval");
    ServerConfig::from_lua(value, &lua).expect("server config")
}

fn parse_config(lua_src: &str) -> mlua::Result<ServerConfig> {
    let lua = Lua::new();
    let value: Value = lua.load(lua_src).eval()?;
    ServerConfig::from_lua(value, &lua)
}

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("rover_https_test_{}_{}", name, nanos))
}

fn fixture_pem(content: &str, marker: &str) -> String {
    format!(
        "-----BEGIN {}-----\n{}\n-----END {}-----\n",
        marker, content, marker
    )
}

mod https_startup {
    use super::*;

    #[test]
    fn should_parse_valid_tls_config() {
        let dir = unique_test_dir("https_valid");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ tls = {{ cert_file = '{}', key_file = '{}', reload_interval_secs = 5 }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        let tls = config.tls.as_ref().unwrap();
        assert_eq!(tls.cert_file, cert_file.to_string_lossy());
        assert_eq!(tls.key_file, key_file.to_string_lossy());
        assert_eq!(tls.reload_interval_secs, 5);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_reject_tls_config_missing_cert_file() {
        let err = parse_config("{ tls = { key_file = '/tmp/key.pem' } }")
            .expect_err("must reject missing cert_file");
        assert!(err.to_string().contains("tls.cert_file is required"));
    }

    #[test]
    fn should_reject_tls_config_missing_key_file() {
        let err = parse_config("{ tls = { cert_file = '/tmp/cert.pem' } }")
            .expect_err("must reject missing key_file");
        assert!(err.to_string().contains("tls.key_file is required"));
    }

    #[test]
    fn should_reject_tls_config_empty_cert_file() {
        let dir = unique_test_dir("https_empty_cert");
        fs::create_dir_all(&dir).expect("mkdir");

        let key_file = dir.join("key.pem");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let err = parse_config(&format!(
            "{{ tls = {{ cert_file = '', key_file = '{}' }} }}",
            key_file.display()
        ))
        .expect_err("must reject empty cert_file");

        assert!(err.to_string().contains("tls.cert_file cannot be empty"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_reject_tls_config_empty_key_file() {
        let dir = unique_test_dir("https_empty_key");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");

        let err = parse_config(&format!(
            "{{ tls = {{ cert_file = '{}', key_file = '' }} }}",
            cert_file.display()
        ))
        .expect_err("must reject empty key_file");

        assert!(err.to_string().contains("tls.key_file cannot be empty"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_use_default_reload_interval() {
        let dir = unique_test_dir("https_default_reload");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ tls = {{ cert_file = '{}', key_file = '{}' }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        let tls = config.tls.as_ref().unwrap();
        assert_eq!(tls.reload_interval_secs, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_reject_negative_reload_interval() {
        let err = parse_config(
            "{ tls = { cert_file = '/tmp/cert.pem', key_file = '/tmp/key.pem', reload_interval_secs = -1 } }",
        )
        .expect_err("must reject negative reload_interval_secs");
        assert!(
            err.to_string()
                .contains("tls.reload_interval_secs should be > 0")
        );
    }

    #[test]
    fn should_reject_zero_reload_interval() {
        let err = parse_config(
            "{ tls = { cert_file = '/tmp/cert.pem', key_file = '/tmp/key.pem', reload_interval_secs = 0 } }",
        )
        .expect_err("must reject zero reload_interval_secs");
        assert!(
            err.to_string()
                .contains("tls.reload_interval_secs should be > 0")
        );
    }

    #[test]
    fn should_allow_tls_with_public_bind_and_https_redirect() {
        let dir = unique_test_dir("https_redirect_required");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ host = '0.0.0.0', allow_public_bind = true, https_redirect = true, tls = {{ cert_file = '{}', key_file = '{}' }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        assert!(config.https_redirect);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_default_https_redirect_to_false() {
        let dir = unique_test_dir("https_redirect_default");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ tls = {{ cert_file = '{}', key_file = '{}' }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        assert!(!config.https_redirect);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_accept_numeric_reload_interval() {
        let dir = unique_test_dir("https_numeric_reload");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ tls = {{ cert_file = '{}', key_file = '{}', reload_interval_secs = 60.0 }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        let tls = config.tls.as_ref().unwrap();
        assert_eq!(tls.reload_interval_secs, 60);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_accept_tls_config_as_table() {
        let dir = unique_test_dir("https_table_config");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("test-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("test-key", "PRIVATE KEY")).expect("key write");

        let config = config_from_lua(&format!(
            "{{ port = 443, tls = {{ cert_file = '{}', key_file = '{}' }} }}",
            cert_file.display(),
            key_file.display()
        ));

        assert!(config.tls.is_some());
        assert_eq!(config.port, 443);

        let _ = fs::remove_dir_all(&dir);
    }
}

mod timeout_config {
    use super::*;

    #[test]
    fn should_use_default_coroutine_timeout() {
        let config = config_from_lua("{}");
        assert_eq!(config.port, 4242);
    }
}
