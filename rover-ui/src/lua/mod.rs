pub mod derived;
pub mod effect;
pub mod helpers;
pub mod metamethods;
pub mod signal;
pub mod utils;
use crate::task;

use crate::{signal::SignalValue, ui::ui::LuaUi};
use derived::LuaDerived;
use mlua::{Function, Lua, Result, Table, UserData, Value};
use signal::LuaSignal;

/// Marker for delayed coroutine execution
#[derive(Debug, Clone, Copy)]
pub struct DelayMarker {
    pub delay_ms: u64,
}

impl UserData for DelayMarker {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("delay_ms", |_lua, this| Ok(this.delay_ms));
    }
}

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

    // rover.any(...) - utility
    let any_fn = utils::create_any_fn(lua)?;
    rover_table.set("any", any_fn)?;

    // rover.all(...) - utility
    let all_fn = utils::create_all_fn(lua)?;
    rover_table.set("all", all_fn)?;

    // rover.none(...) - utility
    let none_fn = utils::create_none_fn(lua)?;
    rover_table.set("none", none_fn)?;

    // rover._delay_ms(ms) - internal function that creates a DelayMarker
    let delay_fn = lua.create_function(|lua, delay_ms: u64| {
        let marker = DelayMarker { delay_ms };
        lua.create_userdata(marker)
    })?;
    rover_table.set("_delay_ms", delay_fn)?;

    // rover.delay(ms) - delay coroutine execution (non-yielding version by default)
    // Task wrappers override this to yield
    let delay_wrapper = lua.create_function(|lua, delay_ms: u64| {
        let marker = DelayMarker { delay_ms };
        lua.create_userdata(marker)
    })?;
    rover_table.set("delay", delay_wrapper)?;

    // Note: rover.render() is NOT registered here - users define their own
    // global function: `function rover.render() ... end`

    // Register task module (rover.task(fn), rover.task.cancel(), etc.)
    task::register_task_module(lua, rover_table)?;

    // rover.on_destroy(fn) - register cleanup callbacks
    let on_destroy_fn = lua.create_function(|lua, callback: Function| {
        let registry = crate::lua::helpers::get_registry(lua)?;
        let key = lua.create_registry_value(callback)?;
        registry.borrow_mut().add_on_destroy_callback(key);
        Ok(())
    })?;
    rover_table.set("on_destroy", on_destroy_fn)?;

    let lua_ui = lua.create_userdata(LuaUi::new())?;
    lua_ui.set_user_value(lua.create_table()?)?;
    rover_table.set("ui", lua_ui)?;

    Ok(())
}
