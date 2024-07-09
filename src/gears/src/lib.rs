use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use mlua::Lua;

// This one is used by IOS
#[no_mangle]
pub extern "C" fn gretting(name_ptr: *const c_char) -> *mut c_char {
    let name = unsafe {
        assert!(!name_ptr.is_null());
        CStr::from_ptr(name_ptr)
            .to_str()
            .expect("Invalid UTF-8 in input")
            .to_owned()
    };

    let result = gretting_rs(name);

    CString::new(result)
        .expect("Failed to create CString")
        .into_raw()
}

extern "C" {
    fn iosPrint();
}

fn call_ios() {
    unsafe {
        iosPrint();
    }
}

// This one for android
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

#[no_mangle]
pub extern "C" fn greeting_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        let _ = CString::from_raw(s);
    };
}

fn gretting_rs(name: String) -> String {
    call_ios();
    return lua_exec(name);
}

fn lua_exec(name: String) -> String {
    let lua_script = include_str!("../../rover/init.lua");

    let lua = Lua::new();

    let _ = lua
        .load(lua_script)
        .exec()
        .expect("Error loading lua script");

    let lua_greetings: mlua::Function = lua
        .globals()
        .get("luaGreetings")
        .expect("Error getting luaGreetings function in lua script");

    let result: String = lua_greetings
        .call(name)
        .expect("Error calling lua greeting");

    return result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_run_lua_script() {
        let result = gretting_rs("Rover".into());
        assert_eq!(result, "Hello Rover your answer came from lua, BTW");
    }
}
