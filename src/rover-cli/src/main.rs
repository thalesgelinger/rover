use clap::Command;
use notify::{Event, EventKind, RecursiveMode, Result, Watcher};
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{env, fs, thread};

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
    main: String,
}

#[derive(Debug, Deserialize)]
struct RoverConfig {
    package: Package,
}

fn main() -> Result<()> {
    let matches = Command::new("rover")
        .version("1.0")
        .about("Does awesome things")
        .subcommand(Command::new("init").about("Create new rover project"))
        .subcommand(Command::new("run").about("Run rover project"))
        .get_matches();

    match matches.subcommand() {
        Some(("init", _)) => {
            println!("Creating Project");
            Ok(())
        }
        Some(("run", _)) => run_dev(),
        _ => {
            eprintln!("No valid subcommand was used");
            Ok(())
        }
    }
}

fn run_dev() -> Result<()> {
    let rover_toml = "Rover.toml";

    if fs::metadata(rover_toml).is_err() {
        panic!("Rover.toml not found");
    }

    let contents = fs::read_to_string(rover_toml).expect("Failed to read Rover.toml");

    let config: Arc<RoverConfig> =
        Arc::new(toml::from_str(&contents).expect("Failed to parse Rover.toml"));

    println!("Package Name: {}", config.package.name);
    println!("Version: {}", config.package.version);
    println!("Main: {}", config.package.main);

    let listener = TcpListener::bind("127.0.0.1:4242")?;
    println!("Server listening on port 4242");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let config = Arc::clone(&config);
                thread::spawn(move || {
                    let _ = handle_client(&config.package.name, stream);
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }

    loop {
        thread::park();
    }
}

fn handle_client(package_name: &str, mut stream: TcpStream) -> Result<()> {
    println!("New connection: {}", stream.peer_addr()?);

    let project_path = env::current_dir().expect("Failed to get current dir");

    let mut read_stream = stream.try_clone().expect("Failed to clone stream");

    thread::spawn(move || loop {
        let mut buffer = [0; 512];
        match read_stream.read(&mut buffer) {
            Ok(0) => {
                println!("Client disconnected");
                break;
            }
            Ok(n) => {
                let received_data = String::from_utf8_lossy(&buffer[..n]);
                println!("{}", received_data);
            }
            Err(e) => {
                eprintln!("Failed to read from stream: {}", e);
                break;
            }
        }
    });

    let _ = stream.write_all(format!("##{}##\n", package_name).as_bytes());

    send_lua_files(&project_path, &project_path, &mut stream)?;

    let stream = Arc::new(Mutex::new(stream));

    stream.lock().unwrap().write_all(b"READY")?;

    let stream_clone = Arc::clone(&stream);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event>| match res {
        Ok(event) => {
            if let Some(path) = event.paths.first() {
                if path.extension().map_or(false, |ext| ext == "lua") {
                    match event.kind {
                        EventKind::Create(_) => {
                            let project_path =
                                env::current_dir().expect("Failed to get current dir");
                            let full_file_path =
                                event.paths.first().expect("Failed to get file path");

                            let file_path = full_file_path
                                .strip_prefix(project_path)
                                .expect("Failed to strip file prefix")
                                .to_str()
                                .unwrap()
                                .as_bytes();

                            let file = fs::read(full_file_path).unwrap();
                            let data = [file_path, b"$$", &file].concat();

                            if let Err(e) = stream_clone.lock().unwrap().write_all(&data) {
                                eprintln!("Failed to write to socket: {}", e);
                            }
                        }
                        _ => (),
                    }
                }
            }
        }
        Err(e) => println!("watch error: {:?}", e),
    })
    .expect("Failed to create watcher");

    if let Err(e) = watcher.watch(&project_path, RecursiveMode::Recursive) {
        eprintln!("Failed to start watching directory: {}", e);
    }

    loop {
        thread::park();
    }
}

fn send_lua_files<P: AsRef<Path>>(
    project_path: &PathBuf,
    dir: P,
    stream: &mut TcpStream,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively handle directories
            send_lua_files(project_path, &path, stream)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("lua") {
            // Read the .lua file and send it
            let file = fs::read(&path)?;
            let relative_path = path
                .strip_prefix(&project_path)
                .expect("Failed to strip prefix");
            if let Some(relative_path_str) = relative_path.to_str() {
                let data = [relative_path_str.as_bytes(), b"$$", &file].concat();

                stream.write_all(&data)?;
            }
        }
    }
    Ok(())
}
