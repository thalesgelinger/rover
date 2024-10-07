use std::{fs, io::Write, sync::Arc};

use mlua::{Function, Lua, Result, String as LuaString, Table, Value};

use crate::{dev_server::GLOBAL_STREAM, ui::Ui, utils};

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
                let _ = stream.write_all(msg.as_bytes());
            } else {
                println!("{}", msg);
            }

            Ok(())
        })?;

        self.lua.globals().set("print", &tcp_lua_print)?;

        self.lua.globals().set("rover", &lua_rover)?;

        self.setup_view(&lua_rover)?;
        self.setup_text(&lua_rover)?;

        let main_view_id = self.exec(&lua_rover, entry_point);

        self.ui.attach_main_view(main_view_id);

        Ok(())
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
                let params = utils::parse_view_props_children(tbl);
                let view_id = ui.create_view(params);

                Ok(Value::String(lua.create_string(&view_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("view", view_lua_fn)
    }

    fn setup_text(&self, lua_rover: &Table) -> Result<()> {
        let ui = Arc::clone(&self.ui);

        let text_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                let params = utils::parse_text_props_children(tbl);
                let text_id = ui.create_text(params);

                Ok(Value::String(lua.create_string(&text_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("text", text_lua_fn)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap};

    use uuid::Uuid;

    use super::*;

    use crate::ui::{Id, Params, TextProps, Ui, ViewProps};

    struct Mock {
        components: RefCell<HashMap<String, MockComponent>>,
    }

    #[derive(Debug)]
    pub enum MockComponent {
        View(View),
        Text(Text),
    }

    #[derive(Debug)]
    pub struct View {
        props: ViewProps,
        children: Vec<String>,
    }

    #[derive(Debug)]
    pub struct Text {
        props: TextProps,
        children: Vec<String>,
    }

    impl Mock {
        pub fn new() -> Self {
            Mock {
                components: RefCell::new(HashMap::new()),
            }
        }
    }

    impl Ui for Mock {
        fn create_view(&self, params: Params<ViewProps>) -> Id {
            let id = format!("ROVER_VIEW_{}", Uuid::new_v4().to_string());
            println!("Props: {:?}", &params.props.to_json());
            let view = MockComponent::View(View {
                props: params.props,
                children: params.children,
            });

            println!("{:?}", view);
            self.components.borrow_mut().insert(id.clone(), view);
            id
        }

        fn create_text(&self, params: Params<TextProps>) -> Id {
            let id = format!("ROVER_TEXT_{}", Uuid::new_v4().to_string());
            let text = MockComponent::Text(Text {
                props: params.props,
                children: params.children,
            });

            println!("{:?}", text);

            self.components.borrow_mut().insert(id.clone(), text);
            id
        }

        fn attach_main_view(&self, main_id: Id) -> () {
            println!("Main View Id: {}", main_id);
        }
    }

    #[test]
    fn should_run_rover() -> Result<()> {
        let ui: Mock = Mock::new();
        let rover = Rover::new(Arc::new(ui));
        rover.start("../../../template/lib/main.lua".into())?;
        Ok(())
    }
}
