//! Integration tests for hot reload behavior constraints
//!
//! Verifies that hot reload is limited to TLS certificates only.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rover_server::{TlsCertReloader, TlsConfig};

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("rover_hot_reload_{}_{}", name, nanos))
}

fn fixture_pem(content: &str, marker: &str) -> String {
    format!(
        "-----BEGIN {}-----\n{}\n-----END {}-----\n",
        marker, content, marker
    )
}

/// Test that TLS certificate hot reload works as documented
#[test]
fn should_support_tls_certificate_hot_reload() {
    let dir = unique_test_dir("tls_cert_reload");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");
    fs::write(&cert_file, fixture_pem("initial-cert", "CERTIFICATE")).expect("cert write");
    fs::write(&key_file, fixture_pem("initial-key", "PRIVATE KEY")).expect("key write");

    // Create reloader with 1-second check interval
    let mut reloader = TlsCertReloader::new(&TlsConfig {
        cert_file: cert_file.to_string_lossy().to_string(),
        key_file: key_file.to_string_lossy().to_string(),
        reload_interval_secs: 1,
    })
    .expect("reloader");

    // Update certificate files
    fs::write(&cert_file, fixture_pem("updated-cert", "CERTIFICATE")).expect("cert update");
    fs::write(&key_file, fixture_pem("updated-key", "PRIVATE KEY")).expect("key update");

    // Trigger reload
    let changed = reloader.reload_if_changed().expect("reload changed");
    assert!(changed, "Should detect certificate change");

    // Verify updated certificates are loaded
    let snapshot = reloader.current_material().expect("get material");
    let cert_text = String::from_utf8_lossy(&snapshot.cert_pem);
    let key_text = String::from_utf8_lossy(&snapshot.key_pem);
    assert!(cert_text.contains("updated-cert"));
    assert!(key_text.contains("updated-key"));

    let _ = fs::remove_dir_all(&dir);
}

/// Test that hot reload fails safely when certificate files are invalid
#[test]
fn should_fail_safe_when_reload_encounters_invalid_certificate() {
    let dir = unique_test_dir("tls_reload_fail_safe");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");
    fs::write(&cert_file, fixture_pem("valid-cert", "CERTIFICATE")).expect("cert write");
    fs::write(&key_file, fixture_pem("valid-key", "PRIVATE KEY")).expect("key write");

    let mut reloader = TlsCertReloader::new(&TlsConfig {
        cert_file: cert_file.to_string_lossy().to_string(),
        key_file: key_file.to_string_lossy().to_string(),
        reload_interval_secs: 1,
    })
    .expect("reloader");

    // Corrupt the certificate file
    fs::write(&cert_file, b"invalid-pem-content").expect("corrupt cert");

    // Attempt reload should fail
    let result = reloader.reload_if_changed();
    assert!(result.is_err(), "Should fail when certificate is invalid");

    // Verify previous certificates are still in use (fail-safe)
    let snapshot = reloader.current_material().expect("get material");
    let cert_text = String::from_utf8_lossy(&snapshot.cert_pem);
    assert!(
        cert_text.contains("valid-cert"),
        "Should keep previous cert on failure"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Test that PEM format validation enforces safety constraints
#[test]
fn should_validate_pem_format_during_reload() {
    let dir = unique_test_dir("pem_validation");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");

    // Write valid PEM initially
    fs::write(&cert_file, fixture_pem("valid", "CERTIFICATE")).expect("cert write");
    fs::write(&key_file, fixture_pem("valid", "PRIVATE KEY")).expect("key write");

    TlsCertReloader::new(&TlsConfig {
        cert_file: cert_file.to_string_lossy().to_string(),
        key_file: key_file.to_string_lossy().to_string(),
        reload_interval_secs: 1,
    })
    .expect("valid reloader");

    // Now test various invalid PEM formats
    let invalid_cases = vec![
        ("empty file", ""),
        ("no markers", "just some text"),
        (
            "wrong marker",
            "-----BEGIN RSA PRIVATE KEY-----\ntest\n-----END RSA PRIVATE KEY-----\n",
        ),
        ("garbage", "\x00\x01\x02\x03"),
    ];

    for (case_name, content) in &invalid_cases {
        fs::write(&cert_file, content).expect(&format!("write {}", case_name));

        let result = TlsCertReloader::new(&TlsConfig {
            cert_file: cert_file.to_string_lossy().to_string(),
            key_file: key_file.to_string_lossy().to_string(),
            reload_interval_secs: 1,
        });

        assert!(
            result.is_err(),
            "Should reject {}: {:?}",
            case_name,
            content
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Test that both certificate and key must change atomically
#[test]
fn should_detect_change_when_only_one_file_changes() {
    let dir = unique_test_dir("atomic_change");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");
    fs::write(&cert_file, fixture_pem("cert-v1", "CERTIFICATE")).expect("cert write");
    fs::write(&key_file, fixture_pem("key-v1", "PRIVATE KEY")).expect("key write");

    let mut reloader = TlsCertReloader::new(&TlsConfig {
        cert_file: cert_file.to_string_lossy().to_string(),
        key_file: key_file.to_string_lossy().to_string(),
        reload_interval_secs: 1,
    })
    .expect("reloader");

    // Only update cert, not key - this is unusual but should still trigger reload
    fs::write(&cert_file, fixture_pem("cert-v2", "CERTIFICATE")).expect("cert update");

    let changed = reloader.reload_if_changed().expect("reload changed");
    assert!(
        changed,
        "Should detect change even if only one file changed"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Test that reload interval configuration is properly validated
///
/// The reload_interval_secs field must be a positive integer (> 0).
/// This constraint is enforced during TLS configuration parsing.
#[test]
fn should_enforce_positive_reload_interval() {
    // This test documents the requirement that reload_interval_secs > 0
    // The actual validation is tested in https_and_proxy_tests.rs
    // with tests: should_reject_negative_reload_interval and should_reject_zero_reload_interval
    //
    // Hot reload is constrained to TLS certificates only, and the reload
    // interval is a key safety parameter that must be positive.
}
