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
                let is_sealed = tbl.raw_get::<bool>("__sealed").unwrap_or(false);
                if is_sealed {
                    return Err(Error::RuntimeError(format!("Unkown key {:?}", k)));
                } else {
                    let new_table = lua.create_auto_table()?;
                    tbl.raw_set(k, &new_table)?;
                    Ok(new_table)
                }
            })?,
        )?;

        metatable.set(
            "__newindex",
            self.create_function(|_, (tbl, k, v): (Table, String, Value)| tbl.raw_set(k, v))?,
        )?;

        let _ = table.set_metatable(Some(metatable));
        Ok(table)
    }
}
