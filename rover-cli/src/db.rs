use anyhow::Result;

use crate::cli::DbAction;

pub fn handle_db_command(action: DbAction) -> Result<()> {
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
                Err(e) => exit_err(&format!("Migration failed: {}", e)),
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
                Err(e) => exit_err(&format!("Rollback failed: {}", e)),
            }
        }
        DbAction::Status {
            database,
            migrations,
        } => match rover_db::migration_status(&database, &migrations) {
            Ok(status) => {
                println!("📋 Migration Status\n");
                print_status_list(
                    "Applied",
                    &status.applied.iter().cloned().collect::<Vec<_>>(),
                    "✓",
                );
                println!();
                print_status_list(
                    "Pending",
                    &status.pending.iter().cloned().collect::<Vec<_>>(),
                    "○",
                );
                Ok(())
            }
            Err(e) => exit_err(&format!("Failed to get status: {}", e)),
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

            if std::path::Path::new(&database).exists() {
                std::fs::remove_file(&database)?;
                println!("  Deleted {}", database);
            }

            match rover_db::run_migrations(&database, &migrations) {
                Ok(count) => {
                    println!("✅ Reset complete. Applied {} migration(s)", count);
                    Ok(())
                }
                Err(e) => exit_err(&format!("Reset failed: {}", e)),
            }
        }
    }
}

fn print_status_list(label: &str, items: &[String], icon: &str) {
    if items.is_empty() {
        println!("  {}: (none)", label);
    } else {
        println!("  {}:", label);
        for item in items {
            println!("    {} {}", icon, item);
        }
    }
}

fn exit_err(msg: &str) -> Result<()> {
    eprintln!("❌ {}", msg);
    std::process::exit(1);
}
