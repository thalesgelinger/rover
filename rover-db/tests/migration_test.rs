use rover_db::{Connection, MigrationExecutor, rollback_migrations, run_pending_migrations};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
fn test_migration_execution() {
    // Create in-memory database
    let db_path = ":memory:";
    let conn = Connection::new(db_path).expect("Failed to create connection");
    let conn = Arc::new(Mutex::new(conn));

    let executor = MigrationExecutor::new(conn.clone());

    // Ensure migrations table exists
    executor
        .ensure_migrations_table()
        .expect("Failed to create migrations table");

    // Create unique migrations directory for this test
    let migrations_dir = Path::new("tests/migrations_test_exec");
    let _ = std::fs::remove_dir_all(migrations_dir);
    std::fs::create_dir_all(migrations_dir).expect("Failed to create migrations dir");

    // Create a simple migration
    let migration_content = r#"
function change()
    migration.users:create({
        name = rover.db.guard:string():required(),
        email = rover.db.guard:string(),
    })
end
"#;

    let migration_file = migrations_dir.join("001_create_users.lua");
    std::fs::write(&migration_file, migration_content).expect("Failed to write migration file");

    // Run migrations
    let result = run_pending_migrations(&executor, migrations_dir);
    assert!(result.is_ok(), "Migration should succeed: {:?}", result);
    let count = result.unwrap();
    assert_eq!(count, 1, "Should run 1 migration");

    // Check migration status
    let status = executor
        .get_status(migrations_dir)
        .expect("Failed to get status");
    assert!(status.applied.contains("001_create_users"));
    assert!(status.pending.is_empty());

    // Verify table exists
    let conn_lock = conn.blocking_lock();
    let schema = conn_lock
        .get_table_schema("users")
        .expect("Failed to get table schema");
    assert_eq!(schema.len(), 3, "Should have 3 columns (id + name + email)");

    // Cleanup
    let _ = std::fs::remove_file(&migration_file);
    // Only remove directory if it's empty
    if std::fs::read_dir(migrations_dir)
        .map(|mut e| e.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(migrations_dir);
    }
}

#[test]
fn test_rollback_migration() {
    // Create in-memory database
    let db_path = ":memory:";
    let conn = Connection::new(db_path).expect("Failed to create connection");
    let conn = Arc::new(Mutex::new(conn));

    let executor = MigrationExecutor::new(conn.clone());

    // Ensure migrations table exists
    executor
        .ensure_migrations_table()
        .expect("Failed to create migrations table");

    // Create unique migrations directory for this test
    let migrations_dir = Path::new("tests/migrations_test_rollback");
    let _ = std::fs::remove_dir_all(migrations_dir);
    std::fs::create_dir_all(migrations_dir).expect("Failed to create migrations dir");

    // Create a simple migration
    let migration_content = r#"
function change()
    migration.users:create({
        name = rover.db.guard:string():required(),
    })
end
"#;

    let migration_file = migrations_dir.join("002_rollback_test.lua");
    std::fs::write(&migration_file, migration_content).expect("Failed to write migration file");

    // Run migration first
    run_pending_migrations(&executor, migrations_dir).expect("Migration should succeed");

    // Verify table exists
    let conn_lock = conn.blocking_lock();
    let schema = conn_lock
        .get_table_schema("users")
        .expect("Failed to get table schema");
    assert_eq!(schema.len(), 2, "Should have 2 columns");
    drop(conn_lock);

    // Rollback
    let result = rollback_migrations(&executor, migrations_dir, 1);
    assert!(result.is_ok(), "Rollback should succeed: {:?}", result);
    let count = result.unwrap();
    assert_eq!(count, 1, "Should rollback 1 migration");

    // Verify table is dropped
    let status = executor
        .get_status(migrations_dir)
        .expect("Failed to get status");
    assert!(status.applied.is_empty());

    // Cleanup
    let _ = std::fs::remove_file(&migration_file);
    if std::fs::read_dir(migrations_dir)
        .map(|mut e| e.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(migrations_dir);
    }
}

#[test]
fn test_migration_status() {
    let db_path = ":memory:";
    let conn = Connection::new(db_path).expect("Failed to create connection");
    let conn = Arc::new(Mutex::new(conn));

    let executor = MigrationExecutor::new(conn.clone());

    executor
        .ensure_migrations_table()
        .expect("Failed to create migrations table");

    // Create unique migrations directory for this test
    let migrations_dir = Path::new("tests/migrations_test_status");
    let _ = std::fs::remove_dir_all(migrations_dir);
    std::fs::create_dir_all(migrations_dir).expect("Failed to create migrations dir");

    // Create multiple migrations
    let migration1 = r#"function change()
    migration.posts:create({title = rover.db.guard:string()})
end"#;
    let migration2 = r#"function change()
    migration.comments:create({body = rover.db.guard:string()})
end"#;

    std::fs::write(migrations_dir.join("003_posts.lua"), migration1).unwrap();
    std::fs::write(migrations_dir.join("004_comments.lua"), migration2).unwrap();

    // Check status - all pending
    let status = executor.get_status(migrations_dir).unwrap();
    assert_eq!(status.available.len(), 2);
    assert_eq!(status.pending.len(), 2);
    assert!(status.applied.is_empty());

    // Run all pending migrations
    let count = run_pending_migrations(&executor, migrations_dir).unwrap();
    assert_eq!(count, 2, "Should run 2 migrations");

    // Check status - all applied, none pending
    let status = executor.get_status(migrations_dir).unwrap();
    assert_eq!(status.available.len(), 2);
    assert_eq!(status.pending.len(), 0);
    assert_eq!(status.applied.len(), 2, "Should have 2 applied migrations");

    // Cleanup
    std::fs::remove_file(migrations_dir.join("003_posts.lua")).ok();
    std::fs::remove_file(migrations_dir.join("004_comments.lua")).ok();
    if migrations_dir.exists() && std::fs::read_dir(migrations_dir).unwrap().next().is_none() {
        std::fs::remove_dir(migrations_dir).ok();
    }
}
