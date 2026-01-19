//! Schema loader - loads and executes schema Lua files
//!
//! Loads db/schemas/*.lua files and extracts table definitions.

use mlua::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load all schema files from a directory (creates its own Lua instance)
pub fn load_schemas_from_dir(schemas_dir: &Path) -> Result<HashMap<String, TableDefinition>, String> {
    let lua = Lua::new();
    load_schemas(&lua, schemas_dir)
}

/// Load all schema files from a directory and return table definitions
pub fn load_schemas(
    lua: &Lua,
    schemas_dir: &Path,
) -> Result<HashMap<String, TableDefinition>, String> {
    let mut schemas = HashMap::new();

    if !schemas_dir.exists() {
        return Ok(schemas);
    }

    // Get the schema DSL
    let schema_dsl: LuaTable = lua
        .load(include_str!("schema_dsl.lua"))
        .set_name("schema_dsl.lua")
        .eval()
        .map_err(|e| format!("Failed to load schema DSL: {}", e))?;

    // Clear any existing schemas
    let clear_fn: LuaFunction = schema_dsl
        .get("clear")
        .map_err(|e| format!("Failed to get clear function: {}", e))?;
    clear_fn
        .call::<()>(())
        .map_err(|e| format!("Failed to clear schemas: {}", e))?;

    // Load guard module for schema field types
    let guard: LuaTable = lua
        .load(include_str!("guard.lua"))
        .set_name("guard.lua")
        .eval()
        .map_err(|e| format!("Failed to load guard: {}", e))?;

    // Make schema DSL available globally as rover.db.schema and guard
    let globals = lua.globals();
    globals
        .set("guard", guard)
        .map_err(|e| format!("Failed to set guard: {}", e))?;

    let rover: LuaTable = globals
        .get("rover")
        .unwrap_or_else(|_| lua.create_table().unwrap());
    let db: LuaTable = rover
        .get("db")
        .unwrap_or_else(|_| lua.create_table().unwrap());
    db.set("schema", schema_dsl.clone())
        .map_err(|e| format!("Failed to set schema: {}", e))?;
    rover
        .set("db", db)
        .map_err(|e| format!("Failed to set db: {}", e))?;
    globals
        .set("rover", rover)
        .map_err(|e| format!("Failed to set rover: {}", e))?;

    // Load each schema file
    let entries =
        fs::read_dir(schemas_dir).map_err(|e| format!("Failed to read schemas dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("lua") {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("schema");

            lua.load(&content)
                .set_name(filename)
                .exec()
                .map_err(|e| format!("Failed to execute {:?}: {}", path, e))?;
        }
    }

    // Extract registered schemas
    let get_all_fn: LuaFunction = schema_dsl
        .get("get_all")
        .map_err(|e| format!("Failed to get get_all function: {}", e))?;

    let all_schemas: LuaTable = get_all_fn
        .call(())
        .map_err(|e| format!("Failed to get all schemas: {}", e))?;

    // Convert Lua schemas to Rust structs
    for pair in all_schemas.pairs::<String, LuaTable>() {
        let (table_name, definition) =
            pair.map_err(|e| format!("Failed to iterate schemas: {}", e))?;

        let table_def = parse_table_definition(&definition)?;
        schemas.insert(table_name, table_def);
    }

    Ok(schemas)
}

