use clap::Command;
use serde::Deserialize;
use std::error::Error;
use std::{env, fs};
use warp::Filter;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
            Ok(())
        }
        Some(("run", _)) => run_dev().await,
        _ => {
            eprintln!("No valid subcommand was used");
            Ok(())
        }
    }
}

async fn run_dev() -> Result<(), Box<dyn Error>> {
    let rover_toml = "Rover.toml";

    if fs::metadata(rover_toml).is_err() {
        panic!("Rover.toml not found");
    }
    let contents = fs::read_to_string(rover_toml).expect("Failed to read Rover.toml");

    let config: Config = toml::from_str(&contents).expect("Failed to parse Rover.toml");

    println!("Package Name: {}", config.package.name);
    println!("Version: {}", config.package.version);
    println!("Main: {}", config.package.main);

    let project_path = env::current_dir()?;

    let route = warp::path("rover").and(warp::fs::dir(project_path));

    warp::serve(route).run(([127, 0, 0, 1], 4242)).await;
    println!("Dev server running at: 4242");
    Ok(())
}
