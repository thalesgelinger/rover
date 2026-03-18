use anyhow::Result;
use colored::Colorize;
use rover_core::register_extra_modules;
use rover_db::run_pending_migrations;
use rover_tui::{TuiRenderer, TuiRunner};
use rover_ui::app::App;
use rover_ui::ui::StubRenderer;
use rover_web::{WebServerOptions, serve_static};
use serde_json::json;
use std::fs;
use std::io::BufRead;
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

use crate::build::{BuildOptions, run_build};
use crate::cli::Platform;

pub fn run_file(
    file: PathBuf,
    yolo: bool,
    platform: Option<Platform>,
    args: Vec<String>,
) -> Result<()> {
    pre_run_db_analysis(&file, yolo)?;

    match platform {
        None => rover_core::run(file.to_str().unwrap(), &args, false),
        Some(Platform::Stub) => run_with_stub(file, args),
        Some(Platform::Tui) => run_with_tui(file, args),
        Some(Platform::Web) => run_with_web(file, args),
        Some(platform) => {
            println!("Platform '{}' coming soon!", platform);
            std::process::exit(0);
        }
    }
}

fn set_lua_args(lua: &mlua::Lua, file: &PathBuf, args: &[String]) -> Result<()> {
    let arg_table = lua.create_table()?;
    arg_table.set(0, file.to_string_lossy().to_string())?;
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i + 1, arg.as_str())?;
    }
    arg_table.set(-1, "rover")?;
    lua.globals().set("arg", arg_table)?;
    Ok(())
}

fn run_with_stub(file: PathBuf, args: Vec<String>) -> Result<()> {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).map_err(|e| anyhow::anyhow!("Failed to create app: {}", e))?;
    register_extra_modules(app.lua())?;
    set_lua_args(app.lua(), &file, &args)?;
    let content = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
    app.run_script(&content)
        .map_err(|e| anyhow::anyhow!("Script error: {}", e))?;
    app.run().map_err(|e| anyhow::anyhow!("App error: {}", e))?;
    Ok(())
}

fn run_with_tui(file: PathBuf, args: Vec<String>) -> Result<()> {
    let renderer =
        TuiRenderer::new().map_err(|e| anyhow::anyhow!("Failed to create TUI renderer: {}", e))?;
    let app = App::new(renderer).map_err(|e| anyhow::anyhow!("Failed to create app: {}", e))?;
    let mut runner = TuiRunner::new(app);
    register_extra_modules(runner.app().lua())?;
    set_lua_args(runner.app().lua(), &file, &args)?;
    let content = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
    runner
        .app_mut()
        .run_script(&content)
        .map_err(|e| anyhow::anyhow!("Script error: {}", e))?;
    runner
        .run()
        .map_err(|e| anyhow::anyhow!("TUI error: {}", e))?;
    Ok(())
}

fn run_with_web(file: PathBuf, _args: Vec<String>) -> Result<()> {
    let web = prepare_web_root(&file)?;

    println!("Starting rover web UI runtime on http://127.0.0.1:4242");
    println!("Assets: {}", web.root.display());

    serve_static(WebServerOptions {
        root_dir: web.root,
        source_root: web.source_root,
        source_files: web.source_files,
        host: "127.0.0.1".to_string(),
        port: 4242,
    })
}

struct PreparedWeb {
    root: PathBuf,
    source_root: PathBuf,
    source_files: Vec<String>,
}

