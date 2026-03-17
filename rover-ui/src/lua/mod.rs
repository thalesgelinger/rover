pub mod derived;
pub mod effect;
pub mod helpers;
pub mod metamethods;
pub mod signal;
pub mod utils;
use crate::task;

use crate::platform::{UiCapability, ViewportSignals};
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
    // Ensure Lua scripts can resolve global `rover` during module setup.
    lua.globals().set("rover", rover_table.clone())?;

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
    let (default_theme, create_mod, mod_obj): (Table, Function, Table) = {
        let modifier_module: Table = lua
            .load(include_str!("modifier.lua"))
            .set_name("rover_ui_modifier.lua")
            .eval()?;
        let default_theme: Table = modifier_module.get("default_theme")?;
        let create_mod: Function = modifier_module.get("create_mod")?;
        let mod_obj: Table = create_mod.call(default_theme.clone())?;
        (default_theme, create_mod, mod_obj)
    };
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

    uv.set("theme", default_theme.clone())?;
    uv.set("mod", mod_obj)?;
    uv.set("screen", screen)?;
    lua_ui.set_user_value(uv)?;
    lua.globals().set("_rover_ui_theme", default_theme)?;
    lua.globals().set("_rover_ui_create_mod", create_mod)?;
    rover_table.set("ui", lua_ui)?;

    if crate::lua::helpers::has_capability(lua, UiCapability::TuiNamespace)? {
        let tui_module = create_tui_module(lua)?;
        rover_table.set("tui", tui_module)?;
    }

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
            if !crate::lua::helpers::has_capability(lua, UiCapability::TuiNamespace)? {
                return Err(mlua::Error::RuntimeError(format!(
                    "require(\"rover.tui\") denied by capability policy (capability={}, target={})",
                    UiCapability::TuiNamespace.as_str(),
                    target.as_str()
                )));
            }

            let rover_table: Table = lua.globals().get("rover")?;
            match rover_table.get::<Value>("tui")? {
                Value::Table(module) => Ok(module),
                _ => {
                    let module = create_tui_module(lua)?;
                    rover_table.set("tui", module.clone())?;
                    Ok(module)
                }
            }
        })?,
    )?;

    Ok(())
}

fn create_tui_module(lua: &Lua) -> Result<Table> {
    lua.load(include_str!("tui/module.lua"))
        .set_name("rover_tui_module.lua")
        .eval()
}

#[cfg(test)]
mod tests {
    use super::register_ui_module;
    use crate::platform::{
        DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH, UiCapability, UiRuntimeConfig, UiTarget,
        ViewportSignals,
    };
    use crate::scheduler::{Scheduler, SharedScheduler};
    use crate::signal::{SignalRuntime, SignalValue};
    use crate::ui::registry::UiRegistry;
    use mlua::{Lua, Value};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn setup_lua(runtime_config: UiRuntimeConfig) -> Lua {
        let lua = Lua::new();
        let runtime = Rc::new(SignalRuntime::new());
        let registry = Rc::new(RefCell::new(UiRegistry::new()));
        let scheduler: SharedScheduler = Rc::new(RefCell::new(Scheduler::new()));
        let viewport_signals = ViewportSignals {
            width: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_WIDTH as i64)),
            height: runtime.create_signal(SignalValue::Int(DEFAULT_VIEWPORT_HEIGHT as i64)),
        };

        lua.set_app_data(runtime);
        lua.set_app_data(registry);
        lua.set_app_data(scheduler);
        lua.set_app_data(runtime_config);
        lua.set_app_data(viewport_signals);

        let rover = lua.create_table().expect("create rover table");
        register_ui_module(&lua, &rover).expect("register ui module");
        lua.globals().set("rover", rover).expect("set global rover");
        lua
    }

    #[test]
    fn should_deny_tui_namespace_on_web_by_default() {
        let lua = setup_lua(UiRuntimeConfig::new(UiTarget::Web));

        let (ok, err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    require("rover.tui")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .expect("lua eval");

        assert!(!ok);
        assert!(err.contains("denied by capability policy"));
    }

    #[test]
    fn should_allow_tui_namespace_when_explicitly_allowed() {
        let config =
            UiRuntimeConfig::new(UiTarget::Web).allow_capability(UiCapability::TuiNamespace);
        let lua = setup_lua(config);

        let module_type: String = lua
            .load(
                r#"
                local mod = require("rover.tui")
                return type(mod)
            "#,
            )
            .eval()
            .expect("lua eval");

        assert_eq!(module_type, "table");
    }

    #[test]
    fn should_deny_tui_namespace_when_explicitly_denied_on_tui_target() {
        let config =
            UiRuntimeConfig::new(UiTarget::Tui).deny_capability(UiCapability::TuiNamespace);
        let lua = setup_lua(config);

        let tui_global_is_nil: bool = lua
            .load("return rover.tui == nil")
            .eval()
            .expect("lua eval");
        assert!(tui_global_is_nil);

        let result: Value = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    require("rover.tui")
                end)
                if ok then
                    return true
                end
                return tostring(err)
            "#,
            )
            .eval()
            .expect("lua eval");

        match result {
            Value::Boolean(ok) => assert!(!ok),
            Value::String(err) => assert!(
                err.to_str()
                    .expect("err str")
                    .contains("denied by capability policy")
            ),
            _ => panic!("unexpected result"),
        }
    }

    #[test]
    fn should_prioritize_deny_over_allow() {
        let config = UiRuntimeConfig::new(UiTarget::Tui)
            .allow_capability(UiCapability::TuiNamespace)
            .deny_capability(UiCapability::TuiNamespace);
        let lua = setup_lua(config);

        let (ok, err): (bool, String) = lua
            .load(
                r#"
                local ok, err = pcall(function()
                    require("rover.tui")
                end)
                return ok, tostring(err)
            "#,
            )
            .eval()
            .expect("lua eval");

        assert!(!ok);
        assert!(err.contains("denied by capability policy"));
    }
}
