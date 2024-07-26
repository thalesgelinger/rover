use clap::Command;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecursiveMode, Result, Watcher};
use serde::Deserialize;
use std::fs::File;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::str::Bytes;
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
    let matches = Command::new("myapp")
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

    let config: RoverConfig = toml::from_str(&contents).expect("Failed to parse Rover.toml");

    println!("Package Name: {}", config.package.name);
    println!("Version: {}", config.package.version);
    println!("Main: {}", config.package.main);

    let listener = TcpListener::bind("127.0.0.1:4242")?;
    println!("Server listening on port 4242");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    let _ = handle_client(stream);
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

fn handle_client(mut stream: TcpStream) -> Result<()> {
    println!("New connection: {}", stream.peer_addr()?);

    let _ = stream.write_all("CONNECTED\n".as_bytes());

    let project_path = env::current_dir().expect("Failed to get current dir");

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
                                .to_str();

                            let value = format!("FILE NAME: {}\n", file_path.unwrap());

                            if let Err(e) = stream.write_all(value.as_bytes()) {
                                eprintln!("Failed to write to socket: {}", e);
                            }

                            let file = fs::read(full_file_path).unwrap();

                            if let Err(e) = stream.write_all(&file) {
                                eprintln!("Failed to write to socket: {}", e);
                            }
                        }
                        // EventKind::Modify(modify) => match modify {
                        //     ModifyKind::Data(_) => {
                        //         println!("DATA CHANGED: {:?}", file_path);
                        //         let file = fs::read(file_path.unwrap()).unwrap();

                        //         if let Err(e) = stream.write_all(&file) {
                        //             eprintln!("Failed to write to socket: {}", e);
                        //         }
                        //         file_path = None;
                        //     }
                        //     _ => (),
                        // },
                        _ => println!("MODIFY EVENT: {:?}", event),
                    }
                }
            }
        }
        Err(e) => println!("watch error: {:?}", e),
    })?;

    if let Err(e) = watcher.watch(&project_path, RecursiveMode::Recursive) {
        eprintln!("Failed to start watching directory: {}", e);
    }

    loop {
        thread::park();
    }
}
