use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use rover_parser::{analyze, ParsingError, SemanticModel};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "rover-check",
    about = "Rover code analyzer and linter for Lua files"
)]
struct Cli {
    /// Path to the Lua file to analyze
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Output format: pretty (default), json
    #[arg(short, long, default_value = "pretty")]
    format: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Read the file
    let code = fs::read_to_string(&cli.file)
        .with_context(|| format!("Failed to read file: {}", cli.file.display()))?;

    // Analyze the code
    let model = analyze(&code);

    // Display results
    match cli.format.as_str() {
        "json" => display_json(&model, &cli.file)?,
        _ => display_pretty(&model, &cli.file, cli.verbose)?,
    }

    // Exit with error code if there are errors
    if !model.errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

fn display_pretty(model: &SemanticModel, file: &PathBuf, verbose: bool) -> Result<()> {
    println!("\n{}", "Analyzing Rover code...".bold().cyan());
    println!("{}", "=".repeat(60).cyan());

    let file_display = file.display().to_string();

    if model.errors.is_empty() {
        println!("\n{}", "✓ No errors found!".green().bold());

        if verbose {
            print_model_summary(model);
        }

        return Ok(());
    }

    // Print errors
    println!(
        "\n{} {} found:\n",
        "✗".red().bold(),
        if model.errors.len() == 1 {
            "error".red()
        } else {
            format!("{} errors", model.errors.len()).red()
        }
    );

    for error in &model.errors {
        display_error(error, &file_display);
    }

    if verbose {
        print_model_summary(model);
    }

    Ok(())
}

fn display_error(error: &ParsingError, file: &str) {
    let error_marker = "error:".red().bold();

    if let Some(range) = &error.range {
        println!(
            "{} {}:{}:{}",
            error_marker,
            file.bright_white(),
            format!("{}", range.start.line + 1).yellow(),
            format!("{}", range.start.column + 1).yellow()
        );
    } else {
        println!("{} {}", error_marker, file.bright_white());
    }

    println!("  {}", error.message.white());

    // Add suggestion if available
    if let Some(suggestion) = get_suggestion(error) {
        println!("  {} {}", "help:".cyan().bold(), suggestion.cyan());
    }

    println!();
}

fn get_suggestion(error: &ParsingError) -> Option<String> {
    let msg = &error.message.to_lowercase();

    if msg.contains("nonexistent") && msg.contains("param") {
        Some("Check that you're accessing the correct parameter name. Available params are defined in your route path.".to_string())
    } else if msg.contains("not found") {
        Some("Ensure the variable or function is defined before use.".to_string())
    } else if msg.contains("guard") {
        Some("Review your guard definition syntax. Guards should follow the pattern: guard.string(), guard.number(), etc.".to_string())
    } else if msg.contains("validation") {
        Some("Check your validation schema for proper structure and type definitions.".to_string())
    } else if msg.contains("route") {
        Some("Verify your route definition follows the pattern: api.path.method(ctx)".to_string())
    } else {
        None
    }
}

fn print_model_summary(model: &SemanticModel) {
    println!("\n{}", "Analysis Summary:".bold().cyan());
    println!("{}", "-".repeat(60).cyan());

    if let Some(server) = &model.server {
        println!("  {} {}", "Server:".bold(), if server.exported { "exported ✓".green() } else { "not exported".yellow() });
        println!("  {} {}", "Routes:".bold(), server.routes.len());

        if !server.routes.is_empty() {
            println!("\n  {}", "Route Details:".bold());
            for route in &server.routes {
                println!("    {} {} {}",
                    route.method.bright_green(),
                    route.path.bright_white(),
                    if !route.responses.is_empty() {
                        format!("({} responses)", route.responses.len()).dimmed()
                    } else {
                        "".dimmed()
                    }
                );

                // Show params info
                if !route.request.path_params.is_empty() {
                    for param in &route.request.path_params {
                        let status = if param.used { "✓".green() } else { "✗ unused".yellow() };
                        println!("      {} param: {} {}", "→".dimmed(), param.name.cyan(), status);
                    }
                }

                if !route.request.query_params.is_empty() {
                    for param in &route.request.query_params {
                        println!("      {} query: {}", "→".dimmed(), param.name.cyan());
                    }
                }

                if !route.request.headers.is_empty() {
                    for header in &route.request.headers {
                        println!("      {} header: {}", "→".dimmed(), header.name.cyan());
                    }
                }
            }
        }
    } else {
        println!("  {} No server definition found", "⚠".yellow());
    }

    println!("  {} {}", "Functions:".bold(), model.functions.len());
}

fn display_json(model: &SemanticModel, file: &PathBuf) -> Result<()> {
    use serde_json::json;

    let errors: Vec<_> = model.errors.iter().map(|e| {
        let mut err = json!({
            "message": e.message,
        });

        if let Some(range) = &e.range {
            err["range"] = json!({
                "start": {
                    "line": range.start.line,
                    "column": range.start.column,
                },
                "end": {
                    "line": range.end.line,
                    "column": range.end.column,
                }
            });
        }

        if let Some(func) = &e.function_name {
            err["function"] = json!(func);
        }

        err
    }).collect();

    let result = json!({
        "file": file.display().to_string(),
        "errors": errors,
        "error_count": model.errors.len(),
        "server_found": model.server.is_some(),
        "routes_count": model.server.as_ref().map_or(0, |s| s.routes.len()),
        "functions_count": model.functions.len(),
    });

    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
