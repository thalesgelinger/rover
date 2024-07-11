use jni::{
    objects::{JClass, JString},
    sys::jstring,
    JNIEnv,
};

use crate::lua::gretting_rs;

#[no_mangle]
pub extern "C" fn Java_com_rovernative_roverandroid_Gears_greeting(
    mut env: JNIEnv,
    _class: JClass,
    input: JString,
) -> jstring {
    println!("JNI function called");

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
