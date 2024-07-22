use clap::Command;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
    main: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    package: Package,
}

fn main() {
    let matches = Command::new("myapp")
        .version("1.0")
        .about("Does awesome things")
        .subcommand(Command::new("init").about("Create new rover project"))
        .subcommand(
            Command::new("run").about("Run rover project"), // .arg(Arg::new("file").help("the file to remove").required(true).index(1)),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("init", _)) => {
            println!("Creating Project");
        }
        Some(("run", _)) => {
            let rover_toml = "Rover.toml";

            if fs::metadata(rover_toml).is_err() {
                panic!("Rover.toml not found");
            }
            let contents = fs::read_to_string(rover_toml).expect("Failed to read Rover.toml");

            let config: Config = toml::from_str(&contents).expect("Failed to parse Rover.toml");

            println!("Package Name: {}", config.package.name);
            println!("Version: {}", config.package.version);
            println!("Main: {}", config.package.main);
        }
        _ => eprintln!("No valid subcommand was used"),
    }
}
