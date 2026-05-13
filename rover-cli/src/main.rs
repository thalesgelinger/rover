mod build;
mod check;
mod cli;
mod db;
mod fmt;
mod repl;
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

    if let Some(bundle) = load_embedded_bundle() {
        return rover_core::run_from_str(&bundle, &args[1..], false);
    }

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
        Commands::Repl { path, eval } => repl::run_repl(repl::ReplOptions { path, eval }),
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
            device,
            device_id,
            args,
        } => run_file(file, yolo, platform, device, device_id, args),
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

fn load_embedded_bundle() -> Option<String> {
    let exe_path = std::env::current_exe().ok()?;
    let data = std::fs::read(exe_path).ok()?;
    parse_embedded_bundle(&data)
}

fn parse_embedded_bundle(data: &[u8]) -> Option<String> {
    const TRAILER_MAGIC: &[u8] = b"ROVER\n";
    let trailer_start = data
        .windows(TRAILER_MAGIC.len())
        .rposition(|window| window == TRAILER_MAGIC)?;
    let trailer = std::str::from_utf8(&data[trailer_start..]).ok()?;
    let mut parts = trailer.split('\n');
    if parts.next()? != "ROVER" {
        return None;
    }

    let offset: usize = parts.next()?.parse().ok()?;
    let length: usize = parts.next()?.parse().ok()?;
    let end = offset.checked_add(length)?;
    if end > data.len() || end > trailer_start {
        return None;
    }

    String::from_utf8(data[offset..end].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::parse_embedded_bundle;

    #[test]
    fn should_parse_embedded_bundle_trailer() {
        let mut data = b"runtime".to_vec();
        let offset = data.len();
        data.extend_from_slice(b"print('hi')");
        data.extend_from_slice(format!("ROVER\n{}\n{}\n", offset, 11).as_bytes());

        assert_eq!(parse_embedded_bundle(&data).as_deref(), Some("print('hi')"));
    }

    #[test]
    fn should_ignore_invalid_embedded_bundle_trailer() {
        let data = b"runtimeROVER\n999\n1\n";

        assert_eq!(parse_embedded_bundle(data), None);
    }
}
