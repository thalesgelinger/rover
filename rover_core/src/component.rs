use mlua::{Lua, Table, Value, Function};
use std::sync::Mutex;
use std::collections::HashMap;
use uuid::Uuid;

/// Global registry of component definitions
/// Maps component type names to their definition tables
static COMPONENT_REGISTRY: Mutex<Option<HashMap<String, ComponentDefinition>>> = Mutex::new(None);

#[derive(Clone)]
pub struct ComponentDefinition {
    pub init: Function,
    pub render: Function,
    pub events: HashMap<String, Function>,
}

/// Initialize the component registry
fn init_registry() {
    let mut registry = COMPONENT_REGISTRY.lock().unwrap();
    if registry.is_none() {
        *registry = Some(HashMap::new());
    }
}

/// Register a component definition
pub fn register_component(name: String, definition: ComponentDefinition) {
    init_registry();
    let mut registry = COMPONENT_REGISTRY.lock().unwrap();
    if let Some(ref mut map) = *registry {
        map.insert(name, definition);
    }
}

/// Get a component definition by name
pub fn get_component(name: &str) -> Option<ComponentDefinition> {
    init_registry();
    let registry = COMPONENT_REGISTRY.lock().unwrap();
    registry.as_ref().and_then(|map| map.get(name).cloned())
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
fn process_html_events(html: &str, instance_id: &str, event_names: &[String]) -> mlua::Result<String> {
    let mut result = html.to_string();

    // Process each event name
    for event_name in event_names {
        // Match onclick="eventName" and similar patterns
        for event_attr in &["onclick", "onchange", "onsubmit", "oninput", "onkeyup", "onkeydown"] {
            let pattern = format!("{}=\"{}\"", event_attr, event_name);
            let replacement = format!(
                "{}=\"roverEvent(event, '{}', '{}')\"; return false;\"",
                event_attr, instance_id, event_name
            );
            result = result.replace(&pattern, &replacement);
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
) -> mlua::Result<(Value, String)> {
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

    // Call event handler with current state
    let new_state: Value = event_handler.call(current_state)?;

    // Call render with new state
    let html: String = definition.render.call(new_state.clone())?;

    // Extract event names for JavaScript wiring
    let event_names: Vec<String> = definition.events.keys().cloned().collect();

    // Process HTML to wire up event handlers (same as initial render)
    let processed_html = process_html_events(&html, instance_id, &event_names)?;

    Ok((new_state, processed_html))
}

/// Generate the global rover event handler JavaScript
pub fn generate_rover_client_script() -> String {
    r#"<script>
window.__roverComponents = window.__roverComponents || {};

async function roverEvent(event, componentId, eventName) {
  event.preventDefault();

  const container = document.getElementById('rover-' + componentId);
  if (!container) {
    console.error('[Rover] Component container not found:', componentId);
    return;
  }

  const component = window.__roverComponents[componentId];
  if (!component) {
    console.error('[Rover] Component not found:', componentId);
    return;
  }

  try {
    const response = await fetch('/__rover/component-event', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        instanceId: componentId,
        eventName: eventName,
        state: component.state
      })
    });

    if (!response.ok) {
      throw new Error('[Rover] Component event failed: ' + response.statusText);
    }

    const result = await response.json();

    // Update state
    component.state = result.state;
    container.dataset.roverState = JSON.stringify(result.state);

    // Update HTML (preserve the container)
    const tempDiv = document.createElement('div');
    tempDiv.innerHTML = result.html;
    container.innerHTML = tempDiv.innerHTML;

  } catch (error) {
    console.error('[Rover] Component event error:', error);
  }
}
</script>"#.to_string()
}
