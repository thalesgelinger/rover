use std::{cell::RefCell, collections::HashMap, sync::Arc};

use mlua::{Function, Table, Value};
use uuid::Uuid;

use crate::ui::{
    ButtonProps, CallbackId, HorizontalAlignement, Params, Size, TextProps, VerticalAlignement,
    ViewProps,
};

pub struct LuaParser<'a> {
    lua_callbacks: RefCell<HashMap<CallbackId, Function<'a>>>,
}

impl<'a> LuaParser<'a> {
    pub fn new() -> Self {
        LuaParser {
            lua_callbacks: RefCell::new(HashMap::new()),
        }
    }
}
