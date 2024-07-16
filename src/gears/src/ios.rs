use std::{borrow::Borrow, cell::RefCell, collections::HashMap, sync::Arc};

use objc2::{
    msg_send,
    runtime::{AnyClass, NSObject},
};

use crate::{
    lua::Rover,
    ui::{Id, Params, TextProps, Ui, ViewProps},
};

#[no_mangle]
pub extern "C" fn start(view: *mut NSObject) {
    let ios = Ios::new(view);
    let rover = Rover::new(Arc::new(ios));
    rover.start().expect("Failed running Rover");
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
                let _: () = msg_send![view, addSubview: subview];
            },
            IosComponent::Text(text) => unsafe {
                let _: () = msg_send![view, addSubview: text];
            },
        }
    }
}

enum IosComponent {
    View(*mut NSObject),
    Text(*mut NSObject),
}

impl Ui for Ios {
    fn attach_main_view(&self, main_id: Id) -> () {
        let components = self.components.borrow();
        let main_view = components.get(&main_id).expect("Missing main view id");

        self.add_subview(self.view, main_view);
    }

    fn create_view(&self, params: Params<ViewProps>) -> Id {
        let id = "VIEW_ID".to_string();

        unsafe {
            let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");

            let view = msg_send![gears_ios, createView: self.view];
            for child_id in params.children {
                let components = self.components.borrow();
                let child = components
                    .get(&child_id)
                    .expect("Expected component to exist");
                self.add_subview(view, child);
            }

            self.components
                .borrow_mut()
                .insert(id.clone(), IosComponent::View(view));
        }

        id
    }

    fn create_text(&self, params: Params<TextProps>) -> Id {
        let id = "TEXT_ID".to_string();

        unsafe {
            let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");

            let text = msg_send![gears_ios, createText: self.view];
            for child_id in params.children {
                let components = self.components.borrow();
                let child = components
                    .get(&child_id)
                    .expect("Expected component to exist");
                self.add_subview(text, child);
            }

            self.components
                .borrow_mut()
                .insert(id.clone(), IosComponent::Text(text));
        }

        id
    }
}
