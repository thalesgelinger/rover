// use std::rc::Rc;
// 
// use jni::objects::{JClass, JObject, JValue};
// use jni::JNIEnv;
// 
// use crate::lua::Rover;
// use crate::ui::Ui;
// 
// #[no_mangle]
// pub extern "system" fn Java_com_rovernative_roverandroid_Gears_start<'a>(
//     mut env: JNIEnv<'a>,
//     _: JClass,
//     context: JObject<'a>,
// ) {
//     let android = Android::new(context, env.into());
//     let rover = Rover::new(Box::new(android));
//     rover.start().expect("Failed running Rover");
// }
// 
// struct Android<'a> {
//     context: JObject<'a>,
//     env: Rc<JNIEnv<'a>>,
// }
// 
// impl<'a> Android<'a> {
//     pub fn new(context: JObject<'a>, env: Rc<JNIEnv<'a>>) -> Android<'a> {
//         Android { context, env }
//     }
// }
// 
// impl<'a> Ui for Android<'a> {
//     fn create_view(&self) {
//         // Find the class
//         let gears_class = match self.env.find_class("com/rovernative/roverandroid/Gears") {
//             Ok(class) => class,
//             Err(e) => {
//                 self.env
//                     .throw_new(
//                         "java/lang/RuntimeException",
//                         format!("Failed to load the target class: {:?}", e),
//                     )
//                     .expect("Failed to throw exception");
//                 panic!("{}", e)
//             }
//         };
// 
//         // Create an instance of the class
//         let gears_android = self
//             .env
//             .alloc_object(gears_class)
//             .expect("Failed to create an instance of the target class");
// 
//         // Call the instance method
//         let result = self.env.call_method(
//             gears_android,
//             "createView",
//             "(Landroid/content/Context;)Landroid/widget/RelativeLayout;",
//             &[JValue::Object(&self.context)],
//         );
// 
//         result
//             .expect("Failed to create View")
//             .l()
//             .expect("Expected a valid object to be returned");
//     }
// 
//     fn create_text(&self) {
//         todo!()
//     }
// }
