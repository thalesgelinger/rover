use super::lua_node::LuaNode;
use super::node::{TextContent, UiNode};
use super::registry::UiRegistry;
use crate::lua::{derived::LuaDerived, signal::LuaSignal};
use mlua::{AnyUserData, Function, Table, UserData, UserDataMethods, Value};
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

        // rover.ui.button({ label = "text", on_click = function() end })
        methods.add_function("button", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            // Extract label (required)
            let label: String = match props.get::<Value>("label")? {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(n) => n.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => "".to_string(),
            };

            // Extract on_click (optional)
            let on_click = match props.get::<Function>("on_click") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            let node = UiNode::Button { label, on_click };
            let node_id = registry_rc.borrow_mut().create_node(node);

            if let Some(effect_id) = on_click {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.input({ value = signal, on_change = function(val) end, on_submit = function(val) end })
        methods.add_function("input", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            // Reserve the node ID first (before creating the effect)
            let node_id = registry_rc.borrow_mut().reserve_node_id();

            // Extract value (signal or static)
            let value = match props.get::<Value>("value")? {
                Value::UserData(ref ud) => {
                    // Check if it's a Signal or Derived - if so, create proper reactive content
                    if ud.is::<LuaSignal>() || ud.is::<LuaDerived>() {
                        create_reactive_input_value(lua, ud.clone(), node_id, registry_rc.clone())?
                    } else {
                        extract_text_content(lua, Value::UserData(ud.clone()))?
                    }
                }
                v => extract_text_content(lua, v)?,
            };

            // Extract on_change (optional)
            let on_change = match props.get::<Function>("on_change") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            // Extract on_submit (optional — called on Enter)
            let on_submit = match props.get::<Function>("on_submit") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            let node = UiNode::Input {
                value,
                on_change,
                on_submit,
            };

            // Finalize the node
            {
                let mut registry = registry_rc.borrow_mut();
                registry.finalize_node(node_id, node);
            }

            // Attach effects for callbacks
            if let Some(effect_id) = on_change {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }
            if let Some(effect_id) = on_submit {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.checkbox({ checked = boolean, on_toggle = function(checked) end })
        methods.add_function("checkbox", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            // Extract checked (boolean)
            let checked: bool = match props.get::<Value>("checked") {
                Ok(Value::Boolean(b)) => b,
                _ => false,
            };

            // Extract on_toggle (optional)
            let on_toggle = match props.get::<Function>("on_toggle") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            let node = UiNode::Checkbox { checked, on_toggle };
            let node_id = registry_rc.borrow_mut().create_node(node);

            if let Some(effect_id) = on_toggle {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.image({ src = "path/to/image" })
        methods.add_function("image", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            // Extract src (required)
            let src: String = match props.get::<Value>("src")? {
                Value::String(s) => s.to_str()?.to_string(),
                _ => "".to_string(),
            };

            let node = UiNode::Image { src };
            let node_id = registry_rc.borrow_mut().create_node(node);

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.column({ children or varargs })
        methods.add_function("column", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, props)?;

            let node = UiNode::Column { children };
            let node_id = registry_rc.borrow_mut().create_node(node);

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.row({ children or varargs })
        methods.add_function("row", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, props)?;

            let node = UiNode::Row { children };
            let node_id = registry_rc.borrow_mut().create_node(node);

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.view({ children or varargs })
        methods.add_function("view", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, props)?;

            let node = UiNode::View { children };
            let node_id = registry_rc.borrow_mut().create_node(node);

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.when(condition, child_fn)
        // Conditionally render a child based on a condition
        methods.add_function("when", |lua, (condition, child_fn): (Value, Function)| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            // Reserve the conditional node ID (before creating the effect)
            let node_id = registry_rc.borrow_mut().reserve_node_id();

            // Store the child function in registry for later use
            let child_fn_key = lua.create_registry_value(child_fn.clone())?;

            // Determine if condition is a signal/derived and extract the source info
            let reactive_source = if let Value::UserData(ref ud) = condition {
                if ud.is::<LuaSignal>() {
                    let signal = ud.borrow::<LuaSignal>()?;
                    Some(ReactiveSource::Signal(signal.id))
                } else if ud.is::<LuaDerived>() {
                    let derived = ud.borrow::<LuaDerived>()?;
                    Some(ReactiveSource::Derived(derived.id))
                } else {
                    None
                }
            } else {
                None
            };

            // Clone values for the effect callback
            let registry_for_callback = registry_rc.clone();
            let child_fn_key_for_callback = child_fn_key;

            // Create the effect callback that evaluates the condition and mounts/unmounts the child
            // If reactive, read from the signal/derived to track dependencies
            let effect_callback = lua.create_function(move |lua, ()| {
                // Read the condition (this tracks signal/derived dependencies)
                let should_show = if let Some(source) = &reactive_source {
                    // Reactive source - read from it to track dependency
                    let runtime = crate::lua::helpers::get_runtime(lua)?;
                    let value = source.get_value(lua, &runtime)?;
                    evaluate_condition(lua, &value)?
                } else {
                    // Static condition - just use the static value
                    // For static conditions, the effect won't re-run, so we store the initial value
                    registry_for_callback
                        .borrow()
                        .get_condition_state(node_id)
                        .unwrap_or(false)
                };

                // Update the registry state
                registry_for_callback
                    .borrow_mut()
                    .set_condition_state(node_id, should_show);

                if should_show {
                    // Condition is true - mount child if not already mounted
                    // Check if child exists first (immutable borrow, then dropped)
                    let child_exists = {
                        let registry = registry_for_callback.borrow();
                        registry.get_condition_child(node_id).is_some()
                    };

                    if !child_exists {
                        // Child not mounted, create it
                        let child_fn: Function = lua
                            .registry_value(&child_fn_key_for_callback)
                            .map_err(|e| {
                                mlua::Error::RuntimeError(format!(
                                    "Failed to get child function: {}",
                                    e
                                ))
                            })?;

                        // Call child function WITHOUT holding registry borrow
                        let child_node: LuaNode = child_fn.call(()).map_err(|e| {
                            mlua::Error::RuntimeError(format!("Child function error: {}", e))
                        })?;

                        // Now update registry (mutably)
                        let mut registry = registry_for_callback.borrow_mut();
                        registry.set_condition_child(node_id, child_node.id());
                        registry.mark_dirty(node_id);
                    }
                } else {
                    // Condition is false - unmount child if mounted
                    // Check if child exists first (immutable borrow, then dropped)
                    let child_id = {
                        let registry = registry_for_callback.borrow();
                        registry.get_condition_child(node_id)
                    };

                    if let Some(_id) = child_id {
                        // Child mounted, remove it
                        let mut registry = registry_for_callback.borrow_mut();
                        registry.remove_condition_child(node_id);
                        registry.mark_dirty(node_id);
                    }
                }

                Ok(())
            })?;

            let effect_key = lua.create_registry_value(effect_callback)?;

            // Evaluate initial condition for static conditions
            if reactive_source.is_none() {
                let initial_condition = evaluate_condition(lua, &condition)?;
                registry_rc
                    .borrow_mut()
                    .set_condition_state(node_id, initial_condition);
            }

            // Create a tracking effect for the condition
            // This will call the callback immediately, which reads the signal/derived and tracks dependencies
            let condition_effect_id = runtime
                .create_effect(lua, effect_key)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            // Create the conditional node
            let node = UiNode::Conditional {
                condition_effect: condition_effect_id,
                child: None,
            };

            {
                let mut registry = registry_rc.borrow_mut();
                registry.finalize_node(node_id, node);
                registry.attach_effect(node_id, condition_effect_id);
            }

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.each(items, render_fn, key_fn)
        // Render a list of items with reconciliation
        methods.add_function(
            "each",
            |lua, (items, render_fn, _key_fn): (Value, Function, Function)| {
                let registry_rc = get_registry_rc(lua)?;
                let runtime = crate::lua::helpers::get_runtime(lua)?;

                // Reserve the list node ID
                let node_id = registry_rc.borrow_mut().reserve_node_id();

                // Store render function in registry
                let render_fn_key = lua.create_registry_value(render_fn.clone())?;

                // Determine if items is a signal/derived and extract the source info
                let reactive_source = if let Value::UserData(ref ud) = items {
                    if ud.is::<LuaSignal>() {
                        let signal = ud.borrow::<LuaSignal>()?;
                        Some(ReactiveSource::Signal(signal.id))
                    } else if ud.is::<LuaDerived>() {
                        let derived = ud.borrow::<LuaDerived>()?;
                        Some(ReactiveSource::Derived(derived.id))
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Clone for effect callback
                let registry_for_callback = registry_rc.clone();
                let render_fn_key_clone = render_fn_key;

                // Create the effect callback that updates the list when items change
                // If reactive, read from the signal/derived to track dependencies
                let effect_callback = lua.create_function(move |lua, ()| {
                    // Read the items (this tracks signal/derived dependencies)
                    let items_value = if let Some(source) = &reactive_source {
                        // Reactive source - read from it to track dependency
                        let runtime = crate::lua::helpers::get_runtime(lua)?;
                        source.get_value(lua, &runtime)?
                    } else {
                        // Static items - use stored value
                        registry_for_callback
                            .borrow()
                            .get_list_items(node_id)
                            .clone()
                    };

                    // Extract items table
                    let items_array = match &items_value {
                        Value::Table(t) => t.clone(),
                        _ => return Ok(()), // No items, nothing to do
                    };

                    // Get current children to remove them (immutable borrow, scoped to be dropped)
                    let current_children = {
                        let registry = registry_for_callback.borrow();
                        registry.get_list_children(node_id).to_vec()
                    };

                    // Remove all current children (single mutable borrow)
                    {
                        let mut registry = registry_for_callback.borrow_mut();
                        for child_id in current_children {
                            registry.remove_node(child_id);
                        }
                    }

                    let mut new_children = Vec::new();

                    // Iterate through items and create new children
                    let mut i = 1;
                    while let Ok(item) = items_array.get::<Value>(i) {
                        if item == Value::Nil {
                            break;
                        }

                        // Create new child (NO registry borrow held during this call)
                        let render_fn: Function =
                            lua.registry_value(&render_fn_key_clone).map_err(|e| {
                                mlua::Error::RuntimeError(format!(
                                    "Failed to get render function: {}",
                                    e
                                ))
                            })?;

                        let child_node: LuaNode = render_fn.call((item, i)).map_err(|e| {
                            mlua::Error::RuntimeError(format!("Render function error: {}", e))
                        })?;

                        new_children.push(child_node.id());
                        i += 1;
                    }

                    // Update the list node's children (mutable borrow after all children created)
                    registry_for_callback
                        .borrow_mut()
                        .update_list_children(node_id, new_children);

                    Ok(())
                })?;

                let effect_key = lua.create_registry_value(effect_callback)?;

                // Store initial items for static conditions
                if reactive_source.is_none() {
                    registry_rc
                        .borrow_mut()
                        .set_list_items(node_id, items.clone());
                }

                // Create the list node FIRST with empty children
                // The effect will populate it after we create it
                let node = UiNode::List {
                    items_effect: crate::signal::graph::EffectId(0), // Placeholder, will be updated below
                    children: Vec::new(),
                };

                {
                    let mut registry = registry_rc.borrow_mut();
                    registry.finalize_node(node_id, node);
                }

                // NOW create the effect
                // This will call the callback immediately, which will populate the list
                let items_effect_id = runtime
                    .create_effect(lua, effect_key)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

                {
                    let mut registry = registry_rc.borrow_mut();
                    // Get the children that were created by the effect
                    let children = registry.get_list_children(node_id).to_vec();
                    // Update the node's effect ID and children
                    if let Some(UiNode::List {
                        items_effect,
                        children: node_children,
                    }) = registry.get_node_mut(node_id)
                    {
                        *items_effect = items_effect_id;
                        *node_children = children;
                    }
                    registry.attach_effect(node_id, items_effect_id);
                }

                Ok(LuaNode::new(node_id))
            },
        );

        // TODO: there must be a best way to define render method lookup
        methods.add_meta_function(
            mlua::MetaMethod::Index,
            |_lua, (ud, key): (AnyUserData, String)| {
                let uv: mlua::Table = ud.user_value()?;
                if key == "render" {
                    uv.get::<Value>("render")
                } else {
                    uv.get::<Value>(key)
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

/// Helper enum to unify Signal and Derived handling
#[derive(Clone, Copy)]
enum ReactiveSource {
    Signal(crate::signal::arena::SignalId),
    Derived(crate::signal::graph::DerivedId),
}

impl ReactiveSource {
    /// Get the current value of this reactive source
    fn get_value(
        &self,
        lua: &mlua::Lua,
        runtime: &crate::SharedSignalRuntime,
    ) -> mlua::Result<Value> {
        match self {
            ReactiveSource::Signal(id) => runtime.get_signal(lua, *id),
            ReactiveSource::Derived(id) => runtime
                .get_derived(lua, *id)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string())),
        }
    }
}

/// Create a reactive text node backed by a signal or derived
fn create_reactive_text_node(
    lua: &mlua::Lua,
    registry_rc: Rc<RefCell<UiRegistry>>,
    userdata: mlua::AnyUserData,
) -> mlua::Result<LuaNode> {
    use crate::lua::helpers::get_runtime;

    // Determine if this is a signal or derived and get the ID
    let reactive_source = if let Ok(signal) = userdata.borrow::<LuaSignal>() {
        ReactiveSource::Signal(signal.id)
    } else if let Ok(derived) = userdata.borrow::<LuaDerived>() {
        ReactiveSource::Derived(derived.id)
    } else {
        return Err(mlua::Error::RuntimeError(
            "Expected Signal or Derived".to_string(),
        ));
    };

    // Get the initial value first (before creating effect or reserving node)
    let runtime = get_runtime(lua)?;
    let initial_value = {
        let value = reactive_source.get_value(lua, &runtime)?;
        lua_value_to_string(lua, value)?
    };

    // Reserve the node ID (before creating the effect)
    let node_id = registry_rc.borrow_mut().reserve_node_id();

    // Clone the Rc for the closure
    let registry_for_callback = registry_rc.clone();

    // Create the effect callback that reads the reactive source and updates the node
    let callback = lua.create_function(move |lua, ()| {
        let runtime = get_runtime(lua)?;
        let value = reactive_source.get_value(lua, &runtime)?;
        let value_str = lua_value_to_string(lua, value)?;

        registry_for_callback
            .borrow_mut()
            .update_text_content(node_id, value_str);

        Ok(())
    })?;

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
            signal_id: None, // Text nodes don't need two-way binding
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

/// Create reactive text content for an Input node with proper updating.
/// Unlike create_reactive_text_node (for Text nodes), this returns TextContent
/// and sets up the effect to update the node's display value when the signal changes.
fn create_reactive_input_value(
    lua: &mlua::Lua,
    userdata: mlua::AnyUserData,
    node_id: super::node::NodeId,
    registry_rc: Rc<RefCell<UiRegistry>>,
) -> mlua::Result<TextContent> {
    use crate::lua::helpers::get_runtime;

    // Determine if this is a signal or derived and get the ID
    // For signals, we store the ID for two-way binding (input fields can update the signal)
    let (reactive_source, signal_id) = if let Ok(signal) = userdata.borrow::<LuaSignal>() {
        (ReactiveSource::Signal(signal.id), Some(signal.id))
    } else if let Ok(derived) = userdata.borrow::<LuaDerived>() {
        (ReactiveSource::Derived(derived.id), None)
    } else {
        return Ok(TextContent::Static("<error>".to_string()));
    };

    // Get the initial value
    let runtime = get_runtime(lua)?;
    let initial_value = {
        let value = reactive_source.get_value(lua, &runtime)?;
        lua_value_to_string(lua, value)?
    };

    // Clone for the closure
    let registry_for_callback = registry_rc.clone();

    // Create the effect callback that updates the input's display value
    let callback = lua.create_function(move |lua, ()| {
        let runtime = get_runtime(lua)?;
        let value = reactive_source.get_value(lua, &runtime)?;
        let value_str = lua_value_to_string(lua, value)?;

        // Update the node's text content so the renderer displays the new value
        registry_for_callback
            .borrow_mut()
            .update_text_content(node_id, value_str);

        Ok(())
    })?;

    // Store the callback in the Lua registry
    let callback_key = lua.create_registry_value(callback)?;

    // Create the effect (this will run it immediately)
    let effect_id = runtime
        .create_effect(lua, callback_key)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

    // Attach the effect to the node
    registry_rc.borrow_mut().attach_effect(node_id, effect_id);

    Ok(TextContent::Reactive {
        current_value: initial_value,
        effect_id,
        signal_id,
    })
}

/// Extract text content from a Lua value (static string, signal, or derived)
fn extract_text_content(lua: &mlua::Lua, value: Value) -> mlua::Result<TextContent> {
    match value {
        // Static string
        Value::String(s) => Ok(TextContent::Static(s.to_str()?.to_string())),

        // Number - convert to string (static)
        Value::Integer(n) => Ok(TextContent::Static(n.to_string())),
        Value::Number(n) => Ok(TextContent::Static(n.to_string())),

        // Boolean - convert to string (static)
        Value::Boolean(b) => Ok(TextContent::Static(b.to_string())),

        // Signal or Derived - reactive
        Value::UserData(ref ud) => {
            // Try to borrow as Signal
            if ud.is::<LuaSignal>() {
                extract_reactive_text_content(lua, ud.clone())
            } else if ud.is::<LuaDerived>() {
                extract_reactive_text_content(lua, ud.clone())
            } else {
                // Unknown userdata, convert to string
                Ok(TextContent::Static("<userdata>".to_string()))
            }
        }

        // Nil or other - empty string
        _ => Ok(TextContent::Static("".to_string())),
    }
}

/// Create reactive text content from a signal or derived
fn extract_reactive_text_content(
    lua: &mlua::Lua,
    userdata: mlua::AnyUserData,
) -> mlua::Result<TextContent> {
    use crate::lua::helpers::get_runtime;

    // Determine if this is a signal or derived and get the ID
    // For signals, we store the ID for two-way binding (input fields can update the signal)
    let (reactive_source, signal_id) = if let Ok(signal) = userdata.borrow::<LuaSignal>() {
        (ReactiveSource::Signal(signal.id), Some(signal.id))
    } else if let Ok(derived) = userdata.borrow::<LuaDerived>() {
        (ReactiveSource::Derived(derived.id), None)
    } else {
        return Ok(TextContent::Static("<error>".to_string()));
    };

    // Get the initial value first (before creating effect or reserving node)
    let runtime = get_runtime(lua)?;
    let initial_value = {
        let value = reactive_source.get_value(lua, &runtime)?;
        lua_value_to_string(lua, value)?
    };

    // We need to create a temporary node ID for the effect
    // This is a bit hacky - the real solution would be to restructure this
    // For now, we'll create a placeholder node ID
    let _node_id = super::node::NodeId(0); // Placeholder, will be updated by the caller

    // Clone the Rc for the closure
    let _registry_rc = get_registry_rc(lua)?;

    // Create the effect callback that reads the reactive source and updates the node
    let callback = lua.create_function(move |lua, ()| {
        let runtime = get_runtime(lua)?;
        let value = reactive_source.get_value(lua, &runtime)?;
        let _value_str = lua_value_to_string(lua, value)?;

        // For input components, we need to update the node's value
        // This requires special handling since we don't have the node_id here
        // We'll handle this differently for input nodes

        Ok(())
    })?;

    // Store the callback in the Lua registry
    let callback_key = lua.create_registry_value(callback)?;

    // Create the effect (this will run it immediately)
    let effect_id = runtime
        .create_effect(lua, callback_key)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

    Ok(TextContent::Reactive {
        current_value: initial_value,
        effect_id,
        signal_id,
    })
}

/// Extract children from a props table
/// Supports both `props.children` array and varargs through `props[1], props[2], ...`
fn extract_children(lua: &mlua::Lua, props: Table) -> mlua::Result<Vec<super::node::NodeId>> {
    let mut children = Vec::new();

    // First try props.children
    if let Ok(Value::Table(children_table)) = props.get::<Value>("children") {
        for pair in children_table.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Ok(node) = extract_node_id(lua, value) {
                children.push(node);
            }
        }
    } else {
        // Try varargs: props[1], props[2], etc.
        // NOTE: In Lua, accessing non-existent index returns Nil, not error
        let mut i = 1;
        loop {
            match props.get::<Value>(i) {
                Ok(Value::Nil) => break, // No more varargs
                Ok(value) => {
                    if let Ok(node) = extract_node_id(lua, value) {
                        children.push(node);
                    }
                }
                Err(_) => break, // Should not happen, but break on error too
            }
            i += 1;
        }
    }

    Ok(children)
}

/// Extract a node ID from a Lua value (LuaNode or compatible)
fn extract_node_id(_lua: &mlua::Lua, value: Value) -> mlua::Result<super::node::NodeId> {
    match value {
        Value::UserData(ud) => {
            if let Ok(node) = ud.borrow::<LuaNode>() {
                Ok(node.id())
            } else {
                Err(mlua::Error::RuntimeError("Expected LuaNode".to_string()))
            }
        }
        Value::Table(t) => {
            // Try to get id field
            if let Ok(id) = t.get::<Value>("id") {
                if let Value::Integer(n) = id {
                    Ok(super::node::NodeId(n as u32))
                } else if let Value::Number(n) = id {
                    Ok(super::node::NodeId(n as u32))
                } else {
                    Err(mlua::Error::RuntimeError("Invalid node id".to_string()))
                }
            } else {
                Err(mlua::Error::RuntimeError(
                    "Expected node with id field".to_string(),
                ))
            }
        }
        _ => Err(mlua::Error::RuntimeError(format!(
            "Expected node, got {:?}",
            value
        ))),
    }
}

/// Evaluate a condition value (signal, derived, or boolean)
fn evaluate_condition(lua: &mlua::Lua, condition: &Value) -> mlua::Result<bool> {
    match condition {
        // Boolean - direct value
        Value::Boolean(b) => Ok(*b),

        // Signal or Derived - get reactive value
        Value::UserData(ud) => {
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            // Try as signal
            if ud.is::<LuaSignal>() {
                let signal = ud.borrow::<LuaSignal>()?;
                let value = runtime.get_signal(lua, signal.id)?;
                match value {
                    Value::Boolean(b) => Ok(b),
                    Value::Integer(n) => Ok(n != 0),
                    Value::Number(n) => Ok(n != 0.0),
                    Value::Nil => Ok(false),
                    _ => Ok(true), // Other truthy values
                }
            } else if ud.is::<LuaDerived>() {
                let derived = ud.borrow::<LuaDerived>()?;
                let value = runtime
                    .get_derived(lua, derived.id)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                match value {
                    Value::Boolean(b) => Ok(b),
                    Value::Integer(n) => Ok(n != 0),
                    Value::Number(n) => Ok(n != 0.0),
                    Value::Nil => Ok(false),
                    _ => Ok(true), // Other truthy values
                }
            } else {
                // Unknown userdata - try to convert to boolean
                Ok(true)
            }
        }

        // Integer/Number - truthy if non-zero
        Value::Integer(n) => Ok(*n != 0),
        Value::Number(n) => Ok(*n != 0.0),

        // Nil is falsy
        Value::Nil => Ok(false),

        // Everything else is truthy
        _ => Ok(true),
    }
}
