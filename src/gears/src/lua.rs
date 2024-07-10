use mlua::Lua;

pub fn gretting_rs(name: String) -> String {
    lua_exec(name)
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

    result
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
