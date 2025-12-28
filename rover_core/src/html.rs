use mlua::{Lua, Table, Value};
use crate::template::{parse_template, generate_lua_code};

/// Standard library functions to copy into template environment
const STD_FUNCTIONS: &[&str] = &[
    "tostring", "tonumber", "ipairs", "pairs", "table", "string", "math", "type", "next",
    "select", "unpack", "pcall", "error", "rawget", "rawset", "setmetatable", "getmetatable",
];

/// Render a template with data and component functions available in the environment
pub fn render_template_with_components(
    lua: &Lua,
    template: &str,
    data: &Table,
    html_table: &Table,
) -> mlua::Result<String> {
    let segments = parse_template(template);
    let lua_code = generate_lua_code(&segments);
    let env = lua.create_table()?;
    let globals = lua.globals();

    // Copy standard library functions
    for name in STD_FUNCTIONS {
        if let Ok(val) = globals.get::<Value>(*name) {
            env.set(*name, val)?;
        }
    }

    // Copy data fields into environment
    for pair in data.pairs::<Value, Value>() {
        let (key, value) = pair?;
        env.set(key, value)?;
    }

    // Add rover reference for nested component calls
    if let Ok(rover) = globals.get::<Table>("rover") {
        env.set("rover", rover)?;
    }

    // Add component functions (skip internal __ prefixed fields)
    for pair in html_table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        if let Value::String(ref s) = key {
            if s.to_str().map(|s| s.starts_with("__")).unwrap_or(false) {
                continue;
            }
        }
        env.set(key, value)?;
    }

    lua.load(&lua_code).set_environment(env).eval().map_err(|e| {
        mlua::Error::RuntimeError(format!(
            "Template rendering failed: {}\nGenerated code:\n{}",
            e, lua_code
        ))
    })
}

/// Get rover.html table from globals
pub fn get_rover_html(lua: &Lua) -> mlua::Result<Table> {
    let rover: Table = lua.globals().get("rover")?;
    rover.get("html")
}

/// Create the rover.html module with templating support and component system
pub fn create_html_module(lua: &Lua) -> mlua::Result<Table> {
    let html_module = lua.create_table()?;
    let html_meta = lua.create_table()?;

    // rover.html(data) returns a template builder
    html_meta.set(
        "__call",
        lua.create_function(|lua, (html_table, data): (Table, Value)| {
            create_template_builder(lua, data, html_table)
        })?,
    )?;

    // __index allows reading component functions from html_module
    html_meta.set("__index", html_module.clone())?;

    // __newindex allows adding component functions to html_module
    html_meta.set(
        "__newindex",
        lua.create_function(|_lua, (table, key, value): (Table, Value, Value)| {
            table.raw_set(key, value)?;
            Ok(())
        })?,
    )?;

    let _ = html_module.set_metatable(Some(html_meta));
    Ok(html_module)
}

/// Create a template builder that renders when called with a template string
fn create_template_builder(lua: &Lua, data: Value, html_table: Table) -> mlua::Result<Table> {
    let builder = lua.create_table()?;
    builder.set("__data", data)?;
    builder.set("__html", html_table)?;

    let builder_meta = lua.create_table()?;
    builder_meta.set(
        "__call",
        lua.create_function(|lua, (builder, template): (Table, String)| {
            let data: Value = builder.get("__data")?;
            let html_table: Table = builder.get("__html")?;

            let data_table = match data {
                Value::Table(t) => t,
                Value::Nil => lua.create_table()?,
                _ => return Err(mlua::Error::RuntimeError(
                    "rover.html() data must be a table or nil".to_string(),
                )),
            };

            render_template_with_components(lua, &template, &data_table, &html_table)
        })?,
    )?;

    let _ = builder.set_metatable(Some(builder_meta));
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_with_empty_data() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let html_table = lua.create_table().unwrap();
        let result = render_template_with_components(&lua, "<h1>Hello</h1>", &data, &html_table).unwrap();
        assert_eq!(result, "<h1>Hello</h1>");
    }

    #[test]
    fn test_render_with_variable() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        data.set("name", "World").unwrap();
        let html_table = lua.create_table().unwrap();
        let result = render_template_with_components(&lua, "Hello {{ name }}", &data, &html_table).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_render_with_component() {
        let lua = Lua::new();
        let data = lua.create_table().unwrap();
        let html_table = lua.create_table().unwrap();

        // Add a simple component function
        let greet = lua.create_function(|_, name: String| {
            Ok(format!("Hello, {}!", name))
        }).unwrap();
        html_table.set("greet", greet).unwrap();

        let result = render_template_with_components(
            &lua,
            "{{ greet(\"World\") }}",
            &data,
            &html_table
        ).unwrap();
        assert_eq!(result, "Hello, World!");
    }
}
