pub mod derived;
pub mod effect;
pub mod helpers;
pub mod metamethods;
pub mod node;
pub mod signal;
pub mod ui;
pub mod utils;

use crate::signal::SignalValue;
use derived::LuaDerived;
use mlua::{Function, Lua, Result, Table, Value};
use signal::LuaSignal;

/// Register the UI module with Lua (adds rover.signal, rover.derive, rover.effect, etc.)
pub fn register_ui_module(lua: &Lua, rover_table: &Table) -> Result<()> {
    // rover.signal(value) - create a signal
    let signal_fn = lua.create_function(|lua, value: Value| {
        let runtime = crate::lua::helpers::get_runtime(lua)?;

        let signal_value = SignalValue::from_lua(lua, value)?;
        let id = runtime.create_signal(signal_value);

        Ok(LuaSignal::new(id))
    })?;

    rover_table.set("signal", signal_fn)?;

    // rover.derive(fn) - create a derived signal
    let derive_fn = lua.create_function(|lua, compute_fn: Function| {
        let key = lua.create_registry_value(compute_fn)?;

        let runtime = crate::lua::helpers::get_runtime(lua)?;

        let id = runtime.create_derived(key);

        Ok(LuaDerived::new(id))
    })?;

    rover_table.set("derive", derive_fn)?;

    // rover.effect(fn) - create an effect
    let effect_fn = effect::create_effect_fn(lua)?;
    rover_table.set("effect", effect_fn)?;

    // rover.ui namespace
    let ui_table = lua.create_table()?;
    ui::register_ui_functions(lua, &ui_table)?;
    rover_table.set("ui", ui_table)?;

    // rover.any(...) - utility
    let any_fn = utils::create_any_fn(lua)?;
    rover_table.set("any", any_fn)?;

    // rover.all(...) - utility
    let all_fn = utils::create_all_fn(lua)?;
    rover_table.set("all", all_fn)?;

    // rover.none(...) - utility
    let none_fn = utils::create_none_fn(lua)?;
    rover_table.set("none", none_fn)?;

    Ok(())
}
