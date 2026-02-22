use anyhow::{anyhow, Result};
use mlua::{Lua, Table, Value};
use rover_openapi::generate_spec;
use rover_parser::analyze;
use rover_server::to_json::ToJson;
use rover_server::{
    Bytes, HttpMethod, MiddlewareChain, Route, RouteTable, RoverResponse, ServerConfig, WsRoute,
};
use rover_types::ValidationErrors;
use rover_ui::scheduler::SharedScheduler;
use rover_ui::SharedSignalRuntime;
use std::collections::HashMap;

use crate::html::{get_rover_html, render_template_with_components};
use crate::{app_type::AppType, auto_table::AutoTable};

pub trait AppServer {
    fn create_server(&self, config: Table) -> Result<Table>;
}

impl AppServer for Lua {
    fn create_server(&self, config: Table) -> Result<Table> {
        let server = self.create_auto_table()?;
        let _ = server.set("__rover_app_type", Value::Integer(AppType::Server.to_i64()))?;
        let _ = server.set("config", config)?;

        let json_helper = self.create_table()?;

        let json_call = self.create_function(|lua, (_self, data): (Table, Value)| {
            // Helper to extract headers from table
            let extract_headers = |table: &Table| -> mlua::Result<
                Option<std::collections::HashMap<String, String>>,
            > {
                let headers_val: Value = table.get("headers")?;
                match headers_val {
                    Value::Nil => Ok(None),
                    Value::Table(headers_table) => {
                        let mut headers = std::collections::HashMap::new();
                        for pair in headers_table.pairs::<String, String>() {
                            let (key, value) = pair?;
                            headers.insert(key, value);
                        }
                        Ok(Some(headers))
                    }
                    _ => Err(mlua::Error::RuntimeError(
                        "headers must be a table".to_string(),
                    )),
                }
            };

            match data {
                Value::String(s) => {
                    let json_str = s.to_str()?;
                    Ok(RoverResponse::json(
                        200,
                        Bytes::copy_from_slice(json_str.as_bytes()),
                        None,
                    ))
                }
                Value::Table(table) => {
                    let json = table.to_json_string().map_err(|e| {
                        mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                    })?;
                    let headers = extract_headers(&table)?;
                    Ok(RoverResponse::json(200, Bytes::from(json), headers))
                }
                _ => Err(mlua::Error::RuntimeError(
                    "api.json() requires a table or string".to_string(),
                )),
            }
        })?;

        let json_status_fn =
            self.create_function(|lua, (_self, status_code, data): (Table, u16, Value)| {
                // Helper to extract headers from table
                let extract_headers = |table: &Table| -> mlua::Result<
                    Option<std::collections::HashMap<String, String>>,
                > {
                    let headers_val: Value = table.get("headers")?;
                    match headers_val {
                        Value::Nil => Ok(None),
                        Value::Table(headers_table) => {
                            let mut headers = std::collections::HashMap::new();
                            for pair in headers_table.pairs::<String, String>() {
                                let (key, value) = pair?;
                                headers.insert(key, value);
                            }
                            Ok(Some(headers))
                        }
                        _ => Err(mlua::Error::RuntimeError(
                            "headers must be a table".to_string(),
                        )),
                    }
                };

                match data {
                    Value::String(s) => {
                        let json_str = s.to_str()?;
                        Ok(RoverResponse::json(
                            status_code,
                            Bytes::copy_from_slice(json_str.as_bytes()),
                            None,
                        ))
                    }
                    Value::Table(table) => {
                        let json = table.to_json_string().map_err(|e| {
                            mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                        })?;
                        let headers = extract_headers(&table)?;
                        Ok(RoverResponse::json(status_code, Bytes::from(json), headers))
                    }
                    _ => Err(mlua::Error::RuntimeError(
                        "api.json.status() requires a table or string".to_string(),
                    )),
                }
            })?;
        json_helper.set("status", json_status_fn)?;

        let meta = self.create_table()?;
        meta.set("__call", json_call)?;
        let _ = json_helper.set_metatable(Some(meta));
        server.set("json", json_helper)?;

        let text_helper = self.create_table()?;

        let text_call = self.create_function(|_lua, (_self, content): (Table, String)| {
            Ok(RoverResponse::text(
                200,
                Bytes::copy_from_slice(content.as_bytes()),
                None,
            ))
        })?;

        let text_status_fn = self.create_function(
            |_lua, (_self, status_code, content): (Table, u16, String)| {
                Ok(RoverResponse::text(
                    status_code,
                    Bytes::copy_from_slice(content.as_bytes()),
                    None,
                ))
            },
        )?;
        text_helper.set("status", text_status_fn)?;

        let text_meta = self.create_table()?;
        text_meta.set("__call", text_call)?;
        let _ = text_helper.set_metatable(Some(text_meta));
        server.set("text", text_helper)?;

        let html_helper = self.create_table()?;

        // Shared function to create HTML response builder
        fn create_html_response_builder(
            lua: &Lua,
            data: Value,
            status: u16,
        ) -> mlua::Result<Table> {
            let builder = lua.create_table()?;
            builder.set("__data", data)?;
            builder.set("__status", status)?;

            let builder_meta = lua.create_table()?;
            builder_meta.set(
                "__call",
                lua.create_function(|lua, (builder, template): (Table, String)| {
                    let data: Value = builder.get("__data")?;
                    let status: u16 = builder.get("__status")?;
                    let html_table = get_rover_html(lua)?;

                    let data_table = match data {
                        Value::Table(t) => t,
                        Value::Nil => lua.create_table()?,
                        _ => {
                            return Err(mlua::Error::RuntimeError(
                                "html() data must be a table or nil".to_string(),
                            ));
                        }
                    };

                    let rendered =
                        render_template_with_components(lua, &template, &data_table, &html_table)?;
                    Ok(RoverResponse::html(
                        status,
                        Bytes::copy_from_slice(rendered.as_bytes()),
                        None,
                    ))
                })?,
            )?;
            let _ = builder.set_metatable(Some(builder_meta));
            Ok(builder)
        }

        let html_call = self.create_function(|lua, (_self, data): (Table, Value)| {
            create_html_response_builder(lua, data, 200)
        })?;

        let html_status_fn =
            self.create_function(|lua, (_self, status_code, data): (Table, u16, Value)| {
                create_html_response_builder(lua, data, status_code)
            })?;
        html_helper.set("status", html_status_fn)?;

        let html_meta = self.create_table()?;
        html_meta.set("__call", html_call)?;
        let _ = html_helper.set_metatable(Some(html_meta));
        server.set("html", html_helper)?;

        let redirect_helper = self.create_table()?;

        let redirect_call = self.create_function(|_lua, (_self, location): (Table, String)| {
            Ok(RoverResponse::redirect(302, location))
        })?;

        let redirect_permanent =
            self.create_function(|_lua, (_self, location): (Table, String)| {
                Ok(RoverResponse::redirect(301, location))
            })?;
        redirect_helper.set("permanent", redirect_permanent)?;

        let redirect_status_fn = self.create_function(
            |_lua, (_self, status_code, location): (Table, u16, String)| {
                Ok(RoverResponse::redirect(status_code, location))
            },
        )?;
        redirect_helper.set("status", redirect_status_fn)?;

        let redirect_meta = self.create_table()?;
        redirect_meta.set("__call", redirect_call)?;
        let _ = redirect_helper.set_metatable(Some(redirect_meta));
        server.set("redirect", redirect_helper)?;

        let error_fn =
            self.create_function(|lua, (_self, (status, message)): (Table, (u16, Value))| {
                // Try ValidationErrors userdata (when passed directly without pcall stringification)
                if let Value::UserData(ref ud) = message {
                    if let Ok(verr) = ud.borrow::<ValidationErrors>() {
                        return Ok(RoverResponse::json(
                            status,
                            Bytes::from(verr.to_json_string()),
                            None,
                        ));
                    }
                }

                // Convert to string
                let message_str = match message {
                    Value::String(s) => s.to_str()?.to_string(),
                    Value::UserData(ud) => {
                        let tostring: mlua::Function = lua.globals().get("tostring")?;
                        tostring.call(Value::UserData(ud))?
                    }
                    other => {
                        let tostring: mlua::Function = lua.globals().get("tostring")?;
                        tostring.call(other)?
                    }
                };

                let mut message_str = message_str
                    .trim_start_matches("runtime error: ")
                    .to_string();
                if let Some(stack_pos) = message_str.find("\nstack traceback:") {
                    message_str = message_str[..stack_pos].to_string();
                }

                // Check if this is a stringified ValidationErrors (from pcall)
                if message_str.contains("Validation failed for request body:") {
                    // Parse the formatted string back to structured JSON
                    use rover_types::ValidationError;
                    let mut errors = Vec::new();
                    let lines: Vec<&str> = message_str.lines().collect();
                    let mut i = 0;

                    while i < lines.len() {
                        let line = lines[i];
                        if let Some(start) = line.find("Field '") {
                            if let Some(end) = line[start + 7..].find('\'') {
                                let field = &line[start + 7..start + 7 + end];
                                let mut error_msg = String::new();
                                let mut error_type = String::new();

                                if i + 1 < lines.len() {
                                    let next_line = lines[i + 1].trim();
                                    if next_line.starts_with("Error:") {
                                        error_msg = next_line
                                            .strip_prefix("Error:")
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                    }
                                }

                                if i + 2 < lines.len() {
                                    let type_line = lines[i + 2].trim();
                                    if type_line.starts_with("Type:") {
                                        error_type = type_line
                                            .strip_prefix("Type:")
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                    }
                                }

                                errors.push(ValidationError::new(field, &error_msg, &error_type));
                                i += 3;
                                continue;
                            }
                        }
                        i += 1;
                    }

                    if !errors.is_empty() {
                        let validation_errors = ValidationErrors::new(errors);
                        return Ok(RoverResponse::json(
                            status,
                            Bytes::from(validation_errors.to_json_string()),
                            None,
                        ));
                    }
                }

                // Generic error
                let table = lua.create_table()?;
                table.set("error", message_str)?;
                let json = table.to_json_string().map_err(|e| {
                    mlua::Error::RuntimeError(format!("JSON serialization failed: {}", e))
                })?;
                Ok(RoverResponse::json(
                    status,
                    Bytes::copy_from_slice(json.as_bytes()),
                    None,
                ))
            })?;
        server.set("error", error_fn)?;

        let no_content_fn = self.create_function(|_lua, _: Table| Ok(RoverResponse::empty(204)))?;
        server.set("no_content", no_content_fn)?;

        let raw_helper = self.create_table()?;

        let raw_call = self.create_function(|_lua, body: mlua::String| {
            let body_str = body.to_str()?;
            Ok(RoverResponse::raw(
                200,
                Bytes::copy_from_slice(body_str.as_bytes()),
                None,
            ))
        })?;

        let raw_status_fn = self.create_function(
            |_lua, (_self, status_code, body): (Table, u16, mlua::String)| {
                let body_str = body.to_str()?;
                Ok(RoverResponse::raw(
                    status_code,
                    Bytes::copy_from_slice(body_str.as_bytes()),
                    None,
                ))
            },
        )?;
        raw_helper.set("status", raw_status_fn)?;

        let raw_meta = self.create_table()?;
        raw_meta.set("__call", raw_call)?;
        let _ = raw_helper.set_metatable(Some(raw_meta));
        server.set("raw", raw_helper)?;

        Ok(server)
    }
}

