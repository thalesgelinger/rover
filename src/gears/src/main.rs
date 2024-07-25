use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::thread;

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7878")?;
    println!("Connected to the server");

    // Spawn a thread to handle reading from the server
    let mut read_stream = stream.try_clone()?;
    thread::spawn(move || {
        let mut buffer = [0; 512];
        loop {
            match read_stream.read(&mut buffer) {
                Ok(0) => {
                    println!("Connection closed by server");
                    break;
                }
                Ok(n) => {
                    println!("Received: {}", String::from_utf8_lossy(&buffer[..n]));
                }
                Err(e) => {
                    eprintln!("Failed to read from server: {}", e);
                    break;
                }
            }
        }
    });

    // Main thread to handle writing to the server
    let stdin = io::stdin();
    for line in &stdin.lock().lines() {
        let line = line?;
        stream.write_all(line.as_bytes())?;
    }

    Ok(())
}

