use anyhow::Result;
use clap::{Parser, Subcommand};
use rover_core::run;

#[derive(Parser)]
#[command(name = "rover")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// its just a sample for other commands
    Sample,
    #[command(external_subcommand)]
    External(Vec<String>),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sample => {
            println!("Just a sample cmd");
            Ok(())
        }
        Commands::External(args) => {
            let path = args.first().unwrap();
            run(path)
        }
    }
}
