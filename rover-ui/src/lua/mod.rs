pub mod derived;
pub mod effect;
pub mod helpers;
pub mod metamethods;
pub mod signal;
pub mod utils;
use crate::task;

use crate::platform::UiTarget;
use crate::platform::ViewportSignals;
use crate::{signal::SignalValue, ui::ui::LuaUi};
use derived::LuaDerived;
use mlua::{AnyUserData, Function, Lua, Result, Table, UserData, Value};
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

    // rover.spawn(fn) - create and start a background task immediately
    let spawn_fn = lua.create_function(|lua, func: Function| {
        let task_ud = task::create_task(lua, func)?;
        task::start_task(lua, &task_ud)?;
        Ok(task_ud)
    })?;
    rover_table.set("spawn", spawn_fn)?;

    // rover.interval(ms, fn) - run fn immediately, then every ms
    let interval_fn = lua.create_function(|lua, (delay_ms, callback): (u64, Function)| {
        let interval_factory: Function = lua
            .load(
                r#"
                return function(ms, cb)
                    return function(...)
                        cb(...)
                        while true do
                            rover.delay(ms)
                            cb(...)
                        end
                    end
                end
                "#,
            )
            .eval()?;

        let interval_task_fn: Function = interval_factory.call((delay_ms, callback))?;
        let task_ud = task::create_task(lua, interval_task_fn)?;
        task::start_task(lua, &task_ud)?;
        Ok(task_ud)
    })?;
    rover_table.set("interval", interval_fn)?;

    // rover.on_destroy(fn) - register cleanup callbacks
    let on_destroy_fn = lua.create_function(|lua, callback: Function| {
        let registry = crate::lua::helpers::get_registry(lua)?;
        let key = lua.create_registry_value(callback)?;
        registry.borrow_mut().add_on_destroy_callback(key);
        Ok(())
    })?;
    rover_table.set("on_destroy", on_destroy_fn)?;

    let lua_ui = lua.create_userdata(LuaUi::new())?;
    let uv = lua.create_table()?;
    let modifier_module: Table = lua
        .load(include_str!("modifier.lua"))
        .set_name("rover_ui_modifier.lua")
        .eval()?;
    let default_theme: Table = modifier_module.get("default_theme")?;
    let create_mod: Function = modifier_module.get("create_mod")?;
    let mod_obj: Table = create_mod.call(default_theme.clone())?;
    let viewport = lua
        .app_data_ref::<ViewportSignals>()
        .ok_or_else(|| mlua::Error::RuntimeError("Viewport signals not initialized".into()))?;
    let screen = lua.create_table()?;
    screen.set(
        "width",
        lua.create_userdata(LuaSignal::new(viewport.width))?,
    )?;
    screen.set(
        "height",
        lua.create_userdata(LuaSignal::new(viewport.height))?,
    )?;

    uv.set("theme", default_theme)?;
    uv.set("mod", mod_obj)?;
    uv.set("screen", screen)?;
    lua_ui.set_user_value(uv)?;
    rover_table.set("ui", lua_ui)?;

    register_tui_preload_module(lua)?;

    Ok(())
}

fn register_tui_preload_module(lua: &Lua) -> Result<()> {
    let package: Table = lua.globals().get("package")?;
    let preload: Table = package.get("preload")?;
    preload.set(
        "rover.tui",
        lua.create_function(|lua, _name: Value| {
            let target = crate::lua::helpers::get_target(lua)?;
            if target != UiTarget::Tui {
                return Err(mlua::Error::RuntimeError(format!(
                    "require(\"rover.tui\") requires target=tui, got {}",
                    target.as_str()
                )));
            }

            let rover_table: Table = lua.globals().get("rover")?;
            let ui_ud: AnyUserData = rover_table.get("ui")?;
            let uv: Table = ui_ud.user_value()?;

            let module: Table = lua
                .load(include_str!("tui_module.lua"))
                .set_name("rover_tui_module.lua")
                .eval()?;

            let select_fn: Function = module.get("select")?;
            let tab_select_fn: Function = module.get("tab_select")?;
            let scroll_box_fn: Function = module.get("scroll_box")?;
            let textarea_fn: Function = module.get("textarea")?;
            let nav_list_fn: Function = module.get("nav_list")?;
            let separator_fn: Function = module.get("separator")?;
            let badge_fn: Function = module.get("badge")?;
            let progress_fn: Function = module.get("progress")?;
            let paginator_fn: Function = module.get("paginator")?;
            let full_screen_fn: Function = module.get("full_screen")?;

            uv.set("select", select_fn.clone())?;
            uv.set("tab_select", tab_select_fn.clone())?;
            uv.set("scroll_box", scroll_box_fn.clone())?;
            uv.set("textarea", textarea_fn.clone())?;
            uv.set("nav_list", nav_list_fn.clone())?;
            uv.set("separator", separator_fn.clone())?;
            uv.set("badge", badge_fn.clone())?;
            uv.set("progress", progress_fn.clone())?;
            uv.set("paginator", paginator_fn.clone())?;
            uv.set("full_screen", full_screen_fn.clone())?;

            Ok(module)
        })?,
    )?;

    Ok(())
}
