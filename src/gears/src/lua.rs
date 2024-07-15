use mlua::{Function, Lua, Result, Table, Value};

use crate::ui::Ui;

pub struct Rover {
    ui: &'static dyn Ui,
    lua: Lua,
}

impl Rover {
    pub fn new(ui: &'static dyn Ui) -> Rover {
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
        let view_lua_fn = self
            .lua
            .create_function(|lua, tbl: Table| {
                println!("rover.view() called from Rust with table:");
                let view_id = self.ui.create_view();
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
                Ok(Value::String(lua.create_string(&view_id.to_string())?))
            })
            .expect("Failed to setup internal view function");

        lua_rover
            .set("view", view_lua_fn)
            .expect("Failed setting view function on rover table")
    }

    fn setup_text(&self, lua_rover: &Table) -> () {
        let text_lua_fn = self
            .lua
            .create_function(|lua, tbl: Table| {
                println!("rover.text() called from Rust with table:");
                let text_id = self.ui.create_text();
                for pair in tbl.pairs::<Value, Value>() {
                    let (key, value) = pair?;
                    println!("{:?} = {:?}", key, value);
                }

                Ok(Value::String(lua.create_string(&text_id.to_string())?))
            })
            .expect("Failed to setup internal view function");

        lua_rover
            .set("text", text_lua_fn)
            .expect("Failed setting view function on rover table")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ui::{Id, Ui};

    struct Mock {}

    impl Ui for Mock {
        fn create_view(&self) -> Id {
            println!("Called create view");
            "CREATE_VIEW".into()
        }

        fn create_text(&self) -> Id {
            println!("Called create text");
            "CREATE_TEXT".into()
        }
    }

    static UI: Mock = Mock {};

    #[test]
    fn should_run_rover() -> Result<()> {
        let rover = Rover::new(&UI);
        rover.start()?;
        Ok(())
    }
}
