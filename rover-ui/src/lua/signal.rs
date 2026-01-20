use crate::signal::{SignalId, SignalValue};
use mlua::{MetaMethod, UserData, UserDataMethods, Value};

/// Lua userdata for a signal
#[derive(Clone, Copy)]
pub struct LuaSignal {
    pub(crate) id: SignalId,
}

impl LuaSignal {
    pub fn new(id: SignalId) -> Self {
        Self { id }
    }
}

impl UserData for LuaSignal {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // __index: handle .val getter
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if key == "val" {
                crate::lua::helpers::get_signal_as_lua(lua, this.id)
            } else {
                Ok(Value::Nil)
            }
        });

        // __newindex: handle .val setter
        methods.add_meta_method_mut(
            MetaMethod::NewIndex,
            |lua, this, (key, value): (String, Value)| {
                if key == "val" {
                    let runtime = crate::lua::helpers::get_runtime(lua)?;

                    let signal_value = SignalValue::from_lua(lua, value)?;
                    runtime.set_signal(this.id, signal_value);
                    Ok(())
                } else {
                    Err(mlua::Error::RuntimeError(format!(
                        "Cannot set property '{}'",
                        key
                    )))
                }
            },
        );

        // __tostring
        methods.add_meta_method(MetaMethod::ToString, |lua, this, ()| {
            let value = crate::lua::helpers::get_signal_as_lua(lua, this.id)?;
            Ok(format!("Signal({:?})", value))
        });

        // Add arithmetic and comparison metamethods
        crate::lua::metamethods::add_signal_metamethods(methods);
    }
}
