use std::{
    env,
    fs::{self},
    io::{self, Read},
    net::TcpStream,
    path::Path,
};

use regex::Regex;

pub struct DevServer {}

impl DevServer {
    pub fn new() -> DevServer {
        DevServer {}
    }

    pub fn listen<F: Fn()>(&self, cb: F) -> io::Result<()> {
        let mut stream = TcpStream::connect("127.0.0.1:4242")?;

        println!("Connected to the server. Listening for incoming messages...");

        let mut buffer = [0; 512];

        let mut project_path = env::current_dir().expect("Error getting current dir");

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
                            fs::create_dir_all(project_name)?;
                            project_path = project_path.join(project_name);
                        }

                        let parts: Vec<&str> = text.split("$$").collect();

                        if parts.len() == 2 {
                            let file_path = parts[0];
                            let file_content = parts[1];
                            let full_path = Path::new(&project_path).join(file_path);

                            if let Some(parent) = full_path.parent() {
                                fs::create_dir_all(parent)?;
                            }

                            fs::write(full_path, file_content)?;
                        }

                        cb()
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
