use anyhow::{Result, anyhow};
use mlua::{Lua, ObjectLike, Table, Value};
use rover_openapi::generate_spec;
use rover_parser::analyze;
use rover_server::to_json::ToJson;
use rover_server::{
    Bytes, HttpMethod, MiddlewareChain, Route, RouteTable, RoverResponse, ServerConfig,
    SseResponse, WsRoute,
};
use rover_types::ValidationErrors;
use rover_ui::SharedSignalRuntime;
use rover_ui::scheduler::SharedScheduler;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

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

        // Streaming response API
        // api.stream(status, content_type, chunk_producer) -> StreamingResponse
        // chunk_producer is a function that returns strings (chunks) or nil (end of stream)
        let stream_fn = self.create_function(
            |lua,
             (_self, status_code, content_type, chunk_producer): (
                Table,
                u16,
                String,
                mlua::Function,
            )| {
                use rover_server::StreamingResponse;
                use std::sync::Arc;

                let producer_key = lua.create_registry_value(chunk_producer)?;
                Ok(StreamingResponse::new(
                    status_code,
                    content_type,
                    None,
                    Arc::new(producer_key),
                ))
            },
        )?;
        server.set("stream", stream_fn)?;

        // Stream with headers: api.stream_with_headers(status, content_type, headers, chunk_producer)
        let stream_with_headers_fn = self.create_function(
            |lua,
             (_self, status_code, content_type, headers, chunk_producer): (
                Table,
                u16,
                String,
                Table,
                mlua::Function,
            )| {
                use rover_server::StreamingResponse;
                use std::sync::Arc;

                let mut response_headers = std::collections::HashMap::new();
                for pair in headers.pairs::<String, String>() {
                    let (key, value) = pair?;
                    response_headers.insert(key, value);
                }

                let producer_key = lua.create_registry_value(chunk_producer)?;
                Ok(StreamingResponse::new(
                    status_code,
                    content_type,
                    Some(response_headers),
                    Arc::new(producer_key),
                ))
            },
        )?;
        server.set("stream_with_headers", stream_with_headers_fn)?;

        let sse_helper = self.create_table()?;

        let sse_call = self.create_function(
            |lua, (_self, event_producer, retry_ms): (Table, mlua::Function, Option<u32>)| {
                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    200,
                    None,
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;

        let sse_status_fn = self.create_function(
            |lua,
             (_self, status_code, event_producer, retry_ms): (
                Table,
                u16,
                mlua::Function,
                Option<u32>,
            )| {
                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    status_code,
                    None,
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;
        sse_helper.set("status", sse_status_fn)?;

        let sse_with_headers_fn = self.create_function(
            |lua,
             (_self, status_code, headers, event_producer, retry_ms): (
                Table,
                u16,
                Table,
                mlua::Function,
                Option<u32>,
            )| {
                let mut response_headers = std::collections::HashMap::new();
                for pair in headers.pairs::<String, String>() {
                    let (key, value) = pair?;
                    response_headers.insert(key, value);
                }

                let producer_key = lua.create_registry_value(event_producer)?;
                Ok(SseResponse::new(
                    status_code,
                    Some(response_headers),
                    Arc::new(producer_key),
                    retry_ms,
                ))
            },
        )?;
        sse_helper.set("with_headers", sse_with_headers_fn)?;

        let sse_meta = self.create_table()?;
        sse_meta.set("__call", sse_call)?;
        let _ = sse_helper.set_metatable(Some(sse_meta));
        server.set("sse", sse_helper)?;

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
        fn create_static_mount_handler(
            lua: &Lua,
            mount_dir: PathBuf,
            mount_param_name: String,
        ) -> Result<mlua::Function> {
            let handler = lua.create_function(move |lua, ctx: Value| {
                let (params_table, headers_table): (Table, Table) = match ctx {
                    Value::UserData(ud) => {
                        let params: Table = ud.call_method("params", ())?;
                        let headers: Table = ud.call_method("headers", ())?;
                        (params, headers)
                    }
                    Value::Table(t) => {
                        let params_fn: mlua::Function = t.get("params")?;
                        let headers_fn: mlua::Function = t.get("headers")?;
                        let params: Table = params_fn.call(())?;
                        let headers: Table = headers_fn.call(())?;
                        (params, headers)
                    }
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "static mount handler expected request context, got {:?}",
                            other
                        )));
                    }
                };

                let mounted_path = params_table
                    .get::<Option<String>>(mount_param_name.as_str())?
                    .unwrap_or_default();

                let mut request_headers = HashMap::new();
                for pair in headers_table.pairs::<String, String>() {
                    let (key, value) = pair?;
                    request_headers.insert(key, value);
                }

                let request_headers_ref = if request_headers.is_empty() {
                    None
                } else {
                    Some(&request_headers)
                };

                let response = rover_server::serve_static_file(
                    mount_dir.as_path(),
                    &mounted_path,
                    request_headers_ref,
                    None,
                );

                lua.create_userdata(response).map(Value::UserData)
            })?;

            Ok(handler)
        }

        fn extract_static_mount_route(
            lua: &Lua,
            table: &Table,
            current_path: &str,
            routes: &mut Vec<Route>,
        ) -> Result<()> {
            let mount_config = match table.raw_get::<Value>("__rover_static_mount")? {
                Value::Table(config) => config,
                Value::Nil => return Ok(()),
                _ => {
                    return Err(anyhow!(
                        "Invalid static mount at '{}': expected table config",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
            };

            let mount_dir = match mount_config.get::<Value>("dir")? {
                Value::String(s) => s.to_str()?.trim().to_string(),
                Value::Nil => {
                    return Err(anyhow!(
                        "Missing 'dir' in static mount at '{}'",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
                _ => {
                    return Err(anyhow!(
                        "Invalid 'dir' in static mount at '{}': expected string",
                        if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        }
                    ));
                }
            };

            if mount_dir.is_empty() {
                return Err(anyhow!(
                    "Invalid static mount at '{}': 'dir' cannot be empty",
                    if current_path.is_empty() {
                        "/"
                    } else {
                        current_path
                    }
                ));
            }

            let mount_base = if current_path.is_empty() {
                "/"
            } else {
                current_path
            };
            let mount_param_name = "__rover_mount_path".to_string();
            let pattern = if mount_base == "/" {
                format!("/{{*{}}}", mount_param_name)
            } else {
                format!(
                    "{}/{{*{}}}",
                    mount_base.trim_end_matches('/'),
                    mount_param_name
                )
            };
            let handler = create_static_mount_handler(
                lua,
                PathBuf::from(mount_dir),
                mount_param_name.clone(),
            )?;

            routes.push(Route {
                method: HttpMethod::Get,
                pattern: Bytes::from(pattern),
                param_names: vec![mount_param_name],
                handler,
                is_static: false,
                middlewares: MiddlewareChain::default(),
            });

            Ok(())
        }

        fn extract_recursive(
            lua: &Lua,
            table: &Table,
            current_path: &str,
            param_names: &mut Vec<String>,
            routes: &mut Vec<Route>,
            ws_routes: &mut Vec<WsRoute>,
            inherited_middlewares: &crate::middleware::MiddlewareChain,
        ) -> Result<()> {
            let local_middlewares = crate::middleware::extract_middlewares(lua, table)?;
            let scope_middlewares = crate::middleware::merge_middleware_chains(
                inherited_middlewares,
                &local_middlewares,
            );

            extract_static_mount_route(lua, table, current_path, routes)?;

            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;

                // Skip internal rover fields and API helpers at root level
                // Note: "before", "after", and "on_error" are only skipped at root level
                // At nested levels, they're route-specific and should be processed
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
                        || key_str_val == "stream"
                        || key_str_val == "stream_with_headers"
                        || key_str_val == "sse"
                        || (is_root
                            && (key_str_val == "before"
                                || key_str_val == "after"
                                || key_str_val == "on_error"))
                    {
                        continue;
                    }
                }

                match (key, value) {
                    (Value::String(key_str), Value::Function(func)) => {
                        let key_string = key_str.to_str()?.to_string();

                        if key_string == "static" {
                            continue;
                        }

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

                        // Create wrapped handler that executes middleware chain
                        let wrapped_handler = if scope_middlewares.is_empty() {
                            func.clone()
                        } else {
                            create_middleware_wrapper(lua, &func, &scope_middlewares)?
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
                            &scope_middlewares,
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
        let root_middlewares = MiddlewareChain::default();
        extract_recursive(
            lua,
            self,
            "",
            &mut param_names,
            &mut routes,
            &mut ws_routes,
            &root_middlewares,
        )?;

        // Sort routes: static routes first (for exact-match priority)
        routes.sort_by(|a, b| match (a.is_static, b.is_static) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        // Extract optional error handler (api.on_error)
        let error_handler = if let Ok(Value::Function(handler)) = self.get("on_error") {
            Some(Arc::new(lua.create_registry_value(handler)?))
        } else {
            None
        };

        Ok(RouteTable {
            routes,
            ws_routes,
            error_handler,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AppServer, Server};
    use mlua::{Lua, Table, Value};
    use rover_server::{HttpMethod, RoverResponse, SseResponse};
    use std::fs;
    use tempfile::tempdir;

    fn create_ctx(lua: &Lua, headers: &[(&str, &str)]) -> mlua::Result<Table> {
        let ctx = lua.create_table()?;
        let state = lua.create_table()?;
        let headers_table = lua.create_table()?;

        for (k, v) in headers {
            headers_table.set(*k, *v)?;
        }

        let headers_clone = headers_table.clone();
        ctx.set(
            "headers",
            lua.create_function(move |_lua, _self: Table| Ok(headers_clone.clone()))?,
        )?;

        let state_set = state.clone();
        ctx.set(
            "set",
            lua.create_function(move |_lua, (_self, key, value): (Table, String, Value)| {
                state_set.set(key, value)?;
                Ok(())
            })?,
        )?;

        let state_get = state.clone();
        ctx.set(
            "get",
            lua.create_function(move |_lua, (_self, key): (Table, String)| {
                state_get.get::<Value>(key)
            })?,
        )?;

        Ok(ctx)
    }

    fn parse_response(value: Value) -> (u16, String) {
        let response_ud = value
            .as_userdata()
            .expect("middleware wrapper must return RoverResponse userdata");
        let response = response_ud
            .borrow::<RoverResponse>()
            .expect("response userdata must be RoverResponse");
        let body = std::str::from_utf8(&response.body)
            .expect("response body must be utf-8")
            .to_string();
        (response.status, body)
    }

    #[test]
    fn should_support_single_static_mount() {
        let temp = tempdir().expect("create temp dir");
        let static_dir = temp.path().join("public");
        fs::create_dir_all(&static_dir).expect("create static dir");
        fs::write(static_dir.join("app.js"), "console.log('ok');").expect("write static file");

        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let dir_lua = static_dir
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!(
            r#"
            local api = rover.server {{}}
            api.assets.static {{ dir = "{}" }}
            return api
        "#,
            dir_lua
        );

        let app: Table = lua.load(&script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok()
                        == Some("/assets/{*__rover_mount_path}")
            })
            .expect("static mount route must exist");

        let ctx = lua.create_table().expect("create ctx table");
        let params = lua.create_table().expect("create params");
        params
            .set("__rover_mount_path", "app.js")
            .expect("set path param");
        ctx.set(
            "params",
            lua.create_function(move |_lua, ()| Ok(params.clone()))
                .expect("create params fn"),
        )
        .expect("set params fn");
        ctx.set(
            "headers",
            lua.create_function(|lua, ()| lua.create_table())
                .expect("create headers fn"),
        )
        .expect("set headers fn");

        let value = route
            .handler
            .call::<Value>(Value::Table(ctx))
            .expect("call static mount route");
        let (status, body) = parse_response(value);

        assert_eq!(status, 200);
        assert_eq!(body, "console.log('ok');");
    }

    #[test]
    fn should_support_multiple_static_mounts() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}
            api.assets.static { dir = "public" }
            api.uploads.static { dir = "uploads" }
            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");

        let patterns: Vec<String> = routes
            .routes
            .iter()
            .filter(|r| r.method == HttpMethod::Get)
            .map(|r| std::str::from_utf8(r.pattern.as_ref()).unwrap().to_string())
            .collect();

        assert!(patterns.contains(&"/assets/{*__rover_mount_path}".to_string()));
        assert!(patterns.contains(&"/uploads/{*__rover_mount_path}".to_string()));
    }

    #[test]
    fn should_apply_group_middlewares_to_nested_routes() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            function api.before.global(ctx)
              ctx:set("request_scope", "global")
            end

            function api.admin.before.authn(ctx)
              if not ctx:headers().Authorization then
                return api:error(401, "Unauthorized: missing Authorization header")
              end
              ctx:set("role", "admin")
            end

            function api.admin.users.get(ctx)
              return api.json {
                scope = ctx:get("request_scope"),
                role = ctx:get("role"),
              }
            end

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/admin/users")
            })
            .expect("route /admin/users must exist");

        let missing_auth_ctx = create_ctx(&lua, &[]).expect("create ctx without auth");
        let deny = route
            .handler
            .call::<Value>(missing_auth_ctx)
            .expect("call route without auth");
        let (deny_status, deny_body) = parse_response(deny);
        assert_eq!(deny_status, 401);
        assert!(deny_body.contains("Unauthorized: missing Authorization header"));

        let allowed_ctx =
            create_ctx(&lua, &[("Authorization", "Bearer test")]).expect("create ctx with auth");
        let ok = route
            .handler
            .call::<Value>(allowed_ctx)
            .expect("call route with auth");
        let (ok_status, ok_body) = parse_response(ok);
        assert_eq!(ok_status, 200);
        assert!(ok_body.contains("\"scope\":\"global\""));
        assert!(ok_body.contains("\"role\":\"admin\""));
    }

    #[test]
    fn should_build_sse_response_from_helper() {
        let lua = Lua::new();
        let rover = lua.create_table().expect("create rover table");
        rover
            .set(
                "server",
                lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                    .expect("create server fn"),
            )
            .expect("set rover.server");
        lua.globals().set("rover", rover).expect("set global rover");

        let script = r#"
            local api = rover.server {}

            function api.events.get(ctx)
              return api.sse(function()
                return {
                  event = "tick",
                  data = { ok = true },
                }
              end, 1500)
            end

            return api
        "#;

        let app: Table = lua.load(script).eval().expect("load app");
        let routes = app.get_routes(&lua).expect("get routes");
        let route = routes
            .routes
            .iter()
            .find(|r| {
                r.method == HttpMethod::Get
                    && std::str::from_utf8(r.pattern.as_ref()).ok() == Some("/events")
            })
            .expect("route /events must exist");

        let ctx = create_ctx(&lua, &[]).expect("create ctx");
        let value = route.handler.call::<Value>(ctx).expect("call SSE route");
        let sse = value
            .as_userdata()
            .expect("SSE helper must return userdata")
            .borrow::<SseResponse>()
            .expect("userdata must be SseResponse");

        assert_eq!(sse.status, 200);
        assert_eq!(sse.retry_ms, Some(1500));
    }
}
