use super::helpers::get_runtime;
use mlua::{Function, Lua, Result};

/// rover.effect(callback)
pub fn create_effect_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, callback: Function| {
        let key = lua.create_registry_value(callback)?;

        let runtime = get_runtime(lua)?;

        let _effect_id = runtime
            .create_effect(lua, key)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

        // Return nothing (effects are fire-and-forget for now)
        // TODO: Return disposer function
        Ok(())
    })
}
