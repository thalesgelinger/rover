use std::{
    ffi::{c_char, CStr, CString},
    fs,
};

use mlua::Lua;

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

fn gretting_rs(name: String) -> String {
    return lua_exec(name);
}

fn lua_exec(name: String) -> String {
    let file_path = "../rover/init.lua";
    let lua_script = fs::read_to_string(file_path).expect("Unable to read Lua script file");

    let lua = Lua::new();

    let _ = lua
        .load(&lua_script)
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
