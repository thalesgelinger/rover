use super::helpers::get_derived_as_lua;
use crate::signal::DerivedId;
use mlua::{MetaMethod, UserData, UserDataMethods, Value};

/// Lua userdata for a derived signal
#[derive(Clone, Copy)]
pub struct LuaDerived {
    pub(crate) id: DerivedId,
}

impl LuaDerived {
    pub fn new(id: DerivedId) -> Self {
        Self { id }
    }
}

impl UserData for LuaDerived {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // __index: handle .val getter (computes if dirty)
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if key == "val" {
                get_derived_as_lua(lua, this.id)
            } else {
                Ok(Value::Nil)
            }
        });

        // __tostring
        methods.add_meta_method(
            MetaMethod::ToString,
            |lua, this, ()| match get_derived_as_lua(lua, this.id) {
                Ok(value) => Ok(format!("Derived({:?})", value)),
                Err(e) => Ok(format!("Derived(error: {})", e)),
            },
        );

        // Add arithmetic and comparison metamethods
        super::metamethods::add_derived_metamethods(methods);
    }
}
