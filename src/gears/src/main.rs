use std::{
    io::{self, Read},
    net::TcpStream,
};

fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:4242")?;

    println!("Connected to the server. Listening for incoming messages...");

    let mut buffer = [0; 512];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("Connection closed by the server.");
                break;
            }
            Ok(n) => {
                let received_data = &buffer[..n];
                if let Ok(text) = std::str::from_utf8(received_data) {
                    println!("Received: {}", text);
                } else {
                    println!("Received (binary data): {:?}", received_data);
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

