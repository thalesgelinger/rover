use mlua::{Lua, Table, Value, Function};
use std::sync::Mutex;
use std::collections::HashMap;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use crate::html_diff;


const ALPINE_JS_MINIFIED: &str = include_str!("alpine.bundle.js");
/// Global registry of component definitions
/// Maps component type names to their definition tables
static COMPONENT_REGISTRY: Mutex<Option<HashMap<String, ComponentDefinition>>> = Mutex::new(None);

/// Global registry of component instances (tracks last rendered HTML)
static INSTANCE_REGISTRY: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

#[derive(Clone)]
pub struct ComponentDefinition {
    pub init: Function,
    pub render: Function,
    pub events: HashMap<String, Function>,
}

#[derive(Serialize, Deserialize)]
pub struct ComponentPatch {
    pub state: serde_json::Value,
    pub html: Option<String>,  // Full HTML if first render or major change
    pub patches: Option<Vec<HtmlPatch>>,  // Minimal patches if small changes
}

impl mlua::IntoLua for ComponentPatch {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        let table = lua.create_table()?;
        table.set("state", serde_json::to_string(&self.state).unwrap_or_else(|_| "null".to_string()))?;
        if let Some(html) = self.html {
            table.set("html", html)?;
        }
        if let Some(patches) = self.patches {
            let patches_json = serde_json::to_string(&patches).map_err(|e| {
                mlua::Error::RuntimeError(format!("Failed to serialize patches: {}", e))
            })?;
            table.set("patches", patches_json)?;
        }
        Ok(mlua::Value::Table(table))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HtmlPatch {
    #[serde(rename = "replace")]
    ReplaceText { selector: String, text: String },
    #[serde(rename = "set_attr")]
    SetAttribute { selector: String, attr: String, value: String },
    #[serde(rename = "remove_attr")]
    RemoveAttribute { selector: String, attr: String },
    #[serde(rename = "replace_html")]
    ReplaceInnerHTML { selector: String, html: String },
}

/// Initialize the registries
fn init_registries() {
    let mut comp_registry = COMPONENT_REGISTRY.lock().unwrap();
    if comp_registry.is_none() {
        *comp_registry = Some(HashMap::new());
    }
    drop(comp_registry);

    let mut inst_registry = INSTANCE_REGISTRY.lock().unwrap();
    if inst_registry.is_none() {
        *inst_registry = Some(HashMap::new());
    }
}

/// Register a component definition
pub fn register_component(name: String, definition: ComponentDefinition) {
    init_registries();
    let mut registry = COMPONENT_REGISTRY.lock().unwrap();
    if let Some(ref mut map) = *registry {
        map.insert(name, definition);
    }
}

/// Get a component definition by name
pub fn get_component(name: &str) -> Option<ComponentDefinition> {
    init_registries();
    let registry = COMPONENT_REGISTRY.lock().unwrap();
    registry.as_ref().and_then(|map| map.get(name).cloned())
}

/// Store rendered HTML for an instance
fn store_instance_html(instance_id: &str, html: &str) {
    init_registries();
    let mut registry = INSTANCE_REGISTRY.lock().unwrap();
    if let Some(ref mut map) = *registry {
        map.insert(instance_id.to_string(), html.to_string());
    }
}

/// Get last rendered HTML for an instance
fn get_instance_html(instance_id: &str) -> Option<String> {
    init_registries();
    let registry = INSTANCE_REGISTRY.lock().unwrap();
    registry.as_ref().and_then(|map| map.get(instance_id).cloned())
}

/// Create the rover.component() function
pub fn create_component_module(lua: &Lua) -> mlua::Result<Table> {
    let component_builder = lua.create_table()?;
    let meta = lua.create_table()?;

    // rover.component() returns a component builder table
    meta.set(
        "__call",
        lua.create_function(|lua, (_component_module, opts): (Table, Option<Table>)| {
            create_component_builder(lua, opts)
        })?,
    )?;

    component_builder.set_metatable(Some(meta))?;
    Ok(component_builder)
}

/// Create a component builder table that collects component methods
fn create_component_builder(lua: &Lua, _opts: Option<Table>) -> mlua::Result<Table> {
    let builder = lua.create_table()?;
    let component_id = Uuid::new_v4().to_string();

    // Store the component type name (will be set when first method is defined)
    builder.set("__component_id", component_id.clone())?;
    builder.set("__component_methods", lua.create_table()?)?;

    let builder_meta = lua.create_table()?;

    // When the component is called {{ Counter() }}, render it
    builder_meta.set(
        "__call",
        lua.create_function(|lua, (builder, props): (Table, Option<Table>)| {
            render_component_instance(lua, builder, props)
        })?,
    )?;

    // Allow setting methods on the component
    builder_meta.set(
        "__newindex",
        lua.create_function(|_lua, (builder, key, value): (Table, String, Value)| {
            // Store the method in __component_methods
            let methods: Table = builder.get("__component_methods")?;
            methods.set(key, value)?;
            Ok(())
        })?,
    )?;

    // Allow getting methods from the component (for Lua access)
    builder_meta.set(
        "__index",
        lua.create_function(|_lua, (builder, key): (Table, String)| -> mlua::Result<Value> {
            let methods: Table = builder.get("__component_methods")?;
            methods.get(key)
        })?,
    )?;

    builder.set_metatable(Some(builder_meta))?;
    Ok(builder)
}

/// Render a component instance
fn render_component_instance(lua: &Lua, builder: Table, props: Option<Table>) -> mlua::Result<String> {
    let methods: Table = builder.get("__component_methods")?;

    // Get required methods
    let init: Function = methods.get("init")
        .map_err(|_| mlua::Error::RuntimeError("Component must have an init() method".to_string()))?;
    let render: Function = methods.get("render")
        .map_err(|_| mlua::Error::RuntimeError("Component must have a render() method".to_string()))?;

    // Get props or create empty table
    let component_props = props.unwrap_or_else(|| lua.create_table().unwrap());

    // Collect event methods (everything except init and render)
    let mut events = HashMap::new();
    for pair in methods.pairs::<String, Value>() {
        let (method_name, method_value) = pair?;
        if method_name != "init" && method_name != "render" {
            if let Value::Function(func) = method_value {
                events.insert(method_name, func);
            }
        }
    }

    // Generate unique instance ID
    let instance_id = Uuid::new_v4().to_string();
    let component_type = builder.get::<String>("__component_id")?;

    // Register component definition for this instance
    register_component(instance_id.clone(), ComponentDefinition {
        init: init.clone(),
        render: render.clone(),
        events: events.clone(),
    });

    // Call init(props) to get initial state
    let initial_state: Value = init.call(component_props)?;

    // Call render(state) to get HTML
    let html: String = render.call(initial_state.clone())?;

    // Serialize state to JSON for client-side storage
    let state_json = lua_value_to_json(lua, &initial_state)?;

    // Extract event names for JavaScript
    let event_names: Vec<String> = events.keys().cloned().collect();

    // Process HTML to wire up event handlers
    let processed_html = process_html_events(&html, &instance_id, &event_names)?;

    // Store the rendered HTML for this instance (for future diffing)
    store_instance_html(&instance_id, &processed_html);

    // Generate the component wrapper with state and JavaScript
    let output = format!(
        r#"<div id="rover-{}" data-rover-component="{}" data-rover-state='{}'>
{}
</div>
<script>
window.__roverComponents = window.__roverComponents || {{}};
window.__roverComponents['{}'] = {{
  type: '{}',
  state: {}
}};
</script>"#,
        instance_id,
        instance_id,
        state_json.replace('\'', "\\'"),
        processed_html,
        instance_id,
        component_type,
        state_json
    );

    Ok(output)
}

/// Convert Lua value to JSON string
fn lua_value_to_json(_lua: &Lua, value: &Value) -> mlua::Result<String> {
    use rover_server::to_json::ToJson;

    match value {
        Value::Table(table) => {
            table.to_json_string().map_err(|e| {
                mlua::Error::RuntimeError(format!("Failed to serialize state to JSON: {}", e))
            })
        }
        Value::String(s) => Ok(format!("\"{}\"", s.to_str()?.replace('"', "\\\""))),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Nil => Ok("null".to_string()),
        _ => Err(mlua::Error::RuntimeError(
            "Unsupported state type - must be a table, string, number, boolean, or nil".to_string()
        )),
    }
}

/// Process HTML to wire up event handlers
/// Converts onclick="increase" to onclick="roverEvent(event, 'instance-id', 'increase')"
/// Also handles onclick="remove(123)" to onclick="roverEvent(event, 'instance-id', 'remove', 123)"
fn process_html_events(html: &str, instance_id: &str, event_names: &[String]) -> mlua::Result<String> {
    use regex::Regex;

    let mut result = html.to_string();

    // Process each event name
    for event_name in event_names {
        for event_attr in &["onclick", "onchange", "onsubmit", "oninput", "onkeyup", "onkeydown"] {
            // Pattern 1: Simple event without parameters - onclick="eventName"
            let simple_pattern = format!("{}=\"{}\"", event_attr, event_name);
            let simple_replacement = format!(
                "{}=\"roverEvent(event, '{}', '{}')\"; return false;\"",
                event_attr, instance_id, event_name
            );
            result = result.replace(&simple_pattern, &simple_replacement);

            // Pattern 2: Event with parameters - onclick="eventName(arg1, arg2, ...)"
            // Match: onclick="eventName(...)"
            let pattern_with_args = format!(r#"{}="{}(\([^)]*\))""#, event_attr, event_name);
            if let Ok(re) = Regex::new(&pattern_with_args) {
                // Find all matches and replace them
                let mut replacements = Vec::new();
                for cap in re.captures_iter(&result.clone()) {
                    if let Some(args_match) = cap.get(1) {
                        let args = args_match.as_str(); // e.g., "(123)" or "('red', 456)"
                        let full_match = cap.get(0).unwrap().as_str();

                        // Convert (arg1, arg2) to just the args without outer parens
                        let args_without_parens = args.trim_start_matches('(').trim_end_matches(')');

                        let replacement = if args_without_parens.is_empty() {
                            // Empty args: onclick="eventName()"
                            format!(
                                "{}=\"roverEvent(event, '{}', '{}')\"; return false;\"",
                                event_attr, instance_id, event_name
                            )
                        } else {
                            // Has args: onclick="eventName(123, 'test')"
                            format!(
                                "{}=\"roverEvent(event, '{}', '{}', {})\"; return false;\"",
                                event_attr, instance_id, event_name, args_without_parens
                            )
                        };

                        replacements.push((full_match.to_string(), replacement));
                    }
                }

                // Apply replacements
                for (from, to) in replacements {
                    result = result.replace(&from, &to);
                }
            }
        }
    }

    Ok(result)
}

/// Handle a component event from the client
pub fn handle_component_event(
    _lua: &Lua,
    instance_id: &str,
    event_name: &str,
    current_state: Value,
    event_data: Option<Value>,
) -> mlua::Result<(Value, ComponentPatch)> {
    // Get component definition
    let definition = get_component(instance_id)
        .ok_or_else(|| mlua::Error::RuntimeError(
            format!("Component instance '{}' not found", instance_id)
        ))?;

    // Get event handler
    let event_handler = definition.events.get(event_name)
        .ok_or_else(|| mlua::Error::RuntimeError(
            format!("Event '{}' not found in component", event_name)
        ))?;

    // Call event handler with current state and optional data
    // If data is an array, unpack it as separate arguments
    let new_state: Value = if let Some(data) = event_data {
        if let Value::Table(ref tbl) = data {
            // Check if it's an array-like table (sequential integer keys)
            let len = tbl.raw_len();
            if len > 0 {
                // It's an array - unpack into variadic call
                let mut args = vec![current_state];
                for i in 1..=len {
                    if let Ok(v) = tbl.raw_get::<Value>(i) {
                        args.push(v);
                    }
                }
                event_handler.call(mlua::MultiValue::from_iter(args))?
            } else {
                // Empty table or object-like - pass as single arg
                event_handler.call((current_state, data))?
            }
        } else {
            // Not a table - pass as single arg
            event_handler.call((current_state, data))?
        }
    } else {
        event_handler.call(current_state)?
    };

    // Call render with new state
    let html: String = definition.render.call(new_state.clone())?;

    // Extract event names for JavaScript wiring
    let event_names: Vec<String> = definition.events.keys().cloned().collect();

    // Process HTML to wire up event handlers (same as initial render)
    let processed_html = process_html_events(&html, instance_id, &event_names)?;

    // Get previous HTML for diffing
    let old_html = get_instance_html(instance_id);
    let html_changed = old_html.as_ref().map_or(true, |old| old != &processed_html);

    // Store new HTML for future comparisons
    store_instance_html(instance_id, &processed_html);

    // Generate patches if HTML changed
    let patches = if html_changed {
        if let Some(old) = old_html {
            html_diff::diff_html(&old, &processed_html)
        } else {
            None
        }
    } else {
        Some(Vec::new())
    };

    // Serialize state to JSON
    let state_json = lua_value_to_json(_lua, &new_state)?;
    let state_value: serde_json::Value = serde_json::from_str(&state_json)
        .map_err(|e| mlua::Error::RuntimeError(format!("Failed to parse state JSON: {}", e)))?;

    // Create patch response
    let patch = if let Some(patch_list) = patches {
        // We have patches
        ComponentPatch {
            state: state_value,
            html: Some(processed_html.clone()),
            patches: Some(patch_list),
        }
    } else if html_changed {
        // No patches but HTML changed - send full HTML
        ComponentPatch {
            state: state_value,
            html: Some(processed_html),
            patches: None,
        }
    } else {
        // No change at all
        ComponentPatch {
            state: state_value,
            html: None,
            patches: None,
        }
    };

    Ok((new_state, patch))
}

const ROVER_CLIENT_JS: &str = include_str!("rover.client.js");

/// Generate the global rover event handler JavaScript
pub fn generate_rover_client_script() -> String {
    // Order matters:
    // 1. First load rover client (sets up alpine:init listener and transforms)
    // 2. Then load Alpine.js (will fire alpine:init which we're listening for)
    format!(
        "<script>{}</script>\n<script>{}</script>",
        ROVER_CLIENT_JS,
        ALPINE_JS_MINIFIED
    )
}
