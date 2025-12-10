#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use anyhow::{anyhow, Context, Result};
use mlua::Lua;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_PORT: u16 = 9876;
pub const CONFIG_FILE: &str = "rover.lua";
const RELOAD_CMD: &str = "RELOAD\n";
const SYNC_CMD: &str = "SYNC\n";
const ACK_CMD: &str = "ACK\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevConfig {
    pub host: String,
    pub port: u16,
}

/// Simple TCP devserver for hot reload
/// Sends RELOAD command to all connected clients when trigger() is called
pub struct DevServer {
    tx: Sender<SyncMessage>,
    port: u16,
}

enum SyncMessage {
    Reload(HashMap<String, Vec<u8>>),
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

    pub fn trigger(&self, files: HashMap<String, Vec<u8>>) -> Result<()> {
        self.tx
            .send(SyncMessage::Reload(files))
            .context("send reload trigger")
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

fn run_server(listener: TcpListener, rx: Receiver<SyncMessage>, port: u16) -> Result<()> {
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
        if let Ok(SyncMessage::Reload(files)) = rx.try_recv() {
            println!("[devserver] syncing {} files to {} clients", files.len(), clients.len());
            clients.retain_mut(|client| {
                if send_sync(client, &files).is_ok() {
                    true
                } else {
                    println!("[devserver] client disconnected");
                    false
                }
            });
        }

        thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn send_sync(client: &mut TcpStream, files: &HashMap<String, Vec<u8>>) -> Result<()> {
    // Send SYNC header
    client.write_all(SYNC_CMD.as_bytes())?;
    
    // Send file count as u32
    let count = files.len() as u32;
    client.write_all(&count.to_le_bytes())?;
    
    // Send each file
    for (path, content) in files {
        let path_bytes = path.as_bytes();
        client.write_all(&(path_bytes.len() as u32).to_le_bytes())?;
        client.write_all(path_bytes)?;
        client.write_all(&(content.len() as u32).to_le_bytes())?;
        client.write_all(content)?;
    }
    
    client.flush()?;
    Ok(())
}

/// Write dev config to rover.lua
pub fn write_config(dir: &Path, cfg: &DevConfig) -> Result<()> {
    let path = dir.join(CONFIG_FILE);
    let lua_content = format!(
        r#"return {{
  dev = {{
    host = "{}",
    port = {}
  }}
}}
"#,
        cfg.host, cfg.port
    );
    std::fs::write(&path, lua_content)
        .with_context(|| format!("write {}", path.display()))
}

/// Read dev config from rover.lua if present
pub fn read_config(dir: &Path) -> Option<DevConfig> {
    let path = dir.join(CONFIG_FILE);
    if !path.exists() {
        return None;
    }
    
    let lua = Lua::new();
    let content = std::fs::read_to_string(&path).ok()?;
    let config: mlua::Table = lua.load(&content).eval().ok()?;
    
    let dev: mlua::Table = config.get("dev").ok()?;
    let host: String = dev.get("host").ok()?;
    let port: u16 = dev.get("port").ok()?;
    
    Some(DevConfig { host, port })
}

/// Client for connecting to devserver and receiving reload commands
pub struct DevClient {
    stream: Option<TcpStream>,
    last_attempt: std::time::Instant,
    host: String,
    port: u16,
    pending_sync: Option<HashMap<String, Vec<u8>>>,
}

impl DevClient {
    pub fn connect(host: String, port: u16) -> Result<Self> {
        let stream = Self::try_connect(&host, port);
        Ok(Self {
            stream,
            last_attempt: std::time::Instant::now(),
            host,
            port,
            pending_sync: None,
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

        // Temporarily switch to blocking to avoid partial header reads
        stream.set_nonblocking(false).ok();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .ok();

        let mut header = [0u8; 5];
        let read_res = stream.read_exact(&mut header);

        // Restore nonblocking for normal polling
        stream.set_nonblocking(true).ok();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(100)))
            .ok();

        match read_res {
            Ok(()) if &header == SYNC_CMD.as_bytes() => {
                match Self::read_sync(stream) {
                    Ok(files) => {
                        self.pending_sync = Some(files);
                        Ok(true)
                    }
                    Err(e) => {
                        eprintln!("[devclient] sync read failed: {e}");
                        self.stream = None;
                        Ok(false)
                    }
                }
            }
            Ok(()) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(false),
            Err(_) => {
                self.stream = None;
                Ok(false)
            }
        }
    }

    fn read_sync(stream: &mut TcpStream) -> Result<HashMap<String, Vec<u8>>> {
        // Stream already in blocking mode with timeout
        let mut count_buf = [0u8; 4];
        stream.read_exact(&mut count_buf)?;
        let count = u32::from_le_bytes(count_buf);
        
        let mut files = HashMap::new();
        for _ in 0..count {
            let mut path_len_buf = [0u8; 4];
            stream.read_exact(&mut path_len_buf)?;
            let path_len = u32::from_le_bytes(path_len_buf) as usize;
            
            let mut path_buf = vec![0u8; path_len];
            stream.read_exact(&mut path_buf)?;
            let path = String::from_utf8(path_buf)?;
            
            let mut content_len_buf = [0u8; 4];
            stream.read_exact(&mut content_len_buf)?;
            let content_len = u32::from_le_bytes(content_len_buf) as usize;
            
            let mut content = vec![0u8; content_len];
            stream.read_exact(&mut content)?;
            
            files.insert(path, content);
        }
        
        Ok(files)
    }

    pub fn take_sync(&mut self) -> Option<HashMap<String, Vec<u8>>> {
        self.pending_sync.take()
    }

    pub fn ack_reload(&mut self) -> Result<()> {
        if let Some(stream) = &mut self.stream {
            stream.write_all(ACK_CMD.as_bytes()).ok();
            stream.flush().ok();
        }
        Ok(())
    }
}
