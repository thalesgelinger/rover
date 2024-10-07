use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, CStr, CString},
    sync::{mpsc, Arc},
    thread,
};

use objc2::{
    class, msg_send,
    runtime::{AnyClass, NSObject},
};
use uuid::Uuid;

use crate::{
    dev_server::{DevServer, ServerMessages},
    lua::Rover,
    ui::{Id, Params, TextProps, Ui, ViewProps},
};

#[no_mangle]
pub extern "C" fn start(view: *mut NSObject, path: *const c_char) {
    let ios = Arc::new(Ios::new(view));
    let rover = Rover::new(ios);
    let path: String = unsafe {
        if !path.is_null() {
            CStr::from_ptr(path).to_string_lossy().into_owned()
        } else {
            String::new()
        }
    };
    println!("Path sent: {}", path);

    match rover.start(path) {
        Ok(_) => println!("Rover started"),
        Err(_) => println!("Rover failed to start"),
    }
}

type Callback = extern "C" fn(*const c_char);

#[no_mangle]
pub extern "C" fn devServer(callback: Callback) {
    println!("devServer server called");
    let (tx, rx) = mpsc::channel();
    let dev_server = DevServer::new("localhost:4242");

    thread::spawn(move || {
        println!("devServer started");
        let _ = dev_server.listen(&tx);
    });

    let mut name = "".to_string();

    for received in rx {
        match received {
            ServerMessages::Project(project_name) => {
                name = project_name.clone();

                unsafe {
                    let file_utils =
                        AnyClass::get("RoverIos.FileUtils").expect("FileUtils not found");

                    let ns_string: *mut NSObject = msg_send![class!(NSString), stringWithUTF8String: project_name.as_ptr() as *const i8];
                    let _: () = msg_send![file_utils, createFolderIfNotExists: ns_string];
                }
            }
            ServerMessages::File(file) => {
                println!("Get updated file");
                let path_project = format!("{}/{}", name, file.path);

                unsafe {
                    let file_utils =
                        AnyClass::get("RoverIos.FileUtils").expect("FileUtils not found");

                    let ns_path: *mut NSObject = msg_send![class!(NSString), stringWithUTF8String: path_project.as_ptr() as *const i8];
                    let ns_content: *mut NSObject = msg_send![class!(NSString), stringWithUTF8String: file.content.as_ptr() as *const i8];
                    let _: () = msg_send![file_utils, writeFile: ns_path content: ns_content];
                }

                let main_path = format!("{}/lib/main.lua", name);
                let c_string = CString::new(main_path).expect("CString::new failed");
                callback(c_string.as_ptr())
            }
            ServerMessages::Ready => {
                let main_path = format!("{}/lib/main.lua", name);
                let c_string = CString::new(main_path).expect("CString::new failed");
                callback(c_string.as_ptr())
            }
        }
    }
}

struct Ios {
    view: *mut NSObject,
    components: RefCell<HashMap<String, IosComponent>>,
}

impl Ios {
    pub fn new(view: *mut NSObject) -> Ios {
        let components = RefCell::new(HashMap::new());

        Ios { view, components }
    }

    fn add_subview(&self, view: *mut NSObject, child: &IosComponent) -> () {
        match child {
            IosComponent::View(subview) => unsafe {
                let _: () = msg_send![view, addSubview: *subview];
            },
            IosComponent::Text(text) => unsafe {
                let _: () = msg_send![view, addSubview: *text];
            },
        };
    }
}

#[derive(Debug)]
enum IosComponent {
    View(*mut NSObject),
    Text(*mut NSObject),
}

impl<'lua> Ui<'lua> for Ios {
    fn attach_main_view(&self, main_id: Id) -> () {
        let components = self.components.borrow();
        let main_view = components.get(&main_id).expect("Missing main view id");

        self.add_subview(self.view, main_view);
    }

    fn create_view(&self, params: Params<ViewProps>) -> Id {
        let id = format!("ROVER_VIEW_{}", Uuid::new_v4().to_string());

        unsafe {
            let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");

            let props = params.props.to_json();

            let ns_string: *mut NSObject =
                msg_send![class!(NSString), stringWithUTF8String: props.as_ptr() as *const i8];

            let view = msg_send![gears_ios, createView: ns_string];
            for child_id in params.children {
                let components = self.components.borrow();
                let child = components
                    .get(&child_id)
                    .expect("Expected component to exist");
                print!("Child {:?}", child);
                self.add_subview(view, child);
            }

            self.components
                .borrow_mut()
                .insert(id.clone(), IosComponent::View(view));
        }

        id
    }

    fn create_text(&self, params: Params<TextProps>) -> Id {
        let id = format!("ROVER_TEXT_{}", Uuid::new_v4().to_string());

        unsafe {
            let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");

            let text = params.children.join("\n");
            let ns_string: *mut NSObject =
                msg_send![class!(NSString), stringWithUTF8String: text.as_ptr() as *const i8];
            let text_view = msg_send![gears_ios, createTextView: ns_string];

            self.components
                .borrow_mut()
                .insert(id.clone(), IosComponent::Text(text_view));
        }

        id
    }

    fn create_button(&self, _params: Params<crate::ui::ButtonProps<'lua>>) -> Id {
        todo!()
    }
}
