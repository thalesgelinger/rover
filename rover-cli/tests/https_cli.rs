use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rcgen::generate_simple_self_signed;

const CLIENT_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const FRAME_DATA: u8 = 0x0;
const FRAME_HEADERS: u8 = 0x1;
const FRAME_SETTINGS: u8 = 0x4;
const FLAG_ACK: u8 = 0x1;
const FLAG_END_HEADERS: u8 = 0x4;

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
    let body = run_https_cli(false, &[], "/hello");
    assert_eq!(body, "{\"message\":\"https ok\"}");
}

#[test]
fn should_serve_http2_request_from_cli() {
    let body = run_https_cli(true, &["--http2", "-w", "\n%{http_version}"], "/hello");
    let (body, version) = body.rsplit_once('\n').expect("version marker");
    assert_eq!(body, "{\"message\":\"https ok\"}");
    assert_eq!(version, "2");
}

#[test]
fn should_serve_http2_post_body_from_cli() {
    let body = run_https_cli(
        true,
        &[
            "--http2",
            "-X",
            "POST",
            "-H",
            "Content-Type: text/plain",
            "--data",
            "hello h2",
            "-w",
            "\n%{http_version}",
        ],
        "/echo",
    );
    let (body, version) = body.rsplit_once('\n').expect("version marker");
    assert_eq!(body, "{\"body\":\"hello h2\"}");
    assert_eq!(version, "2");
}

#[test]
fn should_serve_http2_stream_from_cli() {
    let body = run_https_cli(
        true,
        &["--http2", "-w", "\n%{http_version}"],
        "/flow/chunks",
    );
    let (body, version) = body.rsplit_once('\n').expect("version marker");
    assert_eq!(body, "one:two");
    assert_eq!(version, "2");
}

#[test]
fn should_serve_http2_sse_from_cli() {
    let body = run_https_cli(true, &["--http2", "-w", "\n%{http_version}"], "/events");
    let (body, version) = body.rsplit_once('\n').expect("version marker");
    assert_eq!(body, "id:1\nevent:ready\ndata:h2 sse\n\n");
    assert_eq!(version, "2");
}

#[test]
fn should_serve_http2_websocket_from_cli() {
    let messages = run_h2_websocket_cli();
    assert_eq!(
        messages[0],
        "{\"type\":\"welcome\",\"message\":\"h2 ws ready\"}"
    );
    assert_eq!(messages[1], "{\"type\":\"echo\",\"text\":\"hello h2 ws\"}");
}