pub trait Server {
    fn run_server(&self, lua: &Lua, source: &str) -> Result<()>;
    fn get_routes(&self, lua: &Lua) -> Result<RouteTable>;
}

impl Server for Table {
    fn run_server(&self, lua: &Lua, source: &str) -> Result<()> {
        let routes = self.get_routes(lua)?;
        let config: ServerConfig = self.get("config")?;

        // Generate OpenAPI spec if docs enabled
        let openapi_spec = if config.docs {
            let model = analyze(source);
            Some(generate_spec(&model, "API", "1.0.0"))
        } else {
            None
        };

        let runtime = lua.app_data_ref::<SharedSignalRuntime>().map(|r| r.clone());
        let scheduler = lua.app_data_ref::<SharedScheduler>().map(|s| s.clone());

        let server_lua = lua.clone();
        if let Some(runtime) = runtime {
            server_lua.set_app_data(runtime);
        }
        if let Some(scheduler) = scheduler {
            server_lua.set_app_data(scheduler);
        }

        rover_server::run(server_lua, routes, config, openapi_spec);
        Ok(())
    }

    fn get_routes(&self, lua: &Lua) -> Result<RouteTable> {
        // Collect global middlewares first
        let global_middlewares = crate::middleware::collect_global_middlewares(lua, self)?;

        fn extract_recursive(
            lua: &Lua,
            table: &Table,
            current_path: &str,
            param_names: &mut Vec<String>,
            routes: &mut Vec<Route>,
            ws_routes: &mut Vec<WsRoute>,
            global_middlewares: &crate::middleware::MiddlewareChain,
        ) -> Result<()> {
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;

                // Skip internal rover fields and API helpers at root level
                // Note: "before" and "after" are only skipped at root level (current_path == "")
                // At nested levels, they're route-specific middlewares and should be processed
                if let Value::String(ref key_str) = key {
                    let key_str_val = key_str.to_str()?;
                    let is_root = current_path.is_empty();
                    if key_str_val.starts_with("__rover_")
                        || key_str_val == "config"
                        || key_str_val == "json"
                        || key_str_val == "text"
                        || key_str_val == "html"
                        || key_str_val == "redirect"
                        || key_str_val == "error"
                        || key_str_val == "no_content"
                        || key_str_val == "raw"
                        || (is_root && (key_str_val == "before" || key_str_val == "after"))
                    {
                        continue;
                    }
                }

                match (key, value) {
                    (Value::String(key_str), Value::Function(func)) => {
                        let key_string = key_str.to_str()?.to_string();

                        let path = if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        };

                        // WebSocket endpoint: function api.chat.ws(ws) ... end
                        if key_string == "ws" {
                            let ws_route = extract_ws_endpoint(lua, &func, path, param_names)?;
                            ws_routes.push(ws_route);
                            continue;
                        }

                        let method = HttpMethod::from_str(&key_string).ok_or_else(|| {
                            anyhow!(
                                "Unknown HTTP method '{}' at path '{}'. Valid methods: {}",
                                key_string,
                                path,
                                HttpMethod::valid_methods().join(", ")
                            )
                        })?;

                        // Extract route-specific middlewares and merge with global
                        let route_middlewares = crate::middleware::extract_middlewares(lua, table)?;
                        let merged_middlewares = crate::middleware::merge_middleware_chains(
                            global_middlewares,
                            &route_middlewares,
                        );

                        // Create wrapped handler that executes middleware chain
                        let wrapped_handler = if merged_middlewares.is_empty() {
                            func.clone()
                        } else {
                            create_middleware_wrapper(lua, &func, &merged_middlewares)?
                        };

                        let route = Route {
                            method,
                            pattern: Bytes::from(path.to_string()),
                            param_names: param_names.clone(),
                            handler: wrapped_handler,
                            is_static: param_names.is_empty(),
                            middlewares: MiddlewareChain::default(), // Chain is now in the wrapper
                        };
                        routes.push(route);
                    }
                    (Value::String(key_str), Value::Table(nested_table)) => {
                        let key_string = key_str.to_str()?.to_string();

                        // Skip middleware tables (before/after) - they're extracted separately
                        if key_string == "before" || key_string == "after" {
                            continue;
                        }

                        // Check if this is a parameter segment - convert to matchit format immediately
                        let (segment, param_name) = if key_string.starts_with("p_") {
                            let param = key_string.strip_prefix("p_").unwrap();
                            if param.is_empty() {
                                return Err(anyhow!(
                                    "Empty parameter name at path '{}'",
                                    current_path
                                ));
                            }
                            (format!("{{{}}}", param), Some(param.to_string()))
                        } else {
                            (key_string.clone(), None)
                        };

                        let new_path = if current_path.is_empty() {
                            format!("/{}", segment)
                        } else {
                            format!("{}/{}", current_path, segment)
                        };

                        // Add param name if this is a parameter segment
                        if let Some(param) = param_name {
                            param_names.push(param);
                        }

                        extract_recursive(
                            lua,
                            &nested_table,
                            &new_path,
                            param_names,
                            routes,
                            ws_routes,
                            global_middlewares,
                        )?;

                        // Remove param name after recursion
                        if key_string.starts_with("p_") {
                            param_names.pop();
                        }
                    }
                    (k, v) => {
                        return Err(anyhow!(
                            "Invalid server config at path '{}': expected string key with table/function value, got {:?} = {:?}",
                            if current_path.is_empty() {
                                "/"
                            } else {
                                current_path
                            },
                            k,
                            v
                        ));
                    }
                }
            }
            Ok(())
        }

        /// Create a wrapper function that executes middleware chain before/after the handler
        fn create_middleware_wrapper(
            lua: &Lua,
            handler: &mlua::Function,
            middlewares: &MiddlewareChain,
        ) -> Result<mlua::Function> {
            let before_handlers: Vec<_> = middlewares
                .before
                .iter()
                .map(|mw| lua.registry_value::<mlua::Function>(&mw.handler))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to get before middleware: {}", e))?;

            let after_handlers: Vec<_> = middlewares
                .after
                .iter()
                .map(|mw| lua.registry_value::<mlua::Function>(&mw.handler))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to get after middleware: {}", e))?;

            let handler = handler.clone();

            // Create wrapper function
            let wrapper = lua.create_function(move |lua, ctx: mlua::Value| {
                // Execute before middlewares
                for (i, mw) in before_handlers.iter().enumerate() {
                    let result: mlua::Value = mw.call(ctx.clone())?;

                    // Check if middleware returned a response (short-circuit)
                    // If it's not nil and not the context, treat it as a response
                    if !matches!(result, mlua::Value::Nil) {
                        // If the middleware wants to short-circuit, return its result
                        // We check if it's a RoverResponse by trying to borrow it
                        if let Some(ud) = result.as_userdata() {
                            if ud.borrow::<rover_server::RoverResponse>().is_ok() {
                                return Ok(result);
                            }
                        }
                    }
                }

                // Execute the actual handler
                let result: mlua::Value = handler.call(ctx.clone())?;

                // Execute after middlewares (in reverse order)
                for mw in after_handlers.iter().rev() {
                    let _after_result: mlua::Value = mw.call(ctx.clone())?;
                    // After middlewares typically don't modify the response
                }

                Ok(result)
            })?;

            Ok(wrapper)
        }

        /// Extract a WebSocket endpoint from its setup function.
        ///
        /// Calls the setup function with a fresh ws DSL table, then extracts
        /// the captured join/leave/listen handlers into a WsEndpointConfig.
        fn extract_ws_endpoint(
            lua: &Lua,
            setup_fn: &mlua::Function,
            path: &str,
            param_names: &[String],
        ) -> Result<WsRoute> {
            use ahash::AHashMap;
            use rover_server::ws_lua::create_ws_table;
            use rover_server::ws_manager::WsEndpointConfig;

            // Create the ws DSL table (captures handler assignments via metamethods)
            let ws_table = create_ws_table(lua)
                .map_err(|e| anyhow!("Failed to create WS table for '{}': {}", path, e))?;

            // Execute: function api.x.ws(ws) ... end
            setup_fn
                .call::<()>(ws_table.clone())
                .map_err(|e| anyhow!("WS setup function failed at '{}': {}", path, e))?;

            // Extract join handler (stored as __ws_join by metamethod)
            let join_handler = match ws_table.raw_get::<Value>("__ws_join") {
                Ok(Value::Function(f)) => Some(lua.create_registry_value(f)?),
                _ => None,
            };

            // Extract leave handler (stored as __ws_leave by metamethod)
            let leave_handler = match ws_table.raw_get::<Value>("__ws_leave") {
                Ok(Value::Function(f)) => Some(lua.create_registry_value(f)?),
                _ => None,
            };

            // Extract listen event handlers from ws.listen.__ws_handlers table
            let listen_table: Table = ws_table.raw_get("listen")?;
            let handlers_table: Table = listen_table.raw_get("__ws_handlers")?;

            let mut event_handlers = AHashMap::new();
            for pair in handlers_table.pairs::<String, mlua::Function>() {
                let (event_name, handler_fn) = pair?;
                let key = lua.create_registry_value(handler_fn)?;
                event_handlers.insert(event_name, key);
            }

            // Store the ws table itself in registry (needed at runtime for send context)
            let ws_table_key = lua.create_registry_value(ws_table)?;

            let config = WsEndpointConfig {
                join_handler,
                leave_handler,
                event_handlers,
                ws_table_key,
            };

            Ok(WsRoute {
                pattern: Bytes::from(path.to_string()),
                param_names: param_names.to_vec(),
                is_static: param_names.is_empty(),
                endpoint_config: config,
            })
        }

        let mut routes = Vec::new();
        let mut ws_routes = Vec::new();
        let mut param_names = Vec::new();
        extract_recursive(
            lua,
            self,
            "",
            &mut param_names,
            &mut routes,
            &mut ws_routes,
            &global_middlewares,
        )?;

        // Sort routes: static routes first (for exact-match priority)
        routes.sort_by(|a, b| match (a.is_static, b.is_static) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        Ok(RouteTable { routes, ws_routes })
    }
}
