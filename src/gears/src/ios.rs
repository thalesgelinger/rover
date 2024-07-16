// use std::{any::Any, collections::HashMap, rc::Rc, sync::Arc};
// 
// use objc2::{
//     msg_send,
//     runtime::{AnyClass, NSObject, Object},
// };
// 
// use crate::{
//     lua::Rover,
//     ui::{Component, Ui},
// };
// 
// #[no_mangle]
// pub extern "C" fn start(view: *mut NSObject) {
//     let ios = Ios::new(view);
//     let rover = Rover::new(Arc::new(ios));
//     rover.start().expect("Failed running Rover");
// }
// 
// struct Ios {
//     view: *mut NSObject,
//     components: HashMap<String, Rc<dyn Component>>,
// }
// 
// impl Ios {
//     pub fn new(view: *mut NSObject) -> Ios {
//         let components = HashMap::new();
// 
//         Ios { view, components }
//     }
// }
// 
// enum IosComponent {
//     View(Rc<*mut NSObject>),
//     Text(Rc<NSObject>),
// }
// 
// impl Component for IosComponent {
//     fn get_id(&self) -> String {
//         todo!()
//     }
// 
//     fn set_child(&self, child: &IosComponent) -> () {
//         match self {
//             IosComponent::View(view) => unsafe {
//                 unsafe {
//                     let _: () = msg_send![view, addSubview: child];
//                 }
//             },
//             IosComponent::Text(_) => todo!(),
//         }
//     }
// 
//     fn set_properties(&self, props: crate::ui::Props) -> () {
//         todo!()
//     }
// }
// 
// impl Ui for Ios {
//     fn create_view(&self) -> Box<dyn Component> {
//         let ios_view: *mut NSObject;
// 
//         unsafe {
//             let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");
// 
//             ios_view = msg_send![gears_ios, createView: self.view];
//         }
// 
//         Box::new(IosComponent::View(Rc::new(ios_view)))
//     }
// 
//     fn create_text(&self) {
//         todo!()
//     }
// 
//     fn get_component(&self, id: String) -> &std::rc::Rc<dyn crate::ui::Component> {
//         todo!()
//     }
// 
//     fn set_component(
//         &mut self,
//         id: String,
//         component: std::rc::Rc<dyn crate::ui::Component>,
//     ) -> () {
//         todo!()
//     }
// }
