use std::ffi::{CStr, c_char};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_init() -> *mut mlua::Lua {
    let lua = mlua::Lua::new();
    Box::into_raw(Box::new(lua))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_load_lua(lua: *mut mlua::Lua, source: *const c_char) -> i32 {
    if lua.is_null() || source.is_null() {
        return 1;
    }

    let lua = unsafe { &mut *lua };
    let source = unsafe { CStr::from_ptr(source) };
    let script = source.to_string_lossy();

    match lua.load(script.as_ref()).exec() {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rover_web_tick(_lua: *mut mlua::Lua) -> i32 {
    0
}
