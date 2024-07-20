use std::{cell::RefCell, collections::HashMap, sync::Arc};

use objc2::{
    class, msg_send,
    runtime::{AnyClass, NSObject},
};
use uuid::Uuid;

use crate::{
    lua::Rover,
    ui::{Id, Params, TextProps, Ui, ViewProps},
};

#[no_mangle]
pub extern "C" fn start(view: *mut NSObject) {
    let ios = Arc::new(Ios::new(view));
    let rover = Rover::new(ios);
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

impl Ui for Ios {
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
}
