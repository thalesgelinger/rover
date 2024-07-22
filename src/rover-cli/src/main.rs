use clap::{Command};
fn main() {
    let matches = Command::new("myapp")
        .version("1.0")
        .about("Does awesome things")
        .subcommand(
            Command::new("init")
                .about("Create new rover project")
        )
        .subcommand(
            Command::new("run")
                .about("Run rover project")
                // .arg(Arg::new("file").help("the file to remove").required(true).index(1)),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("init", _)) => {
            println!("Creating Project");
        }
        Some(("run", _)) => {
            println!("Running project");
        }
        _ => eprintln!("No valid subcommand was used"),
    }
}

