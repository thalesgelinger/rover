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
        lines.push(format!(
            "    {} = rover.db.guard:{}(){},",
            field.name, guard_type, modifiers
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

pub fn generate_migration_content(
    table_name: &str,
    fields: &[InferredField],
    is_create: bool,
) -> String {
    let mut lines = Vec::new();

    if is_create {
        lines.push(format!("-- Create {} table", table_name));
        lines.push("function change()".to_string());
        lines.push(format!("    migration.{}:create({{", table_name));

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
            let guard_type = inferred_to_guard_type(&field.field_type);
            let modifiers = if field.name == "id" {
                ":primary():auto()"
            } else {
                ""
            };
            lines.push(format!(
                "        {} = rover.db.guard:{}(){},",
                field.name, guard_type, modifiers
            ));
        }

        lines.push("    })".to_string());
        lines.push("end".to_string());
    } else {
        lines.push(format!("-- Add fields to {}", table_name));
        lines.push("function change()".to_string());
        lines.push(format!("    migration.{}:alter_table()", table_name));

        for (i, field) in fields.iter().enumerate() {
            let guard_type = inferred_to_guard_type(&field.field_type);
            let prefix = if i == 0 { "        " } else { "        " };
            lines.push(format!(
                "{}:add_column(\"{}\", rover.db.guard:{}())",
                prefix, field.name, guard_type
            ));
        }

        lines.push("end".to_string());
    }

    lines.join("\n")
}

fn inferred_to_guard_type(t: &InferredType) -> &'static str {
    match t {
        InferredType::Integer => "integer",
        InferredType::Number => "number",
        InferredType::String => "string",
        InferredType::Boolean => "boolean",
        InferredType::Unknown => "string",
    }
}

pub fn write_schema_file(schemas_dir: &Path, table_name: &str, content: &str) -> io::Result<()> {
    fs::create_dir_all(schemas_dir)?;
    let path = schemas_dir.join(format!("{}.lua", table_name));
    fs::write(path, content)
}

