use anyhow::{Context, Result};
use colored::Colorize;
use rover_parser::{analyze, ParsingError, SemanticModel};
use std::fs;
use std::path::PathBuf;

pub struct CheckOptions {
    pub file: PathBuf,
    pub verbose: bool,
    pub format: OutputFormat,
}

pub enum OutputFormat {
    Pretty,
    Json,
}

pub fn run_check(options: CheckOptions) -> Result<()> {
    // Read the file
    let code = fs::read_to_string(&options.file)
        .with_context(|| format!("Failed to read file: {}", options.file.display()))?;

    // Analyze the code
    let model = analyze(&code);

    // Display results
    match options.format {
        OutputFormat::Json => display_json(&model, &options.file)?,
        OutputFormat::Pretty => display_pretty(&model, &options.file, options.verbose)?,
    }

    // Exit with error code if there are errors
    if !model.errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

/// Run a quick pre-execution check (used before running Lua files)
pub fn pre_run_check(file: &PathBuf) -> Result<bool> {
    // Read the file
    let code = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    // Analyze the code
    let model = analyze(&code);

    let file_display = file.display().to_string();

    // If there are errors or warnings, display them
    if !model.errors.is_empty() {
        println!("{}", "─".repeat(60).dimmed());
        println!(
            "{} {}",
            "Rover Check:".bold().cyan(),
            format!("found {} issue(s)", model.errors.len()).yellow()
        );
        println!("{}", "─".repeat(60).dimmed());

        for error in &model.errors {
            display_error_compact(error, &file_display);
        }

        println!("{}", "─".repeat(60).dimmed());
        println!();

        // Return false to indicate there are issues, but don't exit
        return Ok(false);
    }

    // Show brief success message
    println!("{} {}", "✓".green(), "Code analysis passed".dimmed());
    println!();

    Ok(true)
}

fn display_error_compact(error: &ParsingError, file: &str) {
    if let Some(range) = &error.range {
        println!(
            "  {} {}:{}:{} - {}",
            "✗".red(),
            file.bright_white(),
            format!("{}", range.start.line + 1).yellow(),
            format!("{}", range.start.column + 1).yellow(),
            error.message.white()
        );
    } else {
        println!("  {} {} - {}", "✗".red(), file.bright_white(), error.message.white());
    }

    // Add suggestion if available
    if let Some(suggestion) = get_suggestion(error) {
        println!("    {} {}", "→".cyan(), suggestion.dimmed());
    }
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
        println!(
            "  {} {}",
            "Server:".bold(),
            if server.exported {
                "exported ✓".green()
            } else {
                "not exported".yellow()
            }
        );
        println!("  {} {}", "Routes:".bold(), server.routes.len());

        if !server.routes.is_empty() {
            println!("\n  {}", "Route Details:".bold());
            for route in &server.routes {
                println!(
                    "    {} {} {}",
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
                        let status = if param.used {
                            "✓".green()
                        } else {
                            "✗ unused".yellow()
                        };
                        println!(
                            "      {} param: {} {}",
                            "→".dimmed(),
                            param.name.cyan(),
                            status
                        );
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

    if !model.symbol_specs.is_empty() {
        println!("\n  {}", "Known Symbols:".bold());
        let mut entries: Vec<_> = model.symbol_specs.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let limit = 8usize;
        for (name, spec) in entries.iter().take(limit) {
            let doc_line = spec
                .doc
                .lines()
                .next()
                .unwrap_or("")
                .trim();
            let detail = if doc_line.is_empty() {
                "".to_string()
            } else {
                format!(" — {}", doc_line)
            };
            println!(
                "    {} → {}{}",
                name.cyan(),
                spec.spec_id.bright_white(),
                detail.dimmed()
            );
        }
        if entries.len() > limit {
            println!(
                "    {}",
                format!("… {} more symbols", entries.len() - limit).dimmed()
            );
        }
    }
}

fn display_json(model: &SemanticModel, file: &PathBuf) -> Result<()> {
    use serde_json::json;

    let errors: Vec<_> = model
        .errors
        .iter()
        .map(|e| {
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
        })
        .collect();

    let symbols: Vec<_> = model
        .symbol_specs
        .iter()
        .map(|(name, spec)| {
            json!({
                "name": name,
                "spec": spec.spec_id,
                "doc": spec.doc,
                "members": spec
                    .members
                    .iter()
                    .map(|member| {
                        use rover_parser::MemberKind;
                        json!({
                            "name": member.name,
                            "doc": member.doc,
                            "target": member.target_spec_id,
                            "kind": match member.kind {
                                MemberKind::Field => "Field",
                                MemberKind::Method => "Method",
                            },
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let result = json!({
        "file": file.display().to_string(),
        "errors": errors,
        "error_count": model.errors.len(),
        "server_found": model.server.is_some(),
        "routes_count": model.server.as_ref().map_or(0, |s| s.routes.len()),
        "functions_count": model.functions.len(),
        "symbols_count": model.symbol_specs.len(),
        "symbols": symbols,
        "dynamic_members": model.dynamic_members,
    });

    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
