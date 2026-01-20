use crate::lua::node::LuaNode;
use crate::node::{Node, SignalOrDerived, TextContent};
use mlua::{Function, Lua, Result, Table, Value};
use smartstring::{LazyCompact, SmartString};

type UiString = SmartString<LazyCompact>;

fn create_text_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: Table| -> Result<LuaNode> {
        let runtime = crate::lua::helpers::get_runtime(lua)?;

        let (content, binding) = match args.get::<Value>(1) {
            Ok(Value::String(s)) => {
                let borrowed = s.to_str()?;
                let text = UiString::from(borrowed.as_ref());
                (TextContent::Static(text), None)
            }
            Ok(Value::UserData(ud)) => {
                if ud.is::<crate::lua::signal::LuaSignal>() {
                    let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                    (
                        TextContent::Signal(signal.id),
                        Some(SignalOrDerived::Signal(signal.id)),
                    )
                } else if ud.is::<crate::lua::derived::LuaDerived>() {
                    let derived = ud.borrow::<crate::lua::derived::LuaDerived>()?;
                    (
                        TextContent::Derived(derived.id),
                        Some(SignalOrDerived::Derived(derived.id)),
                    )
                } else {
                    (TextContent::Static(UiString::from("[unknown]")), None)
                }
            }
            Ok(_) => (TextContent::Static(UiString::from("[unsupported]")), None),
            Err(_) => (TextContent::Static(UiString::from("")), None),
        };

        let node_id = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::text(content))
        };

        if let Some(source) = binding {
            runtime.subscribe_node(source, node_id);
        }

        Ok(LuaNode::new(node_id))
    })
}

fn create_column_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|_lua, args: Table| -> Result<LuaNode> {
        let runtime = crate::lua::helpers::get_runtime(_lua)?;
        let mut arena = runtime.node_arena.borrow_mut();
        let parent_id = arena.create(Node::column());

        let key: Option<String> = args.get("key").ok();
        let key = key.map(|s| UiString::from(s));
        if let Some(k) = key {
            arena.set_key(parent_id, Some(k));
        }

        for value in args.sequence_values::<LuaNode>() {
            let child = value?;
            arena.set_parent(child.id, Some(parent_id));
            if let Some(Node::Column(container)) = arena.get_mut(parent_id) {
                container.children.push(child.id);
            }
        }

        Ok(LuaNode::new(parent_id))
    })
}

fn create_row_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|_lua, args: Table| -> Result<LuaNode> {
        let runtime = crate::lua::helpers::get_runtime(_lua)?;
        let mut arena = runtime.node_arena.borrow_mut();
        let parent_id = arena.create(Node::row());

        let key: Option<String> = args.get("key").ok();
        let key = key.map(|s| UiString::from(s));
        if let Some(k) = key {
            arena.set_key(parent_id, Some(k));
        }

        for value in args.sequence_values::<LuaNode>() {
            let child = value?;
            arena.set_parent(child.id, Some(parent_id));
            if let Some(Node::Row(container)) = arena.get_mut(parent_id) {
                container.children.push(child.id);
            }
        }

        Ok(LuaNode::new(parent_id))
    })
}

fn create_when_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(
        |_lua,
         (condition, true_node, false_node): (Value, LuaNode, Option<LuaNode>)|
         -> Result<LuaNode> {
            let runtime = crate::lua::helpers::get_runtime(_lua)?;

            let condition_signal = match condition {
                Value::UserData(ud) => {
                    let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                    signal.id
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "when() requires a signal as first argument".to_string(),
                    ));
                }
            };

            let mut arena = runtime.node_arena.borrow_mut();
            let node_id = arena.create(Node::conditional(condition_signal));

            if let Some(Node::Conditional(node)) = arena.get_mut(node_id) {
                node.true_branch = Some(true_node.id);
                if let Some(false_node) = false_node {
                    node.false_branch = Some(false_node.id);
                }
            }

            arena.set_parent(true_node.id, Some(node_id));
            if let Some(false_node) = false_node {
                arena.set_parent(false_node.id, Some(node_id));
            }

            runtime.subscribe_node(SignalOrDerived::Signal(condition_signal), node_id);

            Ok(LuaNode::new(node_id))
        },
    )
}

fn create_each_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(
        |lua, (list_signal, render_fn): (Value, Function)| -> Result<LuaNode> {
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            let list_signal_id = match list_signal {
                Value::UserData(ud) => {
                    let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                    signal.id
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "each() requires a signal as first argument".to_string(),
                    ));
                }
            };

            let mut arena = runtime.node_arena.borrow_mut();
            let node_id = arena.create(Node::each(list_signal_id));

            if let Some(Node::Each(each_node)) = arena.get_mut(node_id) {
                each_node.render_fn_key = Some(lua.create_registry_value(render_fn)?);
            }

            Ok(LuaNode::new(node_id))
        },
    )
}

pub fn register_ui_functions(lua: &Lua, ui_table: &Table) -> Result<()> {
    ui_table.set("text", create_text_fn(lua)?)?;
    ui_table.set("column", create_column_fn(lua)?)?;
    ui_table.set("row", create_row_fn(lua)?)?;
    ui_table.set("when", create_when_fn(lua)?)?;
    ui_table.set("each", create_each_fn(lua)?)?;
    Ok(())
}
