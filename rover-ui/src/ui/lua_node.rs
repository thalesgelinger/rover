use super::node::NodeId;
use mlua::{FromLua, Lua, MetaMethod, UserData, UserDataMethods, Value};

/// Lua userdata wrapper for a UI node
///
/// This is what gets returned from ru.text(), ru.column(), etc.
/// and can be passed to ru.render()
#[derive(Clone, Copy)]
pub struct LuaNode {
    pub(crate) id: NodeId,
}

impl LuaNode {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }
}

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Allow accessing .id field
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            if key == "id" {
                Ok(this.id.0)
            } else {
                Err(mlua::Error::RuntimeError(format!(
                    "LuaNode has no field '{}'",
                    key
                )))
            }
        });

        // Allow tostring
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("LuaNode({:?})", this.id))
        });
    }
}

impl FromLua for LuaNode {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::UserData(ud) => Ok(*ud.borrow::<LuaNode>()?),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaNode".to_string(),
                message: Some("Expected LuaNode userdata".to_string()),
            }),
        }
    }
}
