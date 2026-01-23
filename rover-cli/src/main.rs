mod check;
mod fmt;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use rover_core::run;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rover")]
struct Cli {
    /// Show verbose output including stack traces
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
        /// Path to Lua file(s) to format. If omitted, formats all .lua files in current directory
        file: Option<PathBuf>,
        /// Check formatting without modifying files
        #[arg(short, long)]
        check: bool,
    },
    /// Database migration commands
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum DbAction {
    /// Run all pending migrations
    Migrate {
        /// Database file path (default: rover.sqlite)
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations directory (default: db/migrations)
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Rollback migrations
    Rollback {
        /// Number of migrations to rollback
        #[arg(short, long, default_value = "1")]
        steps: usize,
        /// Database file path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations directory
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Show migration status
    Status {
        /// Database file path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations directory
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Reset database (drop all tables, re-run migrations)
    Reset {
        /// Database file path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations directory
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

fn main() -> Result<()> {
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
        Commands::External(args) => {
            // Parse args for --yolo and --platform flags
            let (file_args, yolo_mode, platform) = parse_external_args(&args);

            // Check platform availability
            match platform {
                Platform::Stub => {
                    // Default - debug renderer
                }
                Platform::Tui => {
                    println!("Platform 'tui' coming soon!");
                    std::process::exit(0);
                }
                Platform::Web
                | Platform::Ios
                | Platform::Android
                | Platform::Macos
                | Platform::Windows
                | Platform::Linux => {
                    println!("Platform '{:?}' coming soon!", platform);
                    std::process::exit(0);
                }
            }

            let raw_path = file_args.first().ok_or_else(|| {
                anyhow::anyhow!("No file specified. Usage: rover @<file.lua> [--yolo] [--platform <platform>]")
            })?;
            // Strip @ prefix if present (external_subcommand includes it)
            let path = raw_path.strip_prefix('@').unwrap_or(raw_path);
            let file_path = PathBuf::from(path);

            // Run pre-execution check (syntax/type errors)
            check::pre_run_check(&file_path)?;

            // Run database pre-run analysis
            pre_run_db_analysis(&file_path, yolo_mode)?;

            // Execute the file
            match run(path, cli.verbose) {
                Ok(()) => Ok(()),
                Err(_) => {
                    std::process::exit(1);
                }
            }
        }
    }
}

fn handle_db_command(action: DbAction) -> Result<()> {
    match action {
        DbAction::Migrate {
            database,
            migrations,
        } => {
            println!("🗄️  Running migrations...");
            match rover_db::run_migrations(&database, &migrations) {
                Ok(count) => {
                    if count > 0 {
                        println!("✅ Applied {} migration(s)", count);
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Migration failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        DbAction::Rollback {
            steps,
            database,
            migrations,
        } => {
            println!("⏪ Rolling back {} migration(s)...", steps);
            match rover_db::rollback(&database, &migrations, steps) {
                Ok(count) => {
                    if count > 0 {
                        println!("✅ Rolled back {} migration(s)", count);
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Rollback failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        DbAction::Status {
            database,
            migrations,
        } => match rover_db::migration_status(&database, &migrations) {
            Ok(status) => {
                println!("📋 Migration Status\n");

                if status.applied.is_empty() {
                    println!("  Applied: (none)");
                } else {
                    println!("  Applied:");
                    for m in &status.applied {
                        println!("    ✓ {}", m);
                    }
                }

                println!();

                if status.pending.is_empty() {
                    println!("  Pending: (none)");
                } else {
                    println!("  Pending:");
                    for m in &status.pending {
                        println!("    ○ {}", m);
                    }
                }

                Ok(())
            }
            Err(e) => {
                eprintln!("❌ Failed to get status: {}", e);
                std::process::exit(1);
            }
        },
        DbAction::Reset {
            database,
            migrations,
            force,
        } => {
            if !force {
                println!("⚠️  This will DELETE all data in {}!", database);
                println!("   Run with --force to confirm.");
                return Ok(());
            }

            println!("🔄 Resetting database...");

            // Delete the database file
            if std::path::Path::new(&database).exists() {
                std::fs::remove_file(&database)?;
                println!("  Deleted {}", database);
            }

            // Run all migrations
            match rover_db::run_migrations(&database, &migrations) {
                Ok(count) => {
                    println!("✅ Reset complete. Applied {} migration(s)", count);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Reset failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Platform selection for rendering
#[derive(Clone, Debug, PartialEq)]
enum Platform {
    Stub,   // Debug renderer (prints updates)
    Tui,    // Terminal UI (ratatui)
    Web,
    Ios,
    Android,
    Macos,
    Windows,
    Linux,
}

impl Default for Platform {
    fn default() -> Self {
        Platform::Stub
    }
}

impl std::str::FromStr for Platform {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "stub" => Ok(Platform::Stub),
            "tui" => Ok(Platform::Tui),
            "web" => Ok(Platform::Web),
            "ios" => Ok(Platform::Ios),
            "android" => Ok(Platform::Android),
            "macos" => Ok(Platform::Macos),
            "windows" => Ok(Platform::Windows),
            "linux" => Ok(Platform::Linux),
            _ => Err(anyhow::anyhow!("Unknown platform: {}", s)),
        }
    }
}

/// Parse external command args, extracting --yolo and --platform flags
fn parse_external_args(args: &[String]) -> (Vec<String>, bool, Platform) {
    let mut file_args = Vec::new();
    let mut yolo_mode = false;
    let mut platform = Platform::default();
    let mut skip_next = false;

    for (i, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg == "--yolo" || arg == "-y" {
            yolo_mode = true;
        } else if arg == "--platform" || arg == "-p" {
            // Next arg should be the platform value
            if let Some(platform_str) = args.get(i + 1) {
                if let Ok(p) = platform_str.parse() {
                    platform = p;
                }
                skip_next = true;
            }
        } else {
            file_args.push(arg.clone());
        }
    }

    (file_args, yolo_mode, platform)
}

fn pre_run_db_analysis(file_path: &PathBuf, yolo_mode: bool) -> Result<()> {
    use rover_db::{TableStatus, run_pending_migrations};
    use rover_parser::db_intent::analyze_db_intent;

    let code = std::fs::read_to_string(file_path)?;
    let db_path = "rover.sqlite";
    let schemas_dir = PathBuf::from("db/schemas");
    let migrations_dir = PathBuf::from("db/migrations");

    let intent = analyze_db_intent(&code);
    if intent.tables.is_empty() {
        return Ok(());
    }

    println!("\n{}", "🔍 Analyzing code intent...".cyan());

    let schemas = rover_db::load_schemas_from_dir(&schemas_dir).unwrap_or_default();
    let comparison = rover_db::compare_intent_with_schemas(&intent, &schemas);

    let mut needs_migration = false;

    for diff in &comparison.diffs {
        let table = intent.tables.get(&diff.table_name).unwrap();

        match diff.status {
            TableStatus::New => {
                println!(
                    "\n{}",
                    format!("📋 Table '{}' - inferred from code:", diff.table_name).yellow()
                );
                for field in table.fields.values() {
                    println!(
                        "   • {}: {} ({})",
                        field.name,
                        field.field_type.to_guard_type(),
                        field.source
                    );
                }
                println!("\n   {}", "⚠️  No schema found".yellow());

                let create_schema = if yolo_mode {
                    println!("   {}", "Creating schema (--yolo)".cyan());
                    true
                } else {
                    prompt_yn(&format!(
                        "   Create schema db/schemas/{}.lua?",
                        diff.table_name
                    ))?
                };

                if create_schema {
                    let content = rover_db::generate_schema_content(table);
                    rover_db::write_schema_file(&schemas_dir, &diff.table_name, &content)
                        .map_err(|e| anyhow::anyhow!("Failed to write schema: {}", e))?;
                    println!(
                        "   {}",
                        format!("✓ Created db/schemas/{}.lua", diff.table_name).green()
                    );

                    let create_migration = if yolo_mode {
                        true
                    } else {
                        prompt_yn(&format!("   Create migration for '{}'?", diff.table_name))?
                    };

                    if create_migration {
                        let fields: Vec<_> = table.fields.values().cloned().collect();
                        let mig_content =
                            rover_db::generate_migration_content(&diff.table_name, &fields, true);
                        let mig_name = rover_db::write_migration_file(
                            &migrations_dir,
                            &format!("create_{}", diff.table_name),
                            &mig_content,
                        )
                        .map_err(|e| anyhow::anyhow!("Failed to write migration: {}", e))?;
                        println!(
                            "   {}",
                            format!("✓ Created db/migrations/{}", mig_name).green()
                        );
                        needs_migration = true;
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Aborted - schema not created for '{}'",
                        diff.table_name
                    ));
                }
            }
            TableStatus::NeedsUpdate => {
                println!(
                    "\n{}",
                    format!(
                        "📋 Table '{}' - schema exists but code suggests new fields:",
                        diff.table_name
                    )
                    .yellow()
                );
                for field in &diff.new_fields {
                    println!(
                        "   + {}: {} ({})",
                        field.name,
                        field.field_type.to_guard_type(),
                        field.source
                    );
                }

                let update = if yolo_mode {
                    println!("   {}", "Updating schema (--yolo)".cyan());
                    true
                } else {
                    prompt_yn("   Update schema?")?
                };

                if update {
                    rover_db::update_schema_file(&schemas_dir, &diff.table_name, &diff.new_fields)
                        .map_err(|e| anyhow::anyhow!("Failed to update schema: {}", e))?;
                    println!(
                        "   {}",
                        format!("✓ Updated db/schemas/{}.lua", diff.table_name).green()
                    );

                    let create_migration = if yolo_mode {
                        true
                    } else {
                        prompt_yn("   Create migration for changes?")?
                    };

                    if create_migration {
                        let mig_content = rover_db::generate_migration_content(
                            &diff.table_name,
                            &diff.new_fields,
                            false,
                        );
                        let mig_name = rover_db::write_migration_file(
                            &migrations_dir,
                            &format!("add_{}_fields", diff.table_name),
                            &mig_content,
                        )
                        .map_err(|e| anyhow::anyhow!("Failed to write migration: {}", e))?;
                        println!(
                            "   {}",
                            format!("✓ Created db/migrations/{}", mig_name).green()
                        );
                        needs_migration = true;
                    }
                }
            }
            TableStatus::Exists => {
                println!(
                    "\n{}",
                    format!("✅ Table '{}' - schema up to date", diff.table_name).green()
                );
            }
        }
    }

    if needs_migration {
        let run_mig = if yolo_mode {
            println!("\n{}", "Running migrations (--yolo)...".cyan());
            true
        } else {
            prompt_yn("\nRun pending migrations?")?
        };

        if run_mig {
            let conn = rover_db::Connection::new(db_path)
                .map_err(|e| anyhow::anyhow!("DB error: {}", e))?;
            let executor = rover_db::MigrationExecutor::new(std::sync::Arc::new(
                tokio::sync::Mutex::new(conn),
            ));
            executor
                .ensure_migrations_table()
                .map_err(|e| anyhow::anyhow!("Failed to create migrations table: {}", e))?;
            match run_pending_migrations(&executor, &migrations_dir) {
                Ok(count) => println!("{}", format!("✅ Ran {} migration(s)", count).green()),
                Err(e) => return Err(anyhow::anyhow!("Migration failed: {}", e)),
            }
        }
    }

    println!();
    Ok(())
}

fn prompt_yn(message: &str) -> Result<bool> {
    use std::io::BufRead;
    print!("{} [y/n] ", message);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}
