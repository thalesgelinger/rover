use std::{
    io::{self, Read},
    net::TcpStream,
    sync::mpsc::Sender,
};

use regex::Regex;

pub struct DevServer {
    host: String,
}

pub enum ServerMessages {
    Project(String),
    File(DevFile),
    Ready,
}

pub struct DevFile {
    pub path: String,
    pub content: String,
}

impl DevServer {
    pub fn new(host: &str) -> DevServer {
        DevServer {
            host: host.to_string(),
        }
    }

    pub fn listen(&self, tx: &Sender<ServerMessages>) -> io::Result<()> {
        let mut stream = TcpStream::connect(&self.host)?;

        println!("Connected to the server. Listening for incoming messages...");

        let mut buffer = [0; 512];

        // let mut project_path = env::current_dir()?;

        let mut is_ready = false;

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    println!("Connection closed by the server.");
                    break;
                }
                Ok(n) => {
                    let received_data = &buffer[..n];

                    if let Ok(text) = std::str::from_utf8(received_data) {
                        let re = Regex::new(r"##(.*?)##").unwrap();

                        for cap in re.captures_iter(text) {
                            let project_name = &cap[1];
                            tx.send(ServerMessages::Project(project_name.into()))
                                .expect("Failed to send project name")
                        }

                        let parts: Vec<&str> = text.split("$$").collect();

                        if parts.len() == 2 {
                            let path = parts[0].to_string();
                            let content = parts[1].to_string();

                            tx.send(ServerMessages::File(DevFile { path, content }))
                                .expect("Failed to send project name")
                        }

                        if text.contains("READY") {
                            is_ready = true
                        }

                        if is_ready {
                            tx.send(ServerMessages::Ready)
                                .expect("Failed to send ready message to channel");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read from the connection: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}