fn prepare_web_root(lua_file: &Path) -> Result<PreparedWeb> {
    let root = PathBuf::from(".rover/web");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;

    extract_embedded_assets(&root)?;

    let cwd = std::env::current_dir()?;
    let entry_abs = if lua_file.is_absolute() {
        lua_file.to_path_buf()
    } else {
        cwd.join(lua_file)
    };

    let source_root = cwd;

    let mut files = Vec::new();
    collect_lua_files(&source_root, &source_root, &mut files)?;

    let entry_rel = entry_abs
        .strip_prefix(&source_root)
        .map_err(|e| anyhow::anyhow!("Failed to compute entry relative path: {}", e))?
        .to_string_lossy()
        .replace('\\', "/");

    if !files.iter().any(|f| f == &entry_rel) {
        files.push(entry_rel.clone());
    }

    files.sort();
    files.dedup();

    let manifest = json!({
        "entry": entry_rel,
        "files": files,
        "source_prefix": "/__rover_src",
    });
    fs::write(root.join("manifest.json"), serde_json::to_vec(&manifest)?)?;

    Ok(PreparedWeb {
        root,
        source_root,
        source_files: files,
    })
}

fn collect_lua_files(base: &Path, current: &Path, files: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(name, ".git" | "target" | ".rover" | "node_modules") {
                    continue;
                }
            }
            collect_lua_files(base, &path, files)?;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("lua") {
            continue;
        }

        let rel = path
            .strip_prefix(base)
            .map_err(|e| anyhow::anyhow!("Failed to compute relative path: {}", e))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        files.push(rel_str);
    }

    Ok(())
}

fn extract_embedded_assets(dest: &Path) -> Result<()> {
    static WEB_ASSETS_TAR_GZ: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/rover_web_assets.tar.gz"));

    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(WEB_ASSETS_TAR_GZ));
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest)?;
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

    // Check which tables have at least one migration
    let tables_with_migrations = get_tables_with_migrations(&migrations_dir)?;

    let schemas = rover_db::load_schemas_from_dir(&schemas_dir)
        .map_err(|e| anyhow::anyhow!("Failed to load schemas: {}", e))?;
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
                    } else {
                        return Err(anyhow::anyhow!(
                            "Aborted - migration needed for '{}' but not created. Generate migration manually or say yes to create.",
                            diff.table_name
                        ));
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
                // Check if there's at least one migration for this table
                if !tables_with_migrations.contains(&diff.table_name) {
                    println!(
                        "   {}",
                        format!("⚠️  No migration found for '{}'", diff.table_name).yellow()
                    );

                    let create_migration = confirm_or_yolo(
                        yolo_mode,
                        &format!("   Create migration for '{}'?", diff.table_name),
                    )?;

                    if create_migration {
                        let table = intent.tables.get(&diff.table_name).unwrap();
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
                    } else {
                        return Err(anyhow::anyhow!(
                            "Aborted - migration needed for '{}' but not created. Generate migration manually or say yes to create.",
                            diff.table_name
                        ));
                    }
                }
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

/// Get set of table names that have at least one migration file
fn get_tables_with_migrations(
    migrations_dir: &PathBuf,
) -> Result<std::collections::HashSet<String>> {
    let mut tables = std::collections::HashSet::new();

    if !migrations_dir.exists() {
        return Ok(tables);
    }

    let entries = std::fs::read_dir(migrations_dir)
        .map_err(|e| anyhow::anyhow!("Failed to read migrations dir: {}", e))?;

    for entry in entries.flatten() {
        if let Ok(name) = entry.file_name().into_string() {
            if name.ends_with(".lua") {
                // Extract table name from migration filename
                // Format: 001_create_users.lua -> users
                // Format: 002_add_users_fields.lua -> users
                if let Some(table_name) = extract_table_from_migration(&name) {
                    tables.insert(table_name);
                }
            }
        }
    }

    Ok(tables)
}

/// Extract table name from migration filename
fn extract_table_from_migration(filename: &str) -> Option<String> {
    // Remove .lua extension
    let name = filename.strip_suffix(".lua")?;

    // Remove number prefix
    let name = name.split('_').skip(1).collect::<Vec<_>>().join("_");

    // Handle patterns like:
    // - create_users -> users
    // - add_users_fields -> users
    // - users -> users
    let table_name = name
        .replace("create_", "")
        .replace("add_", "")
        .split('_')
        .next()?
        .to_string();

    Some(table_name)
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
