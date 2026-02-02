use anyhow::Result;
use mlua::{Lua, Table, Value};
use std::env;

fn main() -> Result<()> {
    let lua = Lua::new();

    // Setup arg table
    let args: Vec<String> = env::args().collect();
    let arg_table = lua.create_table()?;
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i, arg.as_str())?;
    }
    lua.globals().set("arg", arg_table)?;

    // Register core modules
    register_core_modules(&lua)?;

    // Try to load embedded bundle
    if let Some(bundle) = load_embedded_bundle() {
        lua.load(&bundle).set_name("bundle").exec()?;
    } else {
        eprintln!("Error: No embedded bundle found");
        std::process::exit(1);
    }

    Ok(())
}

fn register_core_modules(lua: &Lua) -> Result<()> {
    let rover = lua.create_table()?;

    #[cfg(feature = "server")]
    {
        rover.set(
            "server",
            lua.create_function(|lua, opts: Table| {
                let server = create_server(lua, opts)?;
                Ok(server)
            })?,
        )?;
    }

    #[cfg(feature = "ui")]
    {
        // UI module will be registered here
        register_ui_module(lua, &rover)?;
    }

    #[cfg(feature = "db")]
    {
        // DB module will be registered here
        let db_module = create_db_module(lua)?;
        rover.set("db", db_module)?;
    }

    lua.globals().set("rover", rover)?;
    Ok(())
}

#[cfg(feature = "server")]
fn create_server(lua: &Lua, config: Table) -> Result<Table> {
    use rover_server::{RouteTable, ServerConfig};

    let server = lua.create_table()?;
    server.set("config", config)?;

    // Add server methods
    let json_helper = lua.create_table()?;
    let json_call = lua.create_function(|_lua, (_self, data): (Table, Value)| {
        // Simplified - would use rover_server types
        Ok(())
    })?;
    let meta = lua.create_table()?;
    meta.set("__call", json_call)?;
    json_helper.set_metatable(Some(meta))?;
    server.set("json", json_helper)?;

    Ok(server)
}

#[cfg(feature = "ui")]
fn register_ui_module(lua: &Lua, rover: &Table) -> Result<()> {
    // Simplified UI module registration
    let ui = lua.create_table()?;
    rover.set("ui", ui)?;
    Ok(())
}

#[cfg(feature = "db")]
fn create_db_module(lua: &Lua) -> Result<Table> {
    let db = lua.create_table()?;
    // Simplified DB module
    Ok(db)
}

/// Load embedded bundle from binary trailer
fn load_embedded_bundle() -> Option<String> {
    // Read self binary
    let exe_path = env::current_exe().ok()?;
    let data = std::fs::read(&exe_path).ok()?;

    // Look for trailer: "ROVER\n<offset>\n<length>\n"
    const TRAILER_MAGIC: &[u8] = b"ROVER\n";

    if let Some(pos) = data
        .windows(TRAILER_MAGIC.len())
        .rposition(|w| w == TRAILER_MAGIC)
    {
        let trailer_start = pos;
        let trailer = &data[trailer_start..];

        // Parse offset and length
        let trailer_str = std::str::from_utf8(trailer).ok()?;
        let parts: Vec<&str> = trailer_str.split('\n').collect();

        if parts.len() >= 3 {
            let offset: usize = parts[1].parse().ok()?;
            let length: usize = parts[2].parse().ok()?;

            if offset + length <= data.len() {
                let bundle = &data[offset..offset + length];
                return String::from_utf8(bundle.to_vec()).ok();
            }
        }
    }

    None
}
