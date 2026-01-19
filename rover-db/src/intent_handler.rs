use crate::schema_analyzer::TableDefinition;
use rover_parser::db_intent::{DbIntent, FieldType as InferredType, InferredField, InferredTable};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

#[derive(Debug)]
pub struct TableDiff {
    pub table_name: String,
    pub status: TableStatus,
    pub new_fields: Vec<InferredField>,
}

#[derive(Debug)]
pub enum TableStatus {
    New,
    Exists,
    NeedsUpdate,
}

pub struct IntentComparison {
    pub diffs: Vec<TableDiff>,
}

pub fn compare_intent_with_schemas(
    intent: &DbIntent,
    schemas: &HashMap<String, TableDefinition>,
) -> IntentComparison {
    let mut diffs = Vec::new();

    for (table_name, inferred_table) in &intent.tables {
        if let Some(existing) = schemas.get(table_name) {
            let new_fields = find_new_fields(inferred_table, existing);
            if new_fields.is_empty() {
                diffs.push(TableDiff {
                    table_name: table_name.clone(),
                    status: TableStatus::Exists,
                    new_fields: vec![],
                });
            } else {
                diffs.push(TableDiff {
                    table_name: table_name.clone(),
                    status: TableStatus::NeedsUpdate,
                    new_fields,
                });
            }
        } else {
            let new_fields: Vec<_> = inferred_table.fields.values().cloned().collect();
            diffs.push(TableDiff {
                table_name: table_name.clone(),
                status: TableStatus::New,
                new_fields,
            });
        }
    }

    IntentComparison { diffs }
}

fn find_new_fields(inferred: &InferredTable, existing: &TableDefinition) -> Vec<InferredField> {
    let existing_names: std::collections::HashSet<_> =
        existing.fields.iter().map(|f| f.name.as_str()).collect();

    inferred
        .fields
        .values()
        .filter(|f| !existing_names.contains(f.name.as_str()))
        .cloned()
        .collect()
}

pub fn generate_schema_content(table: &InferredTable) -> String {
    let mut lines = Vec::new();
    lines.push(format!("rover.db.schema.{} {{", table.name));

    let mut fields: Vec<_> = table.fields.values().collect();
    fields.sort_by(|a, b| {
        if a.name == "id" {
            std::cmp::Ordering::Less
        } else if b.name == "id" {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    for field in fields {
        let guard_type = field.field_type.to_guard_type();
        let modifiers = if field.name == "id" {
            ":primary():auto()"
        } else {
            ""
        };
        lines.push(format!("    {} = guard:{}(){},", field.name, guard_type, modifiers));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

pub fn generate_migration_content(table_name: &str, fields: &[InferredField], is_create: bool) -> String {
    let mut lines = Vec::new();

    if is_create {
        lines.push(format!("-- Create {} table", table_name));
        lines.push("return {".to_string());
        lines.push("    up = function(db)".to_string());
        lines.push(format!("        db:execute([["));
        lines.push(format!("            CREATE TABLE {} (", table_name));

        let mut field_lines = Vec::new();
        let mut sorted_fields: Vec<_> = fields.iter().collect();
        sorted_fields.sort_by(|a, b| {
            if a.name == "id" {
                std::cmp::Ordering::Less
            } else if b.name == "id" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        for field in sorted_fields {
            let sql_type = inferred_to_sql_type(&field.field_type);
            let constraints = if field.name == "id" {
                " PRIMARY KEY AUTOINCREMENT"
            } else {
                ""
            };
            field_lines.push(format!("                {} {}{}", field.name, sql_type, constraints));
        }

        lines.push(field_lines.join(",\n"));
        lines.push("            )".to_string());
        lines.push("        ]])".to_string());
        lines.push("    end,".to_string());
        lines.push("".to_string());
        lines.push("    down = function(db)".to_string());
        lines.push(format!("        db:execute(\"DROP TABLE IF EXISTS {}\")", table_name));
        lines.push("    end".to_string());
        lines.push("}".to_string());
    } else {
        lines.push(format!("-- Add fields to {}", table_name));
        lines.push("return {".to_string());
        lines.push("    up = function(db)".to_string());
        for field in fields {
            let sql_type = inferred_to_sql_type(&field.field_type);
            lines.push(format!(
                "        db:execute(\"ALTER TABLE {} ADD COLUMN {} {}\")",
                table_name, field.name, sql_type
            ));
        }
        lines.push("    end,".to_string());
        lines.push("".to_string());
        lines.push("    down = function(db)".to_string());
        lines.push("        -- SQLite doesn't support DROP COLUMN easily".to_string());
        lines.push("    end".to_string());
        lines.push("}".to_string());
    }

    lines.join("\n")
}

fn inferred_to_sql_type(t: &InferredType) -> &'static str {
    match t {
        InferredType::Integer => "INTEGER",
        InferredType::Number => "REAL",
        InferredType::String => "TEXT",
        InferredType::Boolean => "INTEGER",
        InferredType::Unknown => "TEXT",
    }
}

pub fn write_schema_file(schemas_dir: &Path, table_name: &str, content: &str) -> io::Result<()> {
    fs::create_dir_all(schemas_dir)?;
    let path = schemas_dir.join(format!("{}.lua", table_name));
    fs::write(path, content)
}

pub fn write_migration_file(migrations_dir: &Path, name: &str, content: &str) -> io::Result<String> {
    fs::create_dir_all(migrations_dir)?;

    let existing: Vec<_> = fs::read_dir(migrations_dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();

    let next_num = existing
        .iter()
        .filter_map(|n| n.split('_').next()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;

    let filename = format!("{:03}_{}.lua", next_num, name);
    let path = migrations_dir.join(&filename);
    fs::write(&path, content)?;
    Ok(filename)
}

pub fn prompt_yes_no(message: &str) -> io::Result<bool> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("{} [y/n] ", message);
    stdout.flush()?;

    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;

    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

pub fn update_schema_file(schemas_dir: &Path, table_name: &str, new_fields: &[InferredField]) -> io::Result<()> {
    let path = schemas_dir.join(format!("{}.lua", table_name));
    let content = fs::read_to_string(&path)?;

    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if let Some(close_idx) = lines.iter().rposition(|l| l.trim() == "}") {
        for field in new_fields {
            let guard_type = field.field_type.to_guard_type();
            let new_line = format!("    {} = guard:{}(),", field.name, guard_type);
            lines.insert(close_idx, new_line);
        }
    }

    fs::write(path, lines.join("\n"))
}
