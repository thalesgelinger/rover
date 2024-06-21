use std::fs;

use mlua::Lua;

#[no_mangle]
pub extern "C" fn gretting(name: String) -> String {
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
        let result = gretting("Rover".into());
        assert_eq!(result, "Hello Rover your answer came from lua, BTW");
    }
}
