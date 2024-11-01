use std::{fs, io::Write, sync::Arc};

use anyhow::Result;
use mlua::{Error as LuaError, Function, Lua, String as LuaString, Table, Value};

use crate::{dev_server::GLOBAL_STREAM, ui::Ui, utils::PropsParser};

pub struct Rover {
    ui: Arc<dyn Ui>,
    lua: Lua,
}

impl Rover {
    pub fn new(ui: Arc<dyn Ui>) -> Rover {
        let lua = Lua::new();
        Rover { ui, lua }
    }

    pub fn start(&self, entry_point: String) -> Result<()> {
        let lua_rover = self.lua.create_table()?;

        let tcp_lua_print = self.lua.create_function(|_, msg: String| {
            let mut global_stream = GLOBAL_STREAM.lock().unwrap();

            if let Some(ref mut stream) = *global_stream {
                let _ = stream.write_all(format!("{}\n", msg).as_bytes());
            } else {
                println!("{}", msg);
            }

            Ok(())
        })?;

        self.lua.globals().set("print", &tcp_lua_print)?;

        self.lua.globals().set("rover", &lua_rover)?;

        self.setup_view(&lua_rover)?;
        self.setup_text(&lua_rover)?;
        self.setup_button(&lua_rover)?;

        let main_view_id = self.exec(&lua_rover, entry_point);

        self.ui.attach_main_view(main_view_id)
    }

    fn exec(&self, lua_rover: &Table, entry_point: String) -> String {
        let script = fs::read_to_string(entry_point).expect("Failed to read entry point");
        self.lua
            .load(script)
            .exec()
            .expect("Fail running rover script");

        let run_func: Function = lua_rover.get("run").expect("Missing run function");
        let main_view_id = run_func
            .call::<(), LuaString>(())
            .expect("Failed running run function");
        main_view_id.to_str().unwrap().to_string()
    }

    fn setup_view(&self, lua_rover: &Table) -> Result<()> {
        let ui = Arc::clone(&self.ui);
        let view_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                let params = tbl.parse_view_props();
                let view_id = ui.create_view(params).map_err(|e| LuaError::external(e))?;

                Ok(Value::String(lua.create_string(&view_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("view", view_lua_fn)?;
        Ok(())
    }

    fn setup_text(&self, lua_rover: &Table) -> Result<()> {
        let ui = Arc::clone(&self.ui);

        let text_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                let params = tbl.parse_text_props();
                let text_id = ui.create_text(params).map_err(|e| LuaError::external(e))?;

                Ok(Value::String(lua.create_string(&text_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("text", text_lua_fn)?;
        Ok(())
    }

    fn setup_button(&self, lua_rover: &Table) -> Result<()> {
        let ui = Arc::clone(&self.ui);

        let button_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                let params = tbl.parse_button_props();
                let text_id = ui
                    .create_button(params)
                    .map_err(|e| LuaError::external(e))?;

                Ok(Value::String(lua.create_string(&text_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("button", button_lua_fn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap};

    use anyhow::Result;
    use uuid::Uuid;

    use super::*;

    use crate::ui::{ButtonProps, Id, Params, TextProps, Ui, ViewProps};

    struct Mock {
        components: RefCell<HashMap<String, MockComponent>>,
    }

    #[derive(Debug, Clone)]
    pub enum MockComponent {
        View(String),
        Text(String),
        Button(String),
    }

    impl Mock {
        pub fn new() -> Self {
            Mock {
                components: RefCell::new(HashMap::new()),
            }
        }

        pub fn show(&self) {
            self.components
                .borrow()
                .clone()
                .into_iter()
                .for_each(|v| print!("{:?}", v))
        }
    }

    impl Ui for Mock {
        fn attach_main_view(&self, main_id: Id) -> Result<()> {
            println!("Main View Id: {}", main_id);
            self.show();
            Ok(())
        }

        fn create_view(&self, params: Params<ViewProps>) -> Result<Id> {
            let id = format!("ROVER_VIEW_{}", Uuid::new_v4().to_string());
            let view = MockComponent::View(format!("{:?}", params));

            println!("View: {:?}", view);
            self.components.borrow_mut().insert(id.clone(), view);
            Ok(id)
        }

        fn create_text(&self, params: Params<TextProps>) -> Result<Id> {
            let id = format!("ROVER_TEXT_{}", Uuid::new_v4().to_string());
            let text = MockComponent::Text(format!("{:?}", params));

            println!("Text: {:?}", text);

            self.components.borrow_mut().insert(id.clone(), text);
            Ok(id)
        }

        fn create_button(&self, params: Params<ButtonProps>) -> Result<Id> {
            let id = format!("ROVER_BUTTON_{}", Uuid::new_v4().to_string());
            let button = MockComponent::Button(format!("{:?}", params));

            println!("Button: {:?}", params);

            let _ = params.props.on_press.unwrap().call::<(), String>(());

            self.components.borrow_mut().insert(id.clone(), button);
            Ok(id)
        }
    }

    #[test]
    fn should_run_rover() -> Result<()> {
        let ui: Mock = Mock::new();
        let rover = Rover::new(Arc::new(ui));
        rover.start("../../template/lib/main.lua".into())?;
        Ok(())
    }
}
