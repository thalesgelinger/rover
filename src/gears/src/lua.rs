use std::{
    collections::HashMap,
    fs,
    io::Write,
    sync::{Arc, Mutex},
};

use mlua::{Function, Lua, Result, String as LuaString, Table, Value};
use uuid::Uuid;

use crate::{
    dev_server::GLOBAL_STREAM,
    ui::{
        ButtonProps, CallbackId, HorizontalAlignement, Params, Size, TextProps, Ui,
        VerticalAlignement, ViewProps,
    },
};

pub struct Rover {
    ui: Arc<dyn Ui>,
    lua: Lua,
    lua_callbacks: Arc<Mutex<HashMap<CallbackId, Function<'static>>>>,
}

impl Rover {
    pub fn new(ui: Arc<dyn Ui>) -> Self {
        let lua = Lua::new();
        let lua_callbacks = Arc::new(Mutex::new(HashMap::new()));
        Rover {
            ui,
            lua,
            lua_callbacks,
        }
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
                let mut params = Params::new(ViewProps::new());

                for pair in tbl.pairs::<Value, Value>() {
                    match pair.expect("Expected to have a pair") {
                        (Value::String(prop), Value::String(value)) => match prop.as_bytes() {
                            b"horizontal" => match value.as_bytes() {
                                b"center" => {
                                    params.props.horizontal = Some(HorizontalAlignement::Center)
                                }
                                b"left" => {
                                    params.props.horizontal = Some(HorizontalAlignement::Left)
                                }
                                b"right" => {
                                    params.props.horizontal = Some(HorizontalAlignement::Right)
                                }
                                _ => panic!("Unexpected property value"),
                            },
                            b"vertical" => match value.as_bytes() {
                                b"center" => {
                                    params.props.vertical = Some(VerticalAlignement::Center)
                                }
                                b"top" => params.props.vertical = Some(VerticalAlignement::Top),
                                b"bottom" => {
                                    params.props.vertical = Some(VerticalAlignement::Bottom)
                                }
                                _ => panic!("Unexpected property value"),
                            },
                            b"color" => {
                                params.props.color = Some(value.to_str().unwrap().to_string())
                            }
                            b"height" => match value.as_bytes() {
                                b"full" => params.props.height = Some(Size::Full),
                                bytes => {
                                    let number_str = std::str::from_utf8(bytes).unwrap();
                                    if let Ok(number) = number_str.parse::<usize>() {
                                        params.props.height = Some(Size::Value(number));
                                    }
                                }
                            },
                            b"width" => match value.as_bytes() {
                                b"full" => params.props.width = Some(Size::Full),
                                bytes => {
                                    let number_str = std::str::from_utf8(bytes).unwrap();
                                    if let Ok(number) = number_str.parse::<usize>() {
                                        params.props.width = Some(Size::Value(number));
                                    }
                                }
                            },
                            _ => panic!("Unexpected property"),
                        },
                        (Value::Integer(_), Value::String(child_id)) => {
                            params.children.push(child_id.to_str().unwrap().to_string())
                        }
                        _ => (),
                    }
                }

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
                let mut params = Params::new(TextProps::new());

                for pair in tbl.pairs::<Value, Value>() {
                    match pair.expect("Expected to have a pair") {
                        (Value::String(ref prop), Value::String(ref value)) => {
                            match prop.as_bytes() {
                                b"color" => {
                                    params.props.color = Some(value.to_str().unwrap().to_string())
                                }
                                _ => panic!("Unexpected property"),
                            }
                        }
                        (Value::Integer(_), Value::String(ref text)) => {
                            params.children.push(text.to_str().unwrap().to_string())
                        }
                        _ => (),
                    }
                }

                let text_id = ui.create_text(params);

                Ok(Value::String(lua.create_string(&text_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("text", text_lua_fn)
    }

    fn setup_button(&self, lua_rover: &Table) -> Result<()> {
        let ui = Arc::clone(&self.ui);
        let lua_callbacks = Arc::clone(&self.lua_callbacks);

        let text_lua_fn = self
            .lua
            .create_function(move |lua, tbl: Table| {
                let mut params = Params::new(ButtonProps::new());

                for pair in tbl.pairs::<Value, Value>() {
                    match pair.expect("Expected to have a pair") {
                        (Value::String(prop), Value::Function(value)) => match prop.as_bytes() {
                            b"onPress" => {
                                let callback_id: CallbackId =
                                    format!("ROVER_LUA_CALLBACK_{}", Uuid::new_v4().to_string());
                                let fun = value.clone();
                                lua_callbacks
                                    .lock()
                                    .unwrap()
                                    .insert(callback_id.clone(), fun);
                                params.props.on_press = Some(callback_id);
                            }
                            _ => panic!("Unexpected property"),
                        },
                        (Value::Integer(_), Value::String(text)) => {
                            params.children.push(text.to_str().unwrap().to_string())
                        }
                        _ => (),
                    }
                }

                let text_id = ui.create_button(params);

                Ok(Value::String(lua.create_string(&text_id)?))
            })
            .expect("Failed to setup internal view function");

        lua_rover.set("button", text_lua_fn)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap};

    use uuid::Uuid;

    use super::*;

    use crate::ui::{ButtonProps, Id, Params, TextProps, Ui, ViewProps};

    struct Mock {
        components: RefCell<HashMap<String, MockComponent>>,
    }

    #[derive(Debug)]
    pub enum MockComponent {
        View(View),
        Text(Text),
        Button(Button),
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

    #[derive(Debug)]
    pub struct Button {
        props: ButtonProps,
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

        fn create_button(&self, params: Params<ButtonProps>) -> Id {
            let id = format!("ROVER_BUTTON_{}", Uuid::new_v4().to_string());
            let button = MockComponent::Button(Button {
                props: params.props,
                children: params.children,
            });

            println!("{:?}", button);

            self.components.borrow_mut().insert(id.clone(), button);
            id
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
