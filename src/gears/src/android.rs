use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

use crate::lua::gretting_rs;

#[no_mangle]
pub extern "system" fn Java_com_example_android_Gears_greeting(
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
