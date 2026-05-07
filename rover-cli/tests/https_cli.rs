use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rcgen::generate_simple_self_signed;

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("rover_cli_{}_{}", name, nanos))
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind free port");
    listener.local_addr().expect("addr").port()
}

#[test]
fn should_serve_https_request_from_cli() {
    let body = run_https_cli(false, &[]);
    assert_eq!(body, "{\"message\":\"https ok\"}");
}

#[test]
fn should_serve_http2_request_from_cli() {
    let body = run_https_cli(true, &["--http2", "-w", "\n%{http_version}"]);
    let (body, version) = body.rsplit_once('\n').expect("version marker");
    assert_eq!(body, "{\"message\":\"https ok\"}");
    assert_eq!(version, "2");
}

fn run_https_cli(http2: bool, curl_args: &[&str]) -> String {
    let dir = unique_test_dir("https");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert = generate_simple_self_signed(["localhost".to_string()]).expect("cert");
    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");
    let log_file = dir.join("server.log");
    fs::write(&cert_file, cert.cert.pem()).expect("cert write");
    fs::write(&key_file, cert.key_pair.serialize_pem()).expect("key write");

    let port = free_port();
    let log = fs::File::create(&log_file).expect("log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_rover"))
        .arg("run")
        .arg("../examples/https_e2e.lua")
        .env("ROVER_E2E_PORT", port.to_string())
        .env("ROVER_E2E_CERT", &cert_file)
        .env("ROVER_E2E_KEY", &key_file)
        .env("ROVER_E2E_HTTP2", if http2 { "1" } else { "0" })
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn rover");

    let mut body = None;
    for _ in 0..20 {
        let mut curl = Command::new("curl");
        curl.arg("-skf")
            .args(curl_args)
            .arg(format!("https://127.0.0.1:{}/hello", port));
        let output = curl.output().expect("curl");

        if output.status.success() {
            body = Some(String::from_utf8(output.stdout).expect("utf8"));
            break;
        }

        thread::sleep(Duration::from_millis(250));
    }

    let _ = child.kill();
    let _ = child.wait();

    let _ = fs::remove_dir_all(&dir);
    body.expect("https response")
}
