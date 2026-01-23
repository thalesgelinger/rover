use super::lua_node::LuaNode;
use super::node::{TextContent, UiNode};
use super::registry::UiRegistry;
use crate::lua::{derived::LuaDerived, signal::LuaSignal};
use mlua::{AnyUserData, Table, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub struct UiTree {}

impl UserData for UiTree {}

pub struct LuaUi;

impl LuaUi {
    pub fn new() -> Self {
        LuaUi
    }
}

impl Default for LuaUi {
    fn default() -> Self {
        Self::new()
    }
}

fn get_registry_rc(lua: &mlua::Lua) -> mlua::Result<Rc<RefCell<UiRegistry>>> {
    lua.app_data_ref::<Rc<RefCell<UiRegistry>>>()
        .ok_or_else(|| mlua::Error::RuntimeError("UiRegistry not found in app_data".to_string()))
        .map(|r| r.clone())
}

impl UserData for LuaUi {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("text", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let value = props.get::<Value>(1)?;

            match value {
                // Static string
                Value::String(s) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(s.to_str()?.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    Ok(LuaNode::new(id))
                }

                // Number - convert to string (static)
                Value::Integer(n) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(n.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    Ok(LuaNode::new(id))
                }

                Value::Number(n) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(n.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    Ok(LuaNode::new(id))
                }

                // Boolean - convert to string (static)
                Value::Boolean(b) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(b.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    Ok(LuaNode::new(id))
                }

                // Signal or Derived - reactive
                Value::UserData(ref ud) => {
                    // Try to borrow as Signal
                    if ud.is::<LuaSignal>() {
                        create_reactive_text_node(lua, registry_rc, ud.clone())
                    } else if ud.is::<LuaDerived>() {
                        create_reactive_text_node(lua, registry_rc, ud.clone())
                    } else {
                        // Unknown userdata, convert to string
                        let node = UiNode::Text {
                            content: TextContent::Static("<userdata>".to_string()),
                        };
                        let id = registry_rc.borrow_mut().create_node(node);
                        Ok(LuaNode::new(id))
                    }
                }

                // Nil or other - empty string
                _ => {
                    let node = UiNode::Text {
                        content: TextContent::Static("".to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    Ok(LuaNode::new(id))
                }
            }
        });

        // TODO: there must be a best way to define render method lookup
        methods.add_meta_function(
            mlua::MetaMethod::Index,
            |_lua, (ud, key): (AnyUserData, String)| {
                if key == "render" {
                    let uv: mlua::Table = ud.user_value()?;
                    uv.get::<Value>("render")
                } else {
                    Ok(Value::Nil)
                }
            },
        );

        methods.add_meta_function(
            mlua::MetaMethod::NewIndex,
            |_lua, (ud, key, value): (AnyUserData, String, Value)| {
                if key == "render" {
                    let uv: mlua::Table = ud.user_value()?;
                    uv.set("render", value)
                } else {
                    Err(mlua::Error::RuntimeError(format!(
                        "Cannot set field '{}' on rover.ui",
                        key
                    )))
                }
            },
        );
    }
}

/// Create a reactive text node backed by a signal or derived
fn create_reactive_text_node(
    lua: &mlua::Lua,
    registry_rc: Rc<RefCell<UiRegistry>>,
    userdata: mlua::AnyUserData,
) -> mlua::Result<LuaNode> {
    use crate::lua::helpers::get_runtime;
    use crate::signal::arena::SignalId;
    use crate::signal::graph::DerivedId;

    // Determine if this is a signal or derived and get the ID
    enum ReactiveValue {
        Signal(SignalId),
        Derived(DerivedId),
    }

    let (reactive_value, signal_id_opt, derived_id_opt) =
        if let Ok(signal) = userdata.borrow::<LuaSignal>() {
            let id = signal.id;
            (ReactiveValue::Signal(id), Some(id), None)
        } else if let Ok(derived) = userdata.borrow::<LuaDerived>() {
            let id = derived.id;
            (ReactiveValue::Derived(id), None, Some(id))
        } else {
            return Err(mlua::Error::RuntimeError(
                "Expected Signal or Derived".to_string(),
            ));
        };

    // Get the initial value first (before creating effect or reserving node)
    let runtime = get_runtime(lua)?;
    let initial_value = match reactive_value {
        ReactiveValue::Signal(signal_id) => {
            let value = runtime.get_signal(lua, signal_id)?;
            lua_value_to_string(lua, value)?
        }
        ReactiveValue::Derived(derived_id) => {
            let value = runtime
                .get_derived(lua, derived_id)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            lua_value_to_string(lua, value)?
        }
    };

    // Reserve the node ID (before creating the effect)
    let node_id = registry_rc.borrow_mut().reserve_node_id();

    // Clone the Rc for the closure
    let registry_for_callback = registry_rc.clone();

    // Create the effect callback that reads the signal and updates the node
    let callback = if let Some(signal_id) = signal_id_opt {
        lua.create_function(move |lua, ()| {
            let runtime = get_runtime(lua)?;
            let value = runtime.get_signal(lua, signal_id)?;
            let value_str = lua_value_to_string(lua, value)?;

            registry_for_callback
                .borrow_mut()
                .update_text_content(node_id, value_str);

            Ok(())
        })?
    } else if let Some(derived_id) = derived_id_opt {
        lua.create_function(move |lua, ()| {
            let runtime = get_runtime(lua)?;
            let value = runtime
                .get_derived(lua, derived_id)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            let value_str = lua_value_to_string(lua, value)?;

            registry_for_callback
                .borrow_mut()
                .update_text_content(node_id, value_str);

            Ok(())
        })?
    } else {
        unreachable!()
    };

    // Store the callback in the Lua registry
    let callback_key = lua.create_registry_value(callback)?;

    // Create the effect (this will run it immediately)
    let effect_id = runtime
        .create_effect(lua, callback_key)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

    // Finalize the node with reactive content
    let node = UiNode::Text {
        content: TextContent::Reactive {
            current_value: initial_value,
            effect_id,
        },
    };

    {
        let mut registry = registry_rc.borrow_mut();
        registry.finalize_node(node_id, node);
        registry.attach_effect(node_id, effect_id);
        // registry borrow is dropped here
    }

    Ok(LuaNode::new(node_id))
}

/// Convert a Lua value to a display string
fn lua_value_to_string(_lua: &mlua::Lua, value: Value) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(n) => Ok(n.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Nil => Ok("nil".to_string()),
        Value::Table(_) => Ok("<table>".to_string()),
        Value::Function(_) => Ok("<function>".to_string()),
        Value::UserData(_) => Ok("<userdata>".to_string()),
        _ => Ok("<?>".to_string()),
    }
}