/// Parse a Lua table definition into a Rust struct
fn parse_table_definition(definition: &LuaTable) -> Result<TableDefinition, String> {
    let mut fields = Vec::new();

    for pair in definition.clone().pairs::<String, LuaValue>() {
        let (field_name, field_def) =
            pair.map_err(|e| format!("Failed to iterate definition: {}", e))?;

        let field = parse_field_definition(&field_name, &field_def)?;
        fields.push(field);
    }

    // Sort fields so id comes first
    fields.sort_by(|a, b| {
        if a.name == "id" {
            std::cmp::Ordering::Less
        } else if b.name == "id" {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    Ok(TableDefinition { fields })
}

/// Parse a field definition from Lua guard type
fn parse_field_definition(name: &str, value: &LuaValue) -> Result<FieldDefinition, String> {
    let mut field = FieldDefinition {
        name: name.to_string(),
        field_type: FieldType::Text,
        nullable: true, // Default nullable
        primary_key: false,
        auto_increment: false,
        unique: false,
        references: None,
        indexed: false,
    };

    match value {
        LuaValue::Table(t) => {
            // Extract type from guard table (uses _type field)
            if let Ok(type_str) = t.get::<String>("_type") {
                field.field_type = FieldType::from_str(&type_str);
            }

            // Check modifiers (guard uses _prefixed fields)
            if let Ok(true) = t.get::<bool>("_primary") {
                field.primary_key = true;
                field.nullable = false;
            }
            if let Ok(true) = t.get::<bool>("_auto") {
                field.auto_increment = true;
            }
            if let Ok(true) = t.get::<bool>("_unique") {
                field.unique = true;
            }
            if let Ok(true) = t.get::<bool>("_required") {
                field.nullable = false;
            }
            if let Ok(false) = t.get::<bool>("_nullable") {
                field.nullable = false;
            }
            if let Ok(true) = t.get::<bool>("_index") {
                field.indexed = true;
            }
            if let Ok(ref_str) = t.get::<String>("_references") {
                // Parse "table.column" format
                if let Some((table, col)) = ref_str.split_once('.') {
                    field.references = Some(ForeignKey {
                        table: table.to_string(),
                        column: col.to_string(),
                    });
                }
            }
        }
        LuaValue::String(s) => {
            // Simple type string
            if let Ok(type_str) = s.to_str() {
                field.field_type = FieldType::from_str(&type_str);
            }
        }
        _ => {}
    }

    Ok(field)
}

/// Table definition
#[derive(Debug, Clone)]
pub struct TableDefinition {
    pub fields: Vec<FieldDefinition>,
}

/// Field definition
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,
    pub nullable: bool,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub unique: bool,
    pub references: Option<ForeignKey>,
    pub indexed: bool,
}

/// Foreign key reference
#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub table: String,
    pub column: String,
}

/// Field types
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Integer,
    Text,
    Real,
    Boolean,
    Datetime,
    Date,
    Blob,
}

impl FieldType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "integer" | "int" => FieldType::Integer,
            "real" | "float" | "double" | "number" => FieldType::Real,
            "boolean" | "bool" => FieldType::Boolean,
            "datetime" => FieldType::Datetime,
            "date" => FieldType::Date,
            "blob" => FieldType::Blob,
            _ => FieldType::Text,
        }
    }

    pub fn to_sql(&self) -> &'static str {
        match self {
            FieldType::Integer => "INTEGER",
            FieldType::Text => "TEXT",
            FieldType::Real => "REAL",
            FieldType::Boolean => "INTEGER",
            FieldType::Datetime => "DATETIME",
            FieldType::Date => "DATE",
            FieldType::Blob => "BLOB",
        }
    }
}

/// Generate CREATE TABLE SQL from definition
pub fn generate_create_table(table_name: &str, def: &TableDefinition) -> String {
    let mut columns = Vec::new();
    let mut constraints = Vec::new();

    for field in &def.fields {
        let mut col = format!("{} {}", field.name, field.field_type.to_sql());

        if field.primary_key {
            col.push_str(" PRIMARY KEY");
            if field.auto_increment && field.field_type == FieldType::Integer {
                col.push_str(" AUTOINCREMENT");
            }
        }

        if !field.nullable && !field.primary_key {
            col.push_str(" NOT NULL");
        }

        if field.unique && !field.primary_key {
            col.push_str(" UNIQUE");
        }

        columns.push(col);

        if let Some(ref fk) = field.references {
            constraints.push(format!(
                "FOREIGN KEY ({}) REFERENCES {}({})",
                field.name, fk.table, fk.column
            ));
        }
    }

    columns.extend(constraints);

    format!(
        "CREATE TABLE IF NOT EXISTS {} (\n  {}\n)",
        table_name,
        columns.join(",\n  ")
    )
}
