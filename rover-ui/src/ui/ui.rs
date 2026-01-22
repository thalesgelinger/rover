use mlua::{String, Table, UserData, Value};

use crate::lua::{derived::LuaDerived, signal::LuaSignal};

pub struct UiTree {}

impl UserData for UiTree {}

enum UiNode {
    Text,
}

trait Renderer {
    fn add_node(ui_node: UiNode);
}

pub struct LuaUi {
    renderer: Box<dyn Renderer>,
}

impl LuaUi {
    pub fn new(renderer: Box<dyn Renderer>) -> Self {
        LuaUi { renderer }
    }
}

impl UserData for LuaUi {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("text", |lua, this, props: Table| {
            let value = props.get::<Value>(1)?;
            match value {
                Value::UserData(ref ud) => {
                    if let Ok(signal) = ud.borrow::<LuaSignal>() {
                    } else if let Ok(derived) = ud.borrow::<LuaDerived>() {
                    }
                }
                text => this.add_node(),
            }
            Ok(())
        });
    }
}
