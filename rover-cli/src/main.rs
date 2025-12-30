mod check;
mod fmt;

use anyhow::Result;
use clap::{Parser, Subcommand};
use rover_core::run;
use std::path::PathBuf;

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
    /// Start the Rover LSP server
    Lsp,
    /// Analyze and check Rover Lua code for errors and warnings
    Check {
        /// Path to the Lua file to analyze
        file: PathBuf,
        /// Show verbose output with detailed analysis
        #[arg(short, long)]
        verbose: bool,
        /// Output format: pretty (default), json
        #[arg(short, long, default_value = "pretty")]
        format: String,
    },
    /// Format Lua code
    Fmt {
        /// Path to the Lua file to format
        file: PathBuf,
        /// Check formatting without modifying files
        #[arg(short, long)]
        check: bool,
    },
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
        Commands::Lsp => {
            rover_lsp::start_server();
            Ok(())
        }
        Commands::Check {
            file,
            verbose,
            format,
        } => {
            let output_format = match format.as_str() {
                "json" => check::OutputFormat::Json,
                _ => check::OutputFormat::Pretty,
            };
            check::run_check(check::CheckOptions {
                file,
                verbose,
                format: output_format,
            })
        }
        Commands::Fmt {
            file,
            check,
        } => {
            fmt::run_fmt(fmt::FmtOptions {
                file,
                check,
            })
        }
        Commands::External(args) => {
            let path = args.first().unwrap();
            let file_path = PathBuf::from(path);
            
            // Run pre-execution check
            check::pre_run_check(&file_path)?;
            
            // Execute the file
            run(path)
        }
    }
}
