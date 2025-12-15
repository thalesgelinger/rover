use anyhow::{Context, Result};
use mlua::Lua;

pub fn run(path: &str) -> Result<()> {
    let lua = Lua::new();
    let content = std::fs::read_to_string(path)?;

    lua.load(&content)
        .set_name(path)
        .exec()
        .context("Failed to execute Lua script")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_read_and_print_lua_file() {
        let result = run("examples/hello.lua");
        assert_eq!(result.unwrap(), ());
    }
}
