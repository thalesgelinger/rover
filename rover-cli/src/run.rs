use anyhow::Result;
use colored::Colorize;
use rover_core::register_extra_modules;
use rover_db::run_pending_migrations;
use rover_ui::app::App;
use rover_ui::ui::StubRenderer;
use std::io::BufRead;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::build::{BuildOptions, run_build};
use crate::check;
use crate::cli::Platform;

pub fn run_file(
    file: PathBuf,
    yolo: bool,
    platform: Option<Platform>,
    args: Vec<String>,
) -> Result<()> {
    check::pre_run_check(&file)?;
    pre_run_db_analysis(&file, yolo)?;

    match platform {
        None => rover_core::run(file.to_str().unwrap(), &args, false),
        Some(Platform::Stub) => run_with_stub(file),
        Some(platform) => {
            println!("Platform '{}' coming soon!", platform);
            std::process::exit(0);
        }
    }
}

fn run_with_stub(file: PathBuf) -> Result<()> {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).map_err(|e| anyhow::anyhow!("Failed to create app: {}", e))?;
    register_extra_modules(app.lua())?;
    let content = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
    app.run_script(&content)
        .map_err(|e| anyhow::anyhow!("Script error: {}", e))?;
    app.run().map_err(|e| anyhow::anyhow!("App error: {}", e))?;
    Ok(())
}

pub fn pre_run_db_analysis(file_path: &PathBuf, yolo_mode: bool) -> Result<()> {
    use rover_db::TableStatus;
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

                    let create_migration = confirm_or_yolo(
                        yolo_mode,
                        &format!("   Create migration for '{}'?", diff.table_name),
                    )?;

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

                    let create_migration =
                        confirm_or_yolo(yolo_mode, "   Create migration for changes?")?;

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
            run_pending_migrations_or_exit(db_path, &migrations_dir)?;
        }
    }

    println!();
    Ok(())
}

fn confirm_or_yolo(yolo_mode: bool, msg: &str) -> Result<bool> {
    if yolo_mode { Ok(true) } else { prompt_yn(msg) }
}

fn run_pending_migrations_or_exit(db_path: &str, migrations_dir: &PathBuf) -> Result<()> {
    let conn =
        rover_db::Connection::new(db_path).map_err(|e| anyhow::anyhow!("DB error: {}", e))?;
    let executor =
        rover_db::MigrationExecutor::new(std::sync::Arc::new(tokio::sync::Mutex::new(conn)));
    executor
        .ensure_migrations_table()
        .map_err(|e| anyhow::anyhow!("Failed to create migrations table: {}", e))?;
    match run_pending_migrations(&executor, migrations_dir) {
        Ok(count) => println!("{}", format!("✅ Ran {} migration(s)", count).green()),
        Err(e) => return Err(anyhow::anyhow!("Migration failed: {}", e)),
    }
    Ok(())
}

fn prompt_yn(message: &str) -> Result<bool> {
    print!("{} [y/n] ", message);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

pub fn run_build_cmd(file: PathBuf, out: Option<PathBuf>, target: Option<String>) -> Result<()> {
    run_build(BuildOptions {
        entrypoint: file,
        output: out,
        target,
    })
}