fn run_https_cli(http2: bool, curl_args: &[&str], path: &str) -> String {
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
    for _ in 0..80 {
        let mut curl = Command::new("curl");
        curl.arg("-skf")
            .args(curl_args)
            .arg(format!("https://127.0.0.1:{}{}", port, path));
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

fn run_h2_websocket_cli() -> Vec<String> {
    let dir = unique_test_dir("h2_ws");
    fs::create_dir_all(&dir).expect("mkdir");

    let cert = generate_simple_self_signed(["localhost".to_string()]).expect("cert");
    let cert_file = dir.join("cert.pem");
    let key_file = dir.join("key.pem");
    let log_file = dir.join("server.log");
    fs::write(&cert_file, cert.cert.pem()).expect("cert write");
    fs::write(&key_file, cert.key_pair.serialize_pem()).expect("key write");

    let port = free_port();
    let log = fs::File::create(&log_file).expect("log");
    let mut server = Command::new(env!("CARGO_BIN_EXE_rover"))
        .arg("run")
        .arg("../examples/https_e2e.lua")
        .env("ROVER_E2E_PORT", port.to_string())
        .env("ROVER_E2E_CERT", &cert_file)
        .env("ROVER_E2E_KEY", &key_file)
        .env("ROVER_E2E_HTTP2", "1")
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn rover");

    for _ in 0..80 {
        let output = Command::new("curl")
            .arg("-skf")
            .arg("--http2")
            .arg(format!("https://127.0.0.1:{}/hello", port))
            .output()
            .expect("curl");
        if output.status.success() {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }

    let messages = h2_ws_round_trip(port);
    let _ = server.kill();
    let _ = server.wait();
    let _ = fs::remove_dir_all(&dir);
    messages
}

fn h2_ws_round_trip(port: u16) -> Vec<String> {
    let mut child = Command::new("openssl")
        .arg("s_client")
        .arg("-quiet")
        .arg("-alpn")
        .arg("h2")
        .arg("-servername")
        .arg("localhost")
        .arg("-connect")
        .arg(format!("127.0.0.1:{}", port))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("openssl s_client");

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = child.stdout.take().expect("stdout");

    stdin.write_all(CLIENT_PREFACE).expect("preface");
    stdin
        .write_all(&h2_frame(FRAME_SETTINGS, 0, 0, &[0, 8, 0, 0, 0, 1]))
        .expect("settings");

    let mut encoder = hpack::Encoder::new();
    let headers = encoder.encode([
        (b":method".as_slice(), b"CONNECT".as_slice()),
        (b":scheme".as_slice(), b"https".as_slice()),
        (b":authority".as_slice(), b"localhost".as_slice()),
        (b":path".as_slice(), b"/echo".as_slice()),
        (b":protocol".as_slice(), b"websocket".as_slice()),
        (b"sec-websocket-version".as_slice(), b"13".as_slice()),
        (
            b"sec-websocket-key".as_slice(),
            b"dGhlIHNhbXBsZSBub25jZQ==".as_slice(),
        ),
    ]);
    stdin
        .write_all(&h2_frame(FRAME_HEADERS, FLAG_END_HEADERS, 1, &headers))
        .expect("headers");
    stdin.flush().expect("flush");

    let mut decoder = hpack::Decoder::new();
    let mut messages = Vec::new();
    loop {
        let frame = read_h2_frame(&mut stdout).expect("read frame");
        match frame.kind {
            FRAME_SETTINGS if frame.flags & FLAG_ACK == 0 => stdin
                .write_all(&h2_frame(FRAME_SETTINGS, FLAG_ACK, 0, &[]))
                .expect("settings ack"),
            FRAME_HEADERS if frame.stream_id == 1 => {
                let decoded = decoder.decode(&frame.payload).expect("hpack decode");
                assert!(
                    decoded
                        .iter()
                        .any(|(name, value)| name == b":status" && value == b"200")
                );
            }
            FRAME_DATA if frame.stream_id == 1 => {
                messages.push(read_ws_text(&frame.payload));
                if messages.len() == 1 {
                    let request = ws_text_frame(b"{\"type\":\"echo\",\"text\":\"hello h2 ws\"}");
                    stdin
                        .write_all(&h2_frame(FRAME_DATA, 0, 1, &request))
                        .expect("ws data");
                    stdin.flush().expect("flush ws");
                } else {
                    break;
                }
            }
            _ => {}
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    messages
}

struct H2Frame {
    kind: u8,
    flags: u8,
    stream_id: u32,
    payload: Vec<u8>,
}

fn h2_frame(kind: u8, flags: u8, stream_id: u32, payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut out = Vec::with_capacity(9 + len);
    out.push(((len >> 16) & 0xff) as u8);
    out.push(((len >> 8) & 0xff) as u8);
    out.push((len & 0xff) as u8);
    out.push(kind);
    out.push(flags);
    out.extend_from_slice(&(stream_id & 0x7fff_ffff).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn read_h2_frame(reader: &mut impl Read) -> std::io::Result<H2Frame> {
    let mut header = [0u8; 9];
    reader.read_exact(&mut header)?;
    let len = ((header[0] as usize) << 16) | ((header[1] as usize) << 8) | header[2] as usize;
    let mut payload = vec![0; len];
    reader.read_exact(&mut payload)?;
    Ok(H2Frame {
        kind: header[3],
        flags: header[4],
        stream_id: u32::from_be_bytes([header[5], header[6], header[7], header[8]]) & 0x7fff_ffff,
        payload,
    })
}

fn ws_text_frame(payload: &[u8]) -> Vec<u8> {
    let mask = [1u8, 2, 3, 4];
    let mut out = Vec::with_capacity(payload.len() + 6);
    out.push(0x81);
    out.push(0x80 | payload.len() as u8);
    out.extend_from_slice(&mask);
    for (idx, byte) in payload.iter().enumerate() {
        out.push(byte ^ mask[idx % 4]);
    }
    out
}

fn read_ws_text(frame: &[u8]) -> String {
    assert_eq!(frame[0] & 0x0f, 1);
    let masked = frame[1] & 0x80 != 0;
    let len = (frame[1] & 0x7f) as usize;
    let payload_start = if masked { 6 } else { 2 };
    let mut payload = frame[payload_start..payload_start + len].to_vec();
    if masked {
        let mask = &frame[2..6];
        for (idx, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[idx % 4];
        }
    }
    String::from_utf8(payload).expect("ws text")
}