pub fn write_migration_file(
    migrations_dir: &Path,
    name: &str,
    content: &str,
) -> io::Result<String> {
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

pub fn update_schema_file(
    schemas_dir: &Path,
    table_name: &str,
    new_fields: &[InferredField],
) -> io::Result<()> {
    let path = schemas_dir.join(format!("{}.lua", table_name));
    let content = fs::read_to_string(&path)?;

    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    if let Some(close_idx) = lines.iter().rposition(|l| l.trim() == "}") {
        for field in new_fields {
            let guard_type = field.field_type.to_guard_type();
            let new_line = format!("    {} = rover.db.guard:{}(),", field.name, guard_type);
            lines.insert(close_idx, new_line);
        }
    }

    fs::write(path, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_parser::db_intent::{FieldSource, FieldType, InferredField, InferredTable};
    use std::collections::HashMap;

    fn create_test_field(name: &str, field_type: FieldType) -> InferredField {
        InferredField {
            name: name.to_string(),
            field_type,
            source: FieldSource::Access,
        }
    }

    fn create_test_table(name: &str, fields: Vec<InferredField>) -> InferredTable {
        let mut field_map = HashMap::new();
        for field in fields {
            field_map.insert(field.name.clone(), field);
        }
        InferredTable {
            name: name.to_string(),
            fields: field_map,
        }
    }

    #[test]
    fn test_generate_schema_content_basic() {
        let fields = vec![
            create_test_field("id", FieldType::Integer),
            create_test_field("name", FieldType::String),
            create_test_field("email", FieldType::String),
        ];
        let table = create_test_table("users", fields);
        let content = generate_schema_content(&table);

        assert!(content.contains("rover.db.schema.users"));
        assert!(content.contains("id = rover.db.guard:integer():primary():auto()"));
        assert!(content.contains("email = rover.db.guard:string(),"));
        assert!(content.contains("name = rover.db.guard:string(),"));
    }

    #[test]
    fn test_generate_schema_content_id_first() {
        // Test that id field is sorted first regardless of input order
        let fields = vec![
            create_test_field("name", FieldType::String),
            create_test_field("id", FieldType::Integer),
            create_test_field("age", FieldType::Integer),
        ];
        let table = create_test_table("people", fields);
        let content = generate_schema_content(&table);

        // id should appear before other fields alphabetically
        let id_pos = content.find("id = rover").unwrap();
        let name_pos = content.find("name = rover").unwrap();
        let age_pos = content.find("age = rover").unwrap();

        assert!(id_pos < name_pos, "id should come before name");
        assert!(id_pos < age_pos, "id should come before age");
    }

    #[test]
    fn test_generate_schema_content_all_types() {
        let fields = vec![
            create_test_field("id", FieldType::Integer),
            create_test_field("name", FieldType::String),
            create_test_field("age", FieldType::Integer),
            create_test_field("score", FieldType::Number),
            create_test_field("active", FieldType::Boolean),
            create_test_field("data", FieldType::Unknown),
        ];
        let table = create_test_table("test_table", fields);
        let content = generate_schema_content(&table);

        assert!(content.contains("integer"), "Should contain integer type");
        assert!(content.contains("string"), "Should contain string type");
        assert!(content.contains("number"), "Should contain number type");
        assert!(content.contains("boolean"), "Should contain boolean type");
        assert!(content.contains("id = rover.db.guard:integer():primary():auto()"));
    }

    #[test]
    fn test_generate_schema_content_empty_table() {
        let table = create_test_table("empty", vec![]);
        let content = generate_schema_content(&table);

        assert!(content.contains("rover.db.schema.empty"));
        assert!(content.contains("}"));
        assert!(!content.contains("rover.db.guard"));
    }

    #[test]
    fn test_generate_schema_content_single_field() {
        let fields = vec![create_test_field("id", FieldType::Integer)];
        let table = create_test_table("single", fields);
        let content = generate_schema_content(&table);

        assert!(content.contains("rover.db.schema.single"));
        assert!(content.contains("id = rover.db.guard:integer():primary():auto()"));
    }

    #[test]
    fn test_generate_migration_content_create() {
        let fields = vec![
            create_test_field("id", FieldType::Integer),
            create_test_field("title", FieldType::String),
            create_test_field("count", FieldType::Integer),
        ];

        let content = generate_migration_content("posts", &fields, true);

        assert!(content.contains("-- Create posts table"));
        assert!(content.contains("function change()"));
        assert!(content.contains("migration.posts:create"));
        assert!(content.contains("id = rover.db.guard:integer():primary():auto()"));
        assert!(content.contains("title = rover.db.guard:string(),"));
        assert!(content.contains("count = rover.db.guard:integer(),"));
        assert!(content.contains("end"));
    }

    #[test]
    fn test_generate_migration_content_alter() {
        let fields = vec![
            create_test_field("description", FieldType::String),
            create_test_field("rating", FieldType::Number),
        ];

        let content = generate_migration_content("products", &fields, false);

        assert!(content.contains("-- Add fields to products"));
        assert!(content.contains("function change()"));
        assert!(content.contains("migration.products:alter_table()"));
        assert!(content.contains(":add_column(\"description\", rover.db.guard:string())"));
        assert!(content.contains(":add_column(\"rating\", rover.db.guard:number())"));
        assert!(content.contains("end"));
    }

    #[test]
    fn test_compare_intent_with_schemas_new_table() {
        let mut intent_tables = HashMap::new();
        let fields = vec![create_test_field("id", FieldType::Integer)];
        intent_tables.insert(
            "new_table".to_string(),
            create_test_table("new_table", fields),
        );

        let intent = DbIntent {
            tables: intent_tables,
            ..DbIntent::new()
        };
        let schemas = HashMap::new();

        let comparison = compare_intent_with_schemas(&intent, &schemas);

        assert_eq!(comparison.diffs.len(), 1);
        assert_eq!(comparison.diffs[0].table_name, "new_table");
        assert!(matches!(comparison.diffs[0].status, TableStatus::New));
    }

    #[test]
    fn test_compare_intent_with_schemas_existing_unchanged() {
        let mut intent_tables = HashMap::new();
        let fields = vec![create_test_field("id", FieldType::Integer)];
        intent_tables.insert("users".to_string(), create_test_table("users", fields));

        let intent = DbIntent {
            tables: intent_tables,
            ..DbIntent::new()
        };

        let mut schemas = HashMap::new();
        let existing_def = TableDefinition {
            fields: vec![crate::schema_analyzer::FieldDefinition {
                name: "id".to_string(),
                field_type: crate::schema_analyzer::FieldType::Integer,
                nullable: false,
                primary_key: true,
                auto_increment: true,
                unique: false,
                references: None,
                indexed: false,
            }],
        };
        schemas.insert("users".to_string(), existing_def);

        let comparison = compare_intent_with_schemas(&intent, &schemas);

        assert_eq!(comparison.diffs.len(), 1);
        assert_eq!(comparison.diffs[0].table_name, "users");
        assert!(matches!(comparison.diffs[0].status, TableStatus::Exists));
        assert!(comparison.diffs[0].new_fields.is_empty());
    }

    #[test]
    fn test_compare_intent_with_schemas_needs_update() {
        let mut intent_tables = HashMap::new();
        let fields = vec![
            create_test_field("id", FieldType::Integer),
            create_test_field("email", FieldType::String),
        ];
        intent_tables.insert("users".to_string(), create_test_table("users", fields));

        let intent = DbIntent {
            tables: intent_tables,
            ..DbIntent::new()
        };

        let mut schemas = HashMap::new();
        let existing_def = TableDefinition {
            fields: vec![crate::schema_analyzer::FieldDefinition {
                name: "id".to_string(),
                field_type: crate::schema_analyzer::FieldType::Integer,
                nullable: false,
                primary_key: true,
                auto_increment: false,
                unique: false,
                references: None,
                indexed: false,
            }],
        };
        schemas.insert("users".to_string(), existing_def);

        let comparison = compare_intent_with_schemas(&intent, &schemas);

        assert_eq!(comparison.diffs.len(), 1);
        assert_eq!(comparison.diffs[0].table_name, "users");
        assert!(matches!(
            comparison.diffs[0].status,
            TableStatus::NeedsUpdate
        ));
        assert_eq!(comparison.diffs[0].new_fields.len(), 1);
        assert_eq!(comparison.diffs[0].new_fields[0].name, "email");
    }
}
