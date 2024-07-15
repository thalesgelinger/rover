// use objc2::{
//     msg_send,
//     runtime::{AnyClass, NSObject},
// };
//
// use crate::{lua::Rover, ui::Ui};
//
// #[no_mangle]
// pub extern "C" fn start(view: *mut NSObject) {
//     let ios = Ios::new(view);
//     let rover = Rover::new(Box::new(ios));
//     rover.start().expect("Failed running Rover");
// }
//
// struct Ios {
//     view: *mut NSObject,
// }
//
// impl Ios {
//     pub fn new(view: *mut NSObject) -> Ios {
//         Ios { view }
//     }
// }
//
// impl Ui for Ios {
//     fn create_view(&self) {
//         unsafe {
//             let gears_ios = AnyClass::get("RoverIos.Gears").expect("Class Gears not found");
//
//             let _: () = msg_send![gears_ios, createView: self.view];
//         }
//     }
//
//     fn create_text(&self) {
//         todo!()
//     }
// }
