use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ScriptValue {
    String(String),
    Function,
}

pub fn load_rover_scripts() -> Option<HashMap<String, ScriptValue>> {
    let config_path = PathBuf::from("rover.lua");

    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    let lua = mlua::Lua::new();
    let config: mlua::Table = lua.load(&content).set_name("rover.lua").eval().ok()?;
    let scripts_table: mlua::Table = config.get("scripts").ok()?;

    let mut scripts = HashMap::new();

    for pair in scripts_table.pairs::<mlua::Value, mlua::Value>() {
        if let Ok((key, value)) = pair {
            if let mlua::Value::String(name) = key {
                let name_str = name.to_str().ok()?.to_string();

                match value {
                    mlua::Value::String(s) => {
                        scripts.insert(name_str, ScriptValue::String(s.to_str().ok()?.to_string()));
                    }
                    mlua::Value::Function(_) => {
                        scripts.insert(name_str, ScriptValue::Function);
                    }
                    _ => {}
                }
            }
        }
    }

    Some(scripts)
}

pub fn run_script_from_config(
    name: &str,
    args: Vec<String>,
    scripts: &HashMap<String, ScriptValue>,
) -> Result<()> {
    let script_value = scripts.get(name).unwrap();

    match script_value {
        ScriptValue::String(cmd) => run_shell_script(name, cmd, args),
        ScriptValue::Function => run_lua_script(name, args),
    }
}

fn run_shell_script(name: &str, cmd: &str, args: Vec<String>) -> Result<()> {
    println!("🚀 Running script '{}'...", name);

    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(format!("{} \"$@\"", cmd)).arg("--");
    for arg in &args {
        command.arg(arg);
    }

    let status = command
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute command: {}", e))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn run_lua_script(name: &str, args: Vec<String>) -> Result<()> {
    println!("🚀 Running script '{}' (Lua function)...", name);

    let config_path = PathBuf::from("rover.lua");
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("Failed to read rover.lua: {}", e))?;

    let lua = mlua::Lua::new();

    // Initialize signal runtime
    let runtime: rover_ui::SharedSignalRuntime = std::rc::Rc::new(rover_ui::SignalRuntime::new());
    lua.set_app_data(runtime);

    // Initialize UI registry
    let ui_registry = std::rc::Rc::new(std::cell::RefCell::new(rover_ui::ui::UiRegistry::new()));
    lua.set_app_data(ui_registry);

    // Set up arg table
    let arg_table = lua.create_table()?;
    arg_table.set(0, "rover.lua")?;
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i + 1, arg.as_str())?;
    }
    arg_table.set(-1, "rover")?;
    lua.globals().set("arg", arg_table)?;

    // Initialize rover modules
    let rover = lua.create_table()?;

    // Add HTTP client module
    let http_module = rover_core::http::create_http_module(&lua)?;
    rover.set("http", http_module)?;

    // Add rover.html global templating function
    let html_module = rover_core::html::create_html_module(&lua)?;
    rover.set("html", html_module)?;

    // Add rover.db database module
    let db_module = rover_db::create_db_module(&lua)?;
    rover.set("db", db_module)?;

    // Override global io module with async version
    let io_module = rover_core::io::create_io_module(&lua)?;
    lua.globals().set("io", io_module)?;

    // Load debug module from embedded Lua file
    let debug_module: mlua::Table = lua
        .load(include_str!("../../rover-core/src/debug.lua"))
        .set_name("debug.lua")
        .eval()?;
    lua.globals().set("debug", debug_module)?;

    // Load guard from embedded Lua file
    let guard: mlua::Table = lua
        .load(include_str!("../../rover-core/src/guard.lua"))
        .set_name("guard.lua")
        .eval()?;

    // Add __call metamethod for rover.guard(data, schema)
    let guard_meta = lua.create_table()?;
    guard_meta.set("__index", guard.clone())?;
    guard_meta.set(
        "__call",
        lua.create_function(|lua, (data, schema): (mlua::Value, mlua::Value)| {
            use rover_core::guard::{ValidationErrors, validate_table};

            let data_table = match data {
                mlua::Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "First argument must be a table".to_string(),
                    ));
                }
            };

            let schema_table = match schema {
                mlua::Value::Table(ref t) => t.clone(),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "Second argument must be a table".to_string(),
                    ));
                }
            };

            match validate_table(lua, &data_table, &schema_table, "") {
                Ok(validated) => Ok(validated),
                Err(errors) => {
                    let validation_errors = ValidationErrors::new(errors);
                    Err(mlua::Error::ExternalError(std::sync::Arc::new(
                        validation_errors,
                    )))
                }
            }
        })?,
    )?;

    let _ = guard.set_metatable(Some(guard_meta));
    rover.set("guard", guard)?;

    // Register UI module (signals, effects, derive)
    rover_ui::register_ui_module(&lua, &rover)?;

    let _ = lua.globals().set("rover", rover);

    // Make migration global via Lua
    let _ = lua.load("_G.migration = rover.db.migration").eval::<()>()?;

    let config: mlua::Table = lua
        .load(&content)
        .set_name("rover.lua")
        .eval()
        .map_err(|e| anyhow::anyhow!("Failed to parse rover.lua: {}", e))?;

    let scripts_table: mlua::Table = config
        .get("scripts")
        .map_err(|e| anyhow::anyhow!("No scripts table found: {}", e))?;

    let script_fn: mlua::Function = scripts_table
        .get(name)
        .map_err(|e| anyhow::anyhow!("Script '{}' not found: {}", name, e))?;

    script_fn
        .call::<()>(())
        .map_err(|e| anyhow::anyhow!("Script execution failed: {}", e))?;

    Ok(())
}
