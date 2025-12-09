#![allow(dead_code)]

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use mlua::{Function, Lua};

pub use mlua::Value;

pub struct LuaEngine {
    lua: Lua,
    render_fn: Option<mlua::RegistryKey>,
    init_fn: Option<mlua::RegistryKey>,
    actions: Vec<String>,
}

impl LuaEngine {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        install_rover_api(&lua)?;
        Ok(Self {
            lua,
            render_fn: None,
            init_fn: None,
            actions: Vec::new(),
        })
    }

    pub fn load_app(&mut self, path: &Path) -> Result<()> {
        let src = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let chunk = self.lua.load(&src).set_name(path.to_string_lossy());
        let app_val = chunk
            .call::<_, Value>(())
            .map_err(|e| anyhow!("exec {}: {e}", path.display()))?;
        let app = match app_val {
            Value::Table(t) => t,
            _ => return Err(anyhow!("entry must return app table")),
        };

        let render_fn: Function = app
            .get("render")
            .map_err(|_| anyhow!("app.render missing or not function"))?;
        let render_key = self.lua.create_registry_value(render_fn)?;

        let init_key = match app.get::<_, Option<Function>>("init")? {
            Some(f) => Some(self.lua.create_registry_value(f)?),
            None => None,
        };

        let mut actions = Vec::new();
        for pair in app.pairs::<String, Value>() {
            let (k, v) = pair?;
            if k == "render" || k == "init" {
                continue;
            }
            if matches!(v, Value::Function(_)) {
                actions.push(k);
            }
        }

        self.render_fn = Some(render_key);
        self.init_fn = init_key;
        self.actions = actions;
        Ok(())
    }

    pub fn init_state(&self) -> Result<Value<'_>> {
        match &self.init_fn {
            Some(key) => {
                let f: Function = self.lua.registry_value(key)?;
                Ok(f.call(())?)
            }
            None => Ok(Value::Nil),
        }
    }

    pub fn render<'lua>(&'lua self, state: Value<'lua>) -> Result<Value<'lua>> {
        let render_key = self
            .render_fn
            .as_ref()
            .ok_or_else(|| anyhow!("render not loaded"))?;
        let render_fn: Function = self.lua.registry_value(render_key)?;
        let act = self.create_actions_table()?;
        render_fn.call((state, act)).map_err(Into::into)
    }

    fn create_actions_table(&self) -> mlua::Result<mlua::Table<'_>> {
        let act = self.lua.create_table()?;
        for name in &self.actions {
            let action_name = name.clone();
            let f = self.lua.create_function(move |lua, ()| {
                let noop = lua.create_function(|_, ()| Ok(()))?;
                Ok(noop)
            })?;
            act.set(action_name.as_str(), f)?;
        }
        Ok(act)
    }
}

fn install_rover_api(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let rover = lua.create_table()?;

    let app_fn = lua.create_function(|lua, ()| lua.create_table())?;
    rover.set("app", app_fn)?;

    rover.set("col", identity_primitive(lua, "col")?)?;
    rover.set("row", identity_primitive(lua, "row")?)?;
    rover.set("text", identity_primitive(lua, "text")?)?;
    rover.set("button", identity_primitive(lua, "button")?)?;

    globals.set("rover", rover)?;
    Ok(())
}

fn identity_primitive<'lua>(lua: &'lua Lua, kind: &'static str) -> mlua::Result<mlua::Function<'lua>> {
    lua.create_function(move |_, value: Value| match value {
        Value::Table(t) => {
            t.set("kind", kind)?;
            Ok(t)
        }
        other => Err(mlua::Error::FromLuaConversionError {
            from: other.type_name(),
            to: "table",
            message: Some("expected table".into()),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn loads_and_renders_example_app() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../../examples/main.lua");
        let mut engine = LuaEngine::new().unwrap();
        engine.load_app(&path).unwrap();

        let state = engine.init_state().unwrap();
        let view = engine.render(state).unwrap();

        match view {
            Value::Table(t) => {
                let kind: String = t.get("kind").unwrap();
                assert_eq!(kind, "col");
            }
            _ => panic!("render did not return table"),
        }
    }
}
