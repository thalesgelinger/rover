use crate::signal::{DerivedId, SignalId};
use crate::ui::registry::UiRegistry;
use crate::{SharedSignalRuntime, scheduler::SharedScheduler};
use mlua::{AppDataRef, Lua, Result, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// Get runtime from Lua app_data
pub fn get_runtime(lua: &Lua) -> Result<AppDataRef<'_, SharedSignalRuntime>> {
    lua.app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))
}

/// Get scheduler from Lua app_data
pub fn get_scheduler(lua: &Lua) -> Result<SharedScheduler> {
    lua.app_data_ref::<SharedScheduler>()
        .ok_or_else(|| mlua::Error::RuntimeError("Scheduler not initialized".into()))
        .map(|s| s.clone())
}

/// Get UI registry from Lua app_data
pub fn get_registry(lua: &Lua) -> Result<Rc<RefCell<UiRegistry>>> {
    lua.app_data_ref::<Rc<RefCell<UiRegistry>>>()
        .ok_or_else(|| mlua::Error::RuntimeError("UI registry not initialized".into()))
        .map(|r| r.clone())
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
