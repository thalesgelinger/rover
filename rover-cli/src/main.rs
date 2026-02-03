mod build;
mod check;
mod cli;
mod db;
mod fmt;
mod run;
mod scripts;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::db::handle_db_command;
use crate::run::{run_build_cmd, run_file};
use crate::scripts::{load_rover_scripts, run_script_from_config};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if let Some(script_name) = get_script_name(&args) {
        if let Some(scripts) = load_rover_scripts() {
            if scripts.contains_key(&script_name) {
                let script_args = args[2..].to_vec();
                return run_script_from_config(&script_name, script_args, &scripts);
            }
        }
    }

    let cli = Cli::parse();

    match cli.command {
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
        Commands::Fmt { file, check } => fmt::run_fmt(fmt::FmtOptions { file, check }),
        Commands::Db { action } => handle_db_command(action),
        Commands::Run {
            file,
            yolo,
            platform,
            args,
        } => run_file(file, yolo, platform, args),
        Commands::Build { file, out, target } => run_build_cmd(file, out, target),
    }
}

fn get_script_name(args: &[String]) -> Option<String> {
    if args.len() > 1 {
        let first_arg = &args[1];
        if !first_arg.starts_with('-') {
            return Some(first_arg.clone());
        }
    }
    None
}
