use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jstring;
use jni::JNIEnv;

use crate::lua::gretting_rs;

#[no_mangle]
pub extern "system" fn Java_com_rovernative_roverandroid_Gears_start<'a>(
    mut env: JNIEnv<'a>,
    _: JClass,
    context: JObject<'a>,
) -> JObject<'a> {
    // Find the class
    let gears_class = match env.find_class("com/rovernative/roverandroid/Gears") {
        Ok(class) => class,
        Err(e) => {
            env.throw_new(
                "java/lang/RuntimeException",
                format!("Failed to load the target class: {:?}", e),
            )
            .expect("Failed to throw exception");
            return JObject::null();
        }
    };

    // Create an instance of the class
    let gears_android = env
        .alloc_object(gears_class)
        .expect("Failed to create an instance of the target class");

    // Call the instance method
    let result = env.call_method(
        gears_android,
        "createView",
        "(Landroid/content/Context;)Landroid/widget/RelativeLayout;",
        &[JValue::Object(&context)],
    );

    result
        .expect("Failed to create View")
        .l()
        .expect("Expected a valid object to be returned")
}

#[no_mangle]
pub extern "system" fn Java_com_rovernative_roverandroid_Gears_greeting(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jstring {
    let input: String = env
        .get_string(&input)
        .expect("Couldn't get Java string!")
        .into();

    let result = gretting_rs(input);

    let output = env
        .new_string(result)
        .expect("Couldn't create Java string!");

    output.into_raw()
}
