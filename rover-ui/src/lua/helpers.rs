use crate::SharedSignalRuntime;
use crate::signal::{DerivedId, SignalId};
use mlua::{AppDataRef, Lua, Result, Value};

/// Get runtime from Lua app_data
pub fn get_runtime(lua: &Lua) -> Result<AppDataRef<'_, SharedSignalRuntime>> {
    lua.app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))
}

/// Get signal value as Lua Value
pub fn get_signal_as_lua(lua: &Lua, id: SignalId) -> Result<Value> {
    let runtime = lua
        .app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))?;
    runtime.get_signal(lua, id)
}

/// Get derived value as Lua Value
pub fn get_derived_as_lua(lua: &Lua, id: DerivedId) -> Result<Value> {
    let runtime = lua
        .app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))?;
    runtime
        .get_derived(lua, id)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
}
