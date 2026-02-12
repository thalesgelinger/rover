use super::lua_node::LuaNode;
use super::node::{TextContent, UiNode};
use super::registry::UiRegistry;
use super::style::NodeStyle;
use crate::lua::{derived::LuaDerived, signal::LuaSignal};
use mlua::{AnyUserData, Function, Table, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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

fn get_ui_user_value_table(lua: &mlua::Lua) -> mlua::Result<Table> {
    let rover: Table = lua.globals().get("rover")?;
    let ui_ud: AnyUserData = rover.get("ui")?;
    ui_ud.user_value()
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
                    apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
                    Ok(LuaNode::new(id))
                }

                // Number - convert to string (static)
                Value::Integer(n) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(n.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
                    Ok(LuaNode::new(id))
                }

                Value::Number(n) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(n.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
                    Ok(LuaNode::new(id))
                }

                // Boolean - convert to string (static)
                Value::Boolean(b) => {
                    let node = UiNode::Text {
                        content: TextContent::Static(b.to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
                    Ok(LuaNode::new(id))
                }

                // Signal or Derived - reactive
                Value::UserData(ref ud) => {
                    // Try to borrow as Signal
                    if ud.is::<LuaSignal>() {
                        let node = create_reactive_text_node(lua, registry_rc.clone(), ud.clone())?;
                        apply_mod_to_node(lua, registry_rc.clone(), &props, node.id())?;
                        Ok(node)
                    } else if ud.is::<LuaDerived>() {
                        let node = create_reactive_text_node(lua, registry_rc.clone(), ud.clone())?;
                        apply_mod_to_node(lua, registry_rc.clone(), &props, node.id())?;
                        Ok(node)
                    } else {
                        // Unknown userdata, convert to string
                        let node = UiNode::Text {
                            content: TextContent::Static("<userdata>".to_string()),
                        };
                        let id = registry_rc.borrow_mut().create_node(node);
                        apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
                        Ok(LuaNode::new(id))
                    }
                }

                // Nil or other - empty string
                _ => {
                    let node = UiNode::Text {
                        content: TextContent::Static("".to_string()),
                    };
                    let id = registry_rc.borrow_mut().create_node(node);
                    apply_mod_to_node(lua, registry_rc.clone(), &props, id)?;
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
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

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
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

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
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

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
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.column({ children or varargs })
        methods.add_function("column", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, &props)?;

            let node = UiNode::Column { children };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.row({ children or varargs })
        methods.add_function("row", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, &props)?;

            let node = UiNode::Row { children };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.view({ children or varargs })
        methods.add_function("view", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, &props)?;

            let node = UiNode::View { children };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.stack({ children or varargs })
        methods.add_function("stack", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;

            let children = extract_children(lua, &props)?;

            let node = UiNode::Stack { children };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.full_screen({ on_key = function(key) end, child })
        methods.add_function("full_screen", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            let child = match props.get::<Value>(1) {
                Ok(Value::Nil) | Err(_) => None,
                Ok(v) => Some(extract_node_id(lua, v)?),
            };

            let on_key = match props.get::<Function>("on_key") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            let node = UiNode::FullScreen { child, on_key };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            if let Some(effect_id) = on_key {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }

            Ok(LuaNode::new(node_id))
        });

        // rover.ui.key_area({ on_key = function(key) end, node })
        methods.add_function("key_area", |lua, props: Table| {
            let registry_rc = get_registry_rc(lua)?;
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            let child = match props.get::<Value>(1) {
                Ok(Value::Nil) | Err(_) => None,
                Ok(v) => Some(extract_node_id(lua, v)?),
            };

            let on_key = match props.get::<Function>("on_key") {
                Ok(callback) => Some(
                    runtime
                        .register_callback(lua, callback)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                ),
                Err(_) => None,
            };

            let node = UiNode::KeyArea { child, on_key };
            let node_id = registry_rc.borrow_mut().create_node(node);
            apply_mod_to_node(lua, registry_rc.clone(), &props, node_id)?;

            if let Some(effect_id) = on_key {
                registry_rc.borrow_mut().attach_effect(node_id, effect_id);
            }

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
                        let removed_child_id = {
                            let mut registry = registry_for_callback.borrow_mut();
                            registry.remove_condition_child(node_id)
                        };

                        if let Some(child_to_remove) = removed_child_id {
                            remove_node_subtree(
                                lua,
                                registry_for_callback.clone(),
                                child_to_remove,
                            )?;
                        }

                        registry_for_callback.borrow_mut().mark_dirty(node_id);
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

        // rover.ui.each(items, render_fn, key_fn?)
        methods.add_function(
            "each",
            |lua, (items, render_fn, key_fn_value): (Value, Function, Value)| {
                let registry_rc = get_registry_rc(lua)?;
                let runtime = crate::lua::helpers::get_runtime(lua)?;

                // Reserve the list node ID
                let node_id = registry_rc.borrow_mut().reserve_node_id();

                // Store render function in registry
                let render_fn_key = lua.create_registry_value(render_fn.clone())?;
                let key_fn_key = match key_fn_value {
                    Value::Function(f) => Some(lua.create_registry_value(f)?),
                    _ => None,
                };

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
                let row_cache_for_callback =
                    Rc::new(RefCell::new(HashMap::<String, ListRowState>::new()));

                // Create the effect callback that updates the list when items change
                // If reactive, read from the signal/derived to track dependencies
                let effect_callback = lua.create_function(move |lua, ()| {
                    let runtime = crate::lua::helpers::get_runtime(lua)?;

                    // Read the items (this tracks signal/derived dependencies)
                    let items_value = if let Some(source) = &reactive_source {
                        // Reactive source - read from it to track dependency
                        source.get_value(lua, &runtime)?
                    } else {
                        // Static items - use stored value
                        registry_for_callback
                            .borrow()
                            .get_list_items(node_id)
                            .clone()
                    };

                    // Extract items table (non-table means empty list)
                    let items_array = match &items_value {
                        Value::Table(t) => Some(t.clone()),
                        _ => None,
                    };

                    let mut new_children = Vec::new();
                    let mut new_cache = HashMap::<String, ListRowState>::new();
                    let mut seen_keys = HashSet::<String>::new();
                    let mut old_cache = std::mem::take(&mut *row_cache_for_callback.borrow_mut());

                    // Iterate through items and reconcile by key
                    if let Some(items_array) = items_array {
                        let len = items_array.raw_len();
                        let mut i = 1usize;
                        while i <= len {
                            let item = items_array.get::<Value>(i)?;
                            if item == Value::Nil {
                                i += 1;
                                continue;
                            }

                            let key = resolve_list_item_key(lua, key_fn_key.as_ref(), &item, i)?;
                            if !seen_keys.insert(key.clone()) {
                                return Err(mlua::Error::RuntimeError(format!(
                                    "Duplicate key '{}' in rover.ui.each",
                                    key
                                )));
                            }

                            if let Some(mut row_state) = old_cache.remove(&key) {
                                update_row_state(lua, &runtime, &mut row_state, item.clone(), i)?;
                                new_children.push(row_state.child_id);
                                new_cache.insert(key, row_state);
                            } else {
                                // Create new child
                                let render_fn: Function =
                                    lua.registry_value(&render_fn_key_clone).map_err(|e| {
                                        mlua::Error::RuntimeError(format!(
                                            "Failed to get render function: {}",
                                            e
                                        ))
                                    })?;

                                let (render_item, render_index, item_binding, index_signal) =
                                    create_row_binding(lua, &runtime, item.clone(), i)?;

                                let child_node: LuaNode =
                                    render_fn.call((render_item, render_index)).map_err(|e| {
                                        mlua::Error::RuntimeError(format!(
                                            "Render function error: {}",
                                            e
                                        ))
                                    })?;

                                let row_state = ListRowState {
                                    child_id: child_node.id(),
                                    index_signal,
                                    item_binding,
                                };

                                new_children.push(row_state.child_id);
                                new_cache.insert(key, row_state);
                            }

                            i += 1;
                        }
                    }

                    // Remove old rows not present anymore
                    for (_, stale_row) in old_cache {
                        remove_node_subtree(
                            lua,
                            registry_for_callback.clone(),
                            stale_row.child_id,
                        )?;
                    }

                    *row_cache_for_callback.borrow_mut() = new_cache;

                    // Update the list node's children after reconciliation
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

        methods.add_function("set_theme", |lua, theme: Table| {
            let uv: Table = get_ui_user_value_table(lua)?;
            let current_theme: Table = uv.get("theme")?;
            replace_table_recursive(&current_theme, &theme)?;
            Ok(())
        });

        methods.add_function("extend_theme", |lua, patch: Table| {
            let uv: Table = get_ui_user_value_table(lua)?;
            let current_theme: Table = uv.get("theme")?;
            merge_tables_recursive(&current_theme, &patch)?;
            Ok(())
        });

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
                } else if key == "theme" {
                    let uv: mlua::Table = ud.user_value()?;
                    match value {
                        Value::Table(theme) => {
                            let current_theme: Table = uv.get("theme")?;
                            replace_table_recursive(&current_theme, &theme)
                        }
                        _ => Err(mlua::Error::RuntimeError(
                            "rover.ui.theme must be a table".to_string(),
                        )),
                    }
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

struct ListRowState {
    child_id: super::node::NodeId,
    index_signal: crate::signal::arena::SignalId,
    item_binding: RowItemBinding,
}

enum RowItemBinding {
    PassThrough,
    Primitive {
        signal_id: crate::signal::arena::SignalId,
    },
    TableFields {
        proxy_table_key: mlua::RegistryKey,
        field_signals: HashMap<String, crate::signal::arena::SignalId>,
    },
}

fn resolve_list_item_key(
    lua: &mlua::Lua,
    key_fn_key: Option<&mlua::RegistryKey>,
    item: &Value,
    index: usize,
) -> mlua::Result<String> {
    let key_value = if let Some(key_fn_key) = key_fn_key {
        let key_fn: Function = lua.registry_value(key_fn_key).map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to get each key function: {}", e))
        })?;
        key_fn
            .call::<Value>((item.clone(), index as i64))
            .map_err(|e| mlua::Error::RuntimeError(format!("each key function error: {}", e)))?
    } else {
        Value::Integer(index as i64)
    };

    key_value_to_string(&key_value)
}

fn key_value_to_string(value: &Value) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(format!("s:{}", s.to_str()?)),
        Value::Integer(n) => Ok(format!("i:{}", n)),
        Value::Number(n) => Ok(format!("n:{:016x}", n.to_bits())),
        Value::Boolean(b) => Ok(format!("b:{}", b)),
        Value::Nil => Err(mlua::Error::RuntimeError(
            "each key function returned nil".to_string(),
        )),
        Value::UserData(ud) => {
            if let Ok(signal) = ud.borrow::<LuaSignal>() {
                Ok(format!("us:{}", signal.id.0))
            } else if let Ok(derived) = ud.borrow::<LuaDerived>() {
                Ok(format!("ud:{}", derived.id.0))
            } else {
                Err(mlua::Error::RuntimeError(
                    "each key function returned unsupported userdata key".to_string(),
                ))
            }
        }
        _ => Err(mlua::Error::RuntimeError(
            "each key function must return string/number/integer/boolean".to_string(),
        )),
    }
}

fn create_row_binding(
    lua: &mlua::Lua,
    runtime: &crate::SharedSignalRuntime,
    item: Value,
    index: usize,
) -> mlua::Result<(Value, Value, RowItemBinding, crate::signal::arena::SignalId)> {
    let index_signal = runtime.create_signal(crate::signal::SignalValue::Int(index as i64));
    let index_value = Value::UserData(lua.create_userdata(LuaSignal::new(index_signal))?);

    if let Value::UserData(ref ud) = item {
        if ud.is::<LuaSignal>() || ud.is::<LuaDerived>() {
            return Ok((item, index_value, RowItemBinding::PassThrough, index_signal));
        }
    }

    if let Value::Table(table) = item {
        let proxy = lua.create_table()?;
        let mut field_signals = HashMap::new();

        for pair in table.pairs::<Value, Value>() {
            let (key, value) = pair?;
            match &key {
                Value::String(s) => {
                    let field_name = s.to_str()?.to_string();
                    let signal_value = crate::signal::SignalValue::from_lua(lua, value)?;
                    let field_signal_id = runtime.create_signal(signal_value);
                    let field_ud = lua.create_userdata(LuaSignal::new(field_signal_id))?;
                    proxy.set(key, Value::UserData(field_ud))?;
                    field_signals.insert(field_name, field_signal_id);
                }
                _ => {
                    proxy.set(key, value)?;
                }
            }
        }

        let proxy_key = lua.create_registry_value(proxy.clone())?;
        return Ok((
            Value::Table(proxy),
            index_value,
            RowItemBinding::TableFields {
                proxy_table_key: proxy_key,
                field_signals,
            },
            index_signal,
        ));
    }

    let signal_value = crate::signal::SignalValue::from_lua(lua, item)?;
    let item_signal_id = runtime.create_signal(signal_value);
    let item_value = Value::UserData(lua.create_userdata(LuaSignal::new(item_signal_id))?);

    Ok((
        item_value,
        index_value,
        RowItemBinding::Primitive {
            signal_id: item_signal_id,
        },
        index_signal,
    ))
}

fn update_row_state(
    lua: &mlua::Lua,
    runtime: &crate::SharedSignalRuntime,
    row_state: &mut ListRowState,
    item: Value,
    index: usize,
) -> mlua::Result<()> {
    runtime.set_signal(
        lua,
        row_state.index_signal,
        crate::signal::SignalValue::Int(index as i64),
    );

    match &mut row_state.item_binding {
        RowItemBinding::PassThrough => Ok(()),
        RowItemBinding::Primitive { signal_id } => {
            let signal_value = crate::signal::SignalValue::from_lua(lua, item)?;
            runtime.set_signal(lua, *signal_id, signal_value);
            Ok(())
        }
        RowItemBinding::TableFields {
            proxy_table_key,
            field_signals,
        } => {
            let table = match item {
                Value::Table(t) => t,
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "List item type changed from table to non-table for existing key"
                            .to_string(),
                    ));
                }
            };

            let proxy_table: Table = lua.registry_value(proxy_table_key).map_err(|e| {
                mlua::Error::RuntimeError(format!("Failed to load list item proxy table: {}", e))
            })?;

            let mut seen = HashSet::new();
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if let Value::String(s) = &key {
                    let field_name = s.to_str()?.to_string();
                    seen.insert(field_name.clone());
                    if let Some(signal_id) = field_signals.get(&field_name).copied() {
                        let signal_value = crate::signal::SignalValue::from_lua(lua, value)?;
                        runtime.set_signal(lua, signal_id, signal_value);
                    } else {
                        let signal_value = crate::signal::SignalValue::from_lua(lua, value)?;
                        let signal_id = runtime.create_signal(signal_value);
                        field_signals.insert(field_name.clone(), signal_id);
                        let signal_ud = lua.create_userdata(LuaSignal::new(signal_id))?;
                        proxy_table.set(key, Value::UserData(signal_ud))?;
                    }
                }
            }

            // Keys that disappeared become nil, preserving existing signal identity.
            for (field_name, signal_id) in field_signals.iter() {
                if !seen.contains(field_name) {
                    runtime.set_signal(lua, *signal_id, crate::signal::SignalValue::Nil);
                }
            }

            Ok(())
        }
    }
}

fn list_child_ids(node: &UiNode) -> Vec<super::node::NodeId> {
    match node {
        UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::View { children }
        | UiNode::Stack { children }
        | UiNode::List { children, .. } => children.clone(),
        UiNode::Conditional { child, .. }
        | UiNode::KeyArea { child, .. }
        | UiNode::FullScreen { child, .. } => child.iter().copied().collect(),
        _ => Vec::new(),
    }
}

fn collect_subtree_postorder(
    registry: &UiRegistry,
    node_id: super::node::NodeId,
    out: &mut Vec<super::node::NodeId>,
) {
    if let Some(node) = registry.get_node(node_id) {
        for child_id in list_child_ids(node) {
            collect_subtree_postorder(registry, child_id, out);
        }
    }
    out.push(node_id);
}

fn remove_node_subtree(
    lua: &mlua::Lua,
    registry_rc: Rc<RefCell<UiRegistry>>,
    root_id: super::node::NodeId,
) -> mlua::Result<()> {
    let runtime = crate::lua::helpers::get_runtime(lua)?;

    let mut postorder = Vec::new();
    {
        let registry = registry_rc.borrow();
        collect_subtree_postorder(&registry, root_id, &mut postorder);
    }

    let mut effects_to_dispose = Vec::new();
    {
        let mut registry = registry_rc.borrow_mut();
        for node_id in postorder {
            if let Some((_node, effects)) = registry.remove_node(node_id) {
                effects_to_dispose.extend(effects);
            }
        }
    }

    for effect_id in effects_to_dispose {
        runtime
            .dispose_effect(lua, effect_id)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
    }

    Ok(())
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

fn apply_mod_to_node(
    lua: &mlua::Lua,
    registry_rc: Rc<RefCell<UiRegistry>>,
    props: &Table,
    node_id: super::node::NodeId,
) -> mlua::Result<()> {
    let mod_value = props.get::<Value>("mod")?;
    let mod_table = match mod_value {
        Value::Table(t) => t,
        Value::Nil => return Ok(()),
        _ => {
            return Err(mlua::Error::RuntimeError(
                "props.mod must be a modifier table".to_string(),
            ));
        }
    };

    let resolve: mlua::Function = mod_table.get("resolve")?;
    let resolved: Table = resolve.call(mod_table.clone())?;
    let style = NodeStyle::from_lua_table(&resolved)?;
    registry_rc.borrow_mut().set_node_style(node_id, style);

    let is_reactive: mlua::Function = mod_table.get("is_reactive")?;
    let reactive: bool = is_reactive.call(mod_table.clone())?;
    if !reactive {
        return Ok(());
    }

    let runtime = crate::lua::helpers::get_runtime(lua)?;
    let registry_for_callback = registry_rc.clone();
    let callback = lua.create_function(move |_lua, ()| {
        let resolve: mlua::Function = mod_table.get("resolve")?;
        let resolved: Table = resolve.call(mod_table.clone())?;
        let style = NodeStyle::from_lua_table(&resolved)?;
        registry_for_callback
            .borrow_mut()
            .set_node_style(node_id, style);
        Ok(())
    })?;

    let callback_key = lua.create_registry_value(callback)?;
    let effect_id = runtime
        .create_effect(lua, callback_key)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

    registry_rc.borrow_mut().attach_effect(node_id, effect_id);
    Ok(())
}

fn replace_table_recursive(dst: &Table, src: &Table) -> mlua::Result<()> {
    let mut keys = Vec::new();
    for pair in dst.clone().pairs::<Value, Value>() {
        let (key, _value) = pair?;
        keys.push(key);
    }

    for key in keys {
        dst.raw_remove(key)?;
    }

    merge_tables_recursive(dst, src)
}

fn merge_tables_recursive(dst: &Table, src: &Table) -> mlua::Result<()> {
    for pair in src.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        match (dst.get::<Value>(key.clone())?, value.clone()) {
            (Value::Table(dst_nested), Value::Table(src_nested)) => {
                merge_tables_recursive(&dst_nested, &src_nested)?;
            }
            _ => {
                dst.set(key, value)?;
            }
        }
    }
    Ok(())
}

/// Extract children from a props table
/// Supports both `props.children` array and varargs through `props[1], props[2], ...`
fn extract_children(lua: &mlua::Lua, props: &Table) -> mlua::Result<Vec<super::node::NodeId>> {
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
