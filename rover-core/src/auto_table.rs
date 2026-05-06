use anyhow::Result;
use mlua::{Error, Lua, Table, Value};

pub trait AutoTable {
    fn create_auto_table(&self) -> Result<Table>;
}
impl AutoTable for Lua {
    fn create_auto_table(&self) -> Result<Table> {
        let table = self.create_table()?;
        let metatable = self.create_table()?;

        metatable.set(
            "__index",
            self.create_function(|lua, (tbl, k): (Table, String)| {
                // Check if key already exists in the table
                let existing: Value = tbl.raw_get(k.clone())?;
                eprintln!(
                    "DEBUG __index: checking key '{}', existing={:?}",
                    k,
                    std::mem::discriminant(&existing)
                );
                if !matches!(existing, Value::Nil) {
                    eprintln!("DEBUG __index: returning existing value for key '{}'", k);
                    return Ok(existing);
                }

                let is_sealed = tbl.raw_get::<bool>("__sealed").unwrap_or(false);
                if is_sealed {
                    return Err(Error::RuntimeError(format!("Unkown key {:?}", k)));
                } else if k == "static" {
                    let owner = tbl.clone();
                    let static_mount = lua.create_function(move |_lua, config: Table| {
                        owner.raw_set("__rover_static_mount", config)?;
                        Ok(())
                    })?;
                    tbl.raw_set("static", static_mount.clone())?;
                    Ok(Value::Function(static_mount))
                } else {
                    eprintln!("DEBUG __index: creating new table for key '{}'", k);
                    let new_table = lua.create_auto_table()?;
                    tbl.raw_set(k, &new_table)?;
                    Ok(Value::Table(new_table))
                }
            })?,
        )?;

        metatable.set(
            "__index",
            self.create_function(|lua, (tbl, k): (Table, String)| {
                // Check if key already exists in the table
                let existing: Value = tbl.raw_get(k.clone())?;
                if !matches!(existing, Value::Nil) {
                    return Ok(existing);
                }

                let is_sealed = tbl.raw_get::<bool>("__sealed").unwrap_or(false);
                if is_sealed {
                    return Err(Error::RuntimeError(format!("Unkown key {:?}", k)));
                } else if k == "static" {
                    let owner = tbl.clone();
                    let static_mount = lua.create_function(move |_lua, config: Table| {
                        owner.raw_set("__rover_static_mount", config)?;
                        Ok(())
                    })?;
                    tbl.raw_set("static", static_mount.clone())?;
                    Ok(Value::Function(static_mount))
                } else {
                    let new_table = lua.create_auto_table()?;
                    tbl.raw_set(k, &new_table)?;
                    Ok(Value::Table(new_table))
                }
            })?,
        )?;

        let _ = table.set_metatable(Some(metatable));
        Ok(table)
    }
}
