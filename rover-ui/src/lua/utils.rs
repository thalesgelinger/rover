use crate::SharedSignalRuntime;
use crate::lua::derived::LuaDerived;
use crate::lua::signal::LuaSignal;
use mlua::{Function, Lua, MultiValue, Result, Value};

/// rover.any(...signals) - returns derived that is true if any signal is truthy
pub fn create_any_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: MultiValue| {
        // Capture all arguments
        let signals: Vec<Value> = args.iter().cloned().collect();

        // Create compute function
        let compute_fn = lua.create_function(move |lua, ()| {
            for sig_val in &signals {
                let value = get_signal_value(lua, sig_val.clone())?;
                if is_truthy(&value) {
                    return Ok(Value::Boolean(true));
                }
            }
            Ok(Value::Boolean(false))
        })?;

        let key = lua.create_registry_value(compute_fn)?;

        let runtime = get_runtime(lua)?;

        let id = runtime.create_derived(key);
        Ok(LuaDerived::new(id))
    })
}

/// rover.all(...signals) - returns derived that is true if all signals are truthy
pub fn create_all_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: MultiValue| {
        let signals: Vec<Value> = args.iter().cloned().collect();

        let compute_fn = lua.create_function(move |lua, ()| {
            for sig_val in &signals {
                let value = get_signal_value(lua, sig_val.clone())?;
                if !is_truthy(&value) {
                    return Ok(Value::Boolean(false));
                }
            }
            Ok(Value::Boolean(true))
        })?;

        let key = lua.create_registry_value(compute_fn)?;

        let runtime = get_runtime(lua)?;

        let id = runtime.create_derived(key);
        Ok(LuaDerived::new(id))
    })
}

/// rover.none(...signals) - returns derived that is true if no signals are truthy
pub fn create_none_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: MultiValue| {
        let signals: Vec<Value> = args.iter().cloned().collect();

        let compute_fn = lua.create_function(move |lua, ()| {
            for sig_val in &signals {
                let value = get_signal_value(lua, sig_val.clone())?;
                if is_truthy(&value) {
                    return Ok(Value::Boolean(false));
                }
            }
            Ok(Value::Boolean(true))
        })?;

        let key = lua.create_registry_value(compute_fn)?;

        let runtime = get_runtime(lua)?;

        let id = runtime.create_derived(key);
        Ok(LuaDerived::new(id))
    })
}

/// Get runtime from Lua app_data
fn get_runtime(lua: &Lua) -> Result<mlua::AppDataRef<'_, SharedSignalRuntime>> {
    lua.app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))
}

/// Get signal value (from signal or derived)
fn get_signal_value(lua: &Lua, value: Value) -> Result<Value> {
    match value {
        Value::UserData(ref ud) => {
            if let Ok(signal) = ud.borrow::<LuaSignal>() {
                let runtime = get_runtime(lua)?;
                runtime.get_signal(lua, signal.id)
            } else if let Ok(derived) = ud.borrow::<LuaDerived>() {
                let runtime = get_runtime(lua)?;
                runtime
                    .get_derived(lua, derived.id)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
            } else {
                Ok(value)
            }
        }
        _ => Ok(value),
    }
}

/// Check if value is truthy (for conditional evaluation)
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Nil => false,
        Value::Boolean(false) => false,
        _ => true,
    }
}
