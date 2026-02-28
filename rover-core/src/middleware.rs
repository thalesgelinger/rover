use anyhow::Result;
use mlua::{Function, Lua, RegistryKey, Table, Value};
use std::collections::HashMap;
use std::sync::Arc;

// Re-export types from rover_server for use in rover_core
pub use rover_server::{MiddlewareChain, MiddlewareHandler};

/// Extract middlewares from a route table
/// Looks for `before` and `after` keys containing functions or tables of functions
pub fn extract_middlewares(lua: &Lua, table: &Table) -> Result<MiddlewareChain> {
    let mut chain = MiddlewareChain::default();

    // Extract before middlewares
    if let Ok(before) = table.get::<Value>("before") {
        chain.before = extract_middleware_list(lua, &before)?;
    }

    // Extract after middlewares
    if let Ok(after) = table.get::<Value>("after") {
        chain.after = extract_middleware_list(lua, &after)?;
    }

    Ok(chain)
}

/// Extract a list of middlewares from a value (function or table of functions)
fn extract_middleware_list(lua: &Lua, value: &Value) -> Result<Vec<MiddlewareHandler>> {
    let mut middlewares = Vec::new();

    match value {
        // Single function
        Value::Function(func) => {
            let key = Arc::new(lua.create_registry_value(func.clone())?);
            middlewares.push(MiddlewareHandler {
                name: "anonymous".to_string(),
                handler: key,
            });
        }
        // Table of functions (named middlewares)
        Value::Table(table) => {
            for pair in table.pairs::<String, Function>() {
                let (name, func) = pair?;
                let key = Arc::new(lua.create_registry_value(func)?);
                middlewares.push(MiddlewareHandler { name, handler: key });
            }
        }
        _ => {}
    }

    Ok(middlewares)
}

/// Context storage for request-scoped data
pub type ContextData = HashMap<String, Value>;

/// Collect all middlewares from server table
pub fn collect_global_middlewares(lua: &Lua, server: &Table) -> Result<MiddlewareChain> {
    extract_middlewares(lua, server)
}

/// Merge global and route-specific middleware chains
/// Order: global.before -> route.before -> handler -> route.after (rev) -> global.after (rev)
pub fn merge_middleware_chains(
    global: &MiddlewareChain,
    route: &MiddlewareChain,
) -> MiddlewareChain {
    let mut merged = MiddlewareChain::default();

    // Global before middlewares come first
    for mw in &global.before {
        merged.before.push(MiddlewareHandler {
            name: mw.name.clone(),
            handler: Arc::clone(&mw.handler),
        });
    }

    // Then route-specific before middlewares
    for mw in &route.before {
        merged.before.push(MiddlewareHandler {
            name: mw.name.clone(),
            handler: Arc::clone(&mw.handler),
        });
    }

    // Route-specific after middlewares (will be executed in reverse order naturally)
    for mw in &route.after {
        merged.after.push(MiddlewareHandler {
            name: mw.name.clone(),
            handler: Arc::clone(&mw.handler),
        });
    }

    // Global after middlewares
    for mw in &global.after {
        merged.after.push(MiddlewareHandler {
            name: mw.name.clone(),
            handler: Arc::clone(&mw.handler),
        });
    }

    merged
}
