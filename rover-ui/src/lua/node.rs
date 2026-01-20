use crate::node::NodeId;
use mlua::{FromLua, Lua, UserData, UserDataMethods};

#[derive(Clone, Copy)]
pub struct LuaNode {
    pub id: NodeId,
}

impl LuaNode {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }
}

impl FromLua for LuaNode {
    fn from_lua(value: mlua::Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::UserData(ud) => {
                let node_ref = ud.borrow::<LuaNode>()?;
                Ok(*node_ref)
            }
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaNode".to_string(),
                message: Some("expected LuaNode userdata".to_string()),
            }),
        }
    }
}

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {}
}
