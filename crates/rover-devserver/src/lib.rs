#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const DEFAULT_PORT: u16 = 9876;
pub const CONFIG_FILE: &str = ".rover_devserver.json";
const RELOAD_CMD: &str = "RELOAD\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevConfig {
    pub host: String,
    pub port: u16,
}

/// Simple TCP devserver for hot reload
/// Sends RELOAD command to all connected clients when trigger() is called
pub struct DevServer {
    tx: Sender<()>,
    port: u16,
}

impl DevServer {
    pub fn start() -> Result<Self> {
        Self::start_with_port(DEFAULT_PORT)
    }

    pub fn start_with_port(base_port: u16) -> Result<Self> {
        let (listener, port) = bind_port(base_port)?;
        let (tx, rx) = channel();
        thread::spawn(move || {
            if let Err(e) = run_server(listener, rx, port) {
                eprintln!("[devserver] error: {e}");
            }
        });
        Ok(Self { tx, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn trigger(&self) -> Result<()> {
        self.tx.send(()).context("send reload trigger")
    }
}

fn bind_port(base: u16) -> Result<(TcpListener, u16)> {
    for p in base..base + 20 {
        match TcpListener::bind(("0.0.0.0", p)) {
            Ok(listener) => return Ok((listener, p)),
            Err(_) => continue,
        }
    }
    Err(anyhow!("could not bind devserver port starting at {base}"))
}

fn run_server(listener: TcpListener, rx: Receiver<()>, port: u16) -> Result<()> {
    listener
        .set_nonblocking(true)
        .context("set nonblocking")?;

    println!("[devserver] listening on 0.0.0.0:{port}");

    let mut clients: Vec<TcpStream> = Vec::new();

    loop {
        // Accept new clients
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("[devserver] client connected: {addr}");
                stream.set_nonblocking(true).ok();
                clients.push(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("[devserver] accept error: {e}"),
        }

        // Check for reload trigger
        if rx.try_recv().is_ok() {
            println!("[devserver] triggering reload for {} clients", clients.len());
            clients.retain_mut(|client| {
                if client.write_all(RELOAD_CMD.as_bytes()).is_ok() {
                    client.flush().is_ok()
                } else {
                    println!("[devserver] client disconnected");
                    false
                }
            });
        }

        thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Write dev config JSON next to entry
pub fn write_config(dir: &Path, cfg: &DevConfig) -> Result<()> {
    let path = dir.join(CONFIG_FILE);
    let data = json!({ "host": cfg.host, "port": cfg.port });
    std::fs::write(&path, serde_json::to_vec_pretty(&data)?)
        .with_context(|| format!("write {}", path.display()))
}

/// Read dev config if present
pub fn read_config(dir: &Path) -> Option<DevConfig> {
    let path = dir.join(CONFIG_FILE);
    let data = std::fs::read(&path).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Client for connecting to devserver and receiving reload commands
pub struct DevClient {
    stream: Option<TcpStream>,
    last_attempt: std::time::Instant,
    host: String,
    port: u16,
}

impl DevClient {
    pub fn connect(host: String, port: u16) -> Result<Self> {
        let stream = Self::try_connect(&host, port);
        Ok(Self {
            stream,
            last_attempt: std::time::Instant::now(),
            host,
            port,
        })
    }

    fn try_connect(host: &str, port: u16) -> Option<TcpStream> {
        let candidates = [
            (host, port),
            ("127.0.0.1", port),
            ("10.0.2.2", port),
            ("host.docker.internal", port),
        ];
        for (h, p) in candidates {            
            if let Ok(stream) = TcpStream::connect((h, p)) {
                stream.set_read_timeout(Some(std::time::Duration::from_millis(100))).ok();
                stream.set_nonblocking(true).ok();
                println!("[devclient] connected to {h}:{p}");
                return Some(stream);
            }
        }
        None
    }

    pub fn check_reload(&mut self) -> Result<bool> {
        // Retry connection every 2s if not connected
        if self.stream.is_none() && self.last_attempt.elapsed().as_secs() >= 2 {
            self.stream = Self::try_connect(&self.host, self.port);
            self.last_attempt = std::time::Instant::now();
        }

        let Some(stream) = &mut self.stream else {
            return Ok(false);
        };

        let mut buf = [0u8; 128];
        match stream.read(&mut buf) {
            Ok(n) if n > 0 => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                Ok(msg.contains("RELOAD"))
            }
            Ok(_) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(false),
            Err(_) => {
                // Connection lost, retry later
                self.stream = None;
                Ok(false)
            }
        }
    }
}
