#![allow(dead_code)]

use std::path::Path;

use anyhow::Result;
use rover_lua::{LuaEngine, Value};

pub struct Runtime {
    lua: LuaEngine,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        let lua = LuaEngine::new()?;
        Ok(Self { lua })
    }

    pub fn load_entry(&mut self, path: &Path) -> Result<()> {
        self.lua.load_app(path)
    }

    pub fn init_state(&self) -> Result<Value<'_>> {
        self.lua.init_state()
    }

    pub fn render<'lua>(&'lua self, state: Value<'lua>) -> Result<Value<'lua>> {
        self.lua.render(state)
    }
}
