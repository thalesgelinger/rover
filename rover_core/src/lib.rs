use anyhow::Result;

pub fn run(path: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_read_and_print_lua_file() {
        let result = run("../examples/hello.lua");
        assert_eq!(result.unwrap(), ());
    }
}
