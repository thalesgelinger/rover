use std::sync::Arc;

use mlua::{Function, Lua, Result, Table, Value};

use crate::ui::Ui;

pub struct Rover {
    ui: Arc<dyn Ui>,
    lua: Lua,
}

impl Rover {
    pub fn new(ui: Arc<dyn Ui>) -> Rover {
        let lua = Lua::new();
        Rover { ui, lua }
    }

    pub fn start(&self) -> Result<()> {
        let lua_rover = self
            .lua
            .create_table()
            .expect("Failed creating rover table");

        self.lua.globals().set("rover", &lua_rover)?;

        self.setup_view(&lua_rover);
        self.setup_text(&lua_rover);

        self.exec(&lua_rover);

        Ok(())
    }

    fn exec(&self, lua_rover: &Table) {
        let script = include_str!("../../rover/init.lua");
        self.lua
            .load(script)
            .exec()
            .expect("Fail running rover script");

        let run_func: Function = lua_rover.get("run").expect("Missing run function");
        run_func
            .call::<(), ()>(())
            .expect("Failed running run function");
    }

    fn setup_view(&self, lua_rover: &Table) -> () {
        let ui = Arc::clone(&self.ui);
        let view_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                println!("rover.view() called from Rust with table:");
                let view_id = ui.create_view();
                for pair in tbl.pairs::<Value, Value>() {
                    match pair.expect("Expected to have a pair") {
                        (Value::String(prop), Value::String(value)) => {
                            println!("Prop, {:?} = {:?}", prop, value);
                            // view.setProperty(prop, value)
                        }

                        (Value::Integer(prop), Value::Table(table)) => {
                            println!("Child, {:?} : {:?}", prop, table);
                            // view.setChild(prop, value)
                        }
                        (a, b) => println!("Not tracked yet, {:?} : {:?}", a, b),
                    }
                }
                Ok(Value::String(lua.create_string(&view_id.get_id())?))
            })
            .expect("Failed to setup internal view function");

        lua_rover
            .set("view", view_lua_fn)
            .expect("Failed setting view function on rover table")
    }

    fn setup_text(&self, lua_rover: &Table) -> () {
        let ui = Arc::clone(&self.ui);

        let text_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                println!("rover.text() called from Rust with table:");
                let text = ui.create_text();
                for pair in tbl.pairs::<Value, Value>() {
                    let (key, value) = pair?;
                    println!("{:?} = {:?}", key, value);
                }

                Ok(Value::String(lua.create_string(&text.get_id())?))
            })
            .expect("Failed to setup internal view function");

        lua_rover
            .set("text", text_lua_fn)
            .expect("Failed setting view function on rover table")
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, rc::Rc};

    use super::*;

    use crate::ui::{Component, Ui};

    struct Mock {
        components: HashMap<String, Rc<dyn Component>>,
    }

    #[derive(Debug, Clone)]
    pub enum MockComponent {
        View(View),
        Text(Text),
    }

    impl Component for MockComponent {
        fn get_id(&self) -> String {
            match &self {
                MockComponent::View(_) => "VIEW_ID".into(),
                MockComponent::Text(_) => "TEXT_ID".into(),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct View {}

    #[derive(Debug, Clone)]
    pub struct Text {}
    impl Text {}

    impl Mock {
        pub fn new() -> Mock {
            Mock {
                components: HashMap::new(),
            }
        }
    }

    impl Ui for Mock {
        fn create_view(&self) -> Box<dyn Component> {
            println!("Called create view");
            Box::new(MockComponent::View(View {}))
        }

        fn create_text(&self) -> Box<dyn Component> {
            println!("Called create text");
            Box::new(MockComponent::Text(Text {}))
        }

        fn get_component(&self, id: String) -> &Rc<dyn Component> {
            self.components.get(&id).expect("Component not found")
        }

        fn set_component(&mut self, id: String, component: Rc<dyn Component>) -> () {
            self.components.insert(id, component);
        }
    }

    #[test]
    fn should_run_rover() -> Result<()> {
        let ui: Mock = Mock::new();
        let rover = Rover::new(Arc::new(ui));
        rover.start()?;
        Ok(())
    }
}
