use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

const HOST: &str = "127.0.0.1";
const PORT: u16 = 3000;

struct ServerGuard {
    child: Child,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct WorkerResult {
    latencies_us: Vec<u64>,
    errors: usize,
}

fn parse_usize_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_u64_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn percentile_us(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * (sorted.len().saturating_sub(1) as f64)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn perf_script_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../tests/perf/main.lua")
}

fn start_server() -> ServerGuard {
    let rover_bin = env!("CARGO_BIN_EXE_rover");
    let script = perf_script_path();

    let child = Command::new(rover_bin)
        .arg("run")
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rover server");

    ServerGuard { child }
}

fn send_one_request(stream: &mut TcpStream, req: &[u8], buf: &mut Vec<u8>) -> std::io::Result<u16> {
    stream.write_all(req)?;

    buf.clear();
    let mut headers_end = None;

    while headers_end.is_none() {
        let mut tmp = [0u8; 2048];
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed while reading headers",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_headers_end(buf) {
            headers_end = Some(pos);
        }
    }

    let headers_end = headers_end.expect("headers_end set");
    let status = parse_status_code(buf).unwrap_or(0);
    let content_len = parse_content_length(&buf[..headers_end]).unwrap_or(0);

    let body_in_buf = buf.len().saturating_sub(headers_end);
    if body_in_buf < content_len {
        let to_read = content_len - body_in_buf;
        let mut remaining = vec![0u8; to_read];
        stream.read_exact(&mut remaining)?;
    }

    Ok(status)
}

fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_status_code(buf: &[u8]) -> Option<u16> {
    let line_end = buf.windows(2).position(|w| w == b"\r\n")?;
    let line = std::str::from_utf8(&buf[..line_end]).ok()?;
    let mut parts = line.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse::<u16>().ok()
}

fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let s = std::str::from_utf8(headers).ok()?;
    s.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    })
}

fn wait_until_ready(timeout: Duration) {
    let start = Instant::now();
    let req = b"GET /echo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

    loop {
        if start.elapsed() > timeout {
            panic!("server did not become ready in {:?}", timeout);
        }

        if let Ok(mut stream) = TcpStream::connect((HOST, PORT)) {
            let mut buf = Vec::with_capacity(4096);
            if let Ok(status) = send_one_request(&mut stream, req, &mut buf) {
                if status == 200 {
                    return;
                }
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn run_load(threads: usize, requests_per_thread: usize) -> (Vec<u64>, usize, Duration) {
    let start_barrier = Arc::new(Barrier::new(threads));
    let req = b"GET /echo HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n".to_vec();

    let start = Instant::now();

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let start_barrier = Arc::clone(&start_barrier);
            let req = req.clone();

            thread::spawn(move || {
                let mut result = WorkerResult {
                    latencies_us: Vec::with_capacity(requests_per_thread),
                    errors: 0,
                };

                let mut stream = match TcpStream::connect((HOST, PORT)) {
                    Ok(s) => s,
                    Err(_) => {
                        result.errors = requests_per_thread;
                        return result;
                    }
                };
                let _ = stream.set_nodelay(true);

                let mut buf = Vec::with_capacity(8192);

                start_barrier.wait();

                for _ in 0..requests_per_thread {
                    let t0 = Instant::now();
                    match send_one_request(&mut stream, &req, &mut buf) {
                        Ok(200) => {
                            result.latencies_us.push(t0.elapsed().as_micros() as u64);
                        }
                        Ok(_) => {
                            result.errors += 1;
                        }
                        Err(_) => {
                            result.errors += 1;
                            match TcpStream::connect((HOST, PORT)) {
                                Ok(new_stream) => {
                                    stream = new_stream;
                                    let _ = stream.set_nodelay(true);
                                }
                                Err(_) => {
                                    let remaining = requests_per_thread
                                        .saturating_sub(result.latencies_us.len() + result.errors);
                                    result.errors += remaining;
                                    break;
                                }
                            }
                        }
                    }
                }

                result
            })
        })
        .collect();

    let mut all_latencies = Vec::with_capacity(threads * requests_per_thread);
    let mut total_errors = 0usize;

    for handle in handles {
        let worker = handle.join().expect("worker thread");
        total_errors += worker.errors;
        all_latencies.extend(worker.latencies_us);
    }

    let duration = start.elapsed();
    (all_latencies, total_errors, duration)
}

#[test]
#[ignore = "performance test; run explicitly"]
fn perf_http_echo_regression() {
    let _server = start_server();
    wait_until_ready(Duration::from_secs(10));

    let threads = parse_usize_env("ROVER_PERF_THREADS", 8);
    let req_per_thread = parse_usize_env("ROVER_PERF_REQUESTS_PER_THREAD", 2000);
    let min_rps = parse_u64_env("ROVER_PERF_MIN_RPS", 20_000);
    let max_p99_ms = parse_u64_env("ROVER_PERF_MAX_P99_MS", 15);

    let (mut latencies, errors, duration) = run_load(threads, req_per_thread);
    latencies.sort_unstable();

    let total = (threads * req_per_thread) as u64;
    let ok = (total as usize).saturating_sub(errors) as u64;
    let rps = if duration.as_secs_f64() > 0.0 {
        (ok as f64 / duration.as_secs_f64()) as u64
    } else {
        0
    };

    let mean_us = if !latencies.is_empty() {
        latencies.iter().copied().sum::<u64>() / (latencies.len() as u64)
    } else {
        0
    };
    let p50_us = percentile_us(&latencies, 50.0);
    let p95_us = percentile_us(&latencies, 95.0);
    let p99_us = percentile_us(&latencies, 99.0);

    println!(
        "perf: total={} ok={} errors={} threads={} req_per_thread={} duration_ms={} rps={} mean_us={} p50_us={} p95_us={} p99_us={}",
        total,
        ok,
        errors,
        threads,
        req_per_thread,
        duration.as_millis(),
        rps,
        mean_us,
        p50_us,
        p95_us,
        p99_us
    );

    assert_eq!(errors, 0, "non-200 or network errors detected");
    assert!(rps >= min_rps, "rps={} below min_rps={}", rps, min_rps);
    assert!(
        p99_us <= max_p99_ms * 1000,
        "p99_ms={} above max_p99_ms={}",
        p99_us as f64 / 1000.0,
        max_p99_ms
    );
}
