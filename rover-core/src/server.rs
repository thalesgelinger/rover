use anyhow::{Result, anyhow};
use mlua::{Lua, Table, Value};
use rover_openapi::generate_spec;
use rover_parser::analyze;
use rover_server::to_json::ToJson;
use rover_server::{Bytes, HttpMethod, Route, RouteTable, RoverResponse, ServerConfig};
use rover_types::ValidationErrors;

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

        let json_call = self.create_function(|_lua, (_self, data): (Table, Value)| match data {
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
                Ok(RoverResponse::json(200, Bytes::from(json), None))
            }
            _ => Err(mlua::Error::RuntimeError(
                "api.json() requires a table or string".to_string(),
            )),
        })?;

        let json_status_fn = self.create_function(
            |_lua, (_self, status_code, data): (Table, u16, Value)| match data {
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
                    Ok(RoverResponse::json(status_code, Bytes::from(json), None))
                }
                _ => Err(mlua::Error::RuntimeError(
                    "api.json.status() requires a table or string".to_string(),
                )),
            },
        )?;
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
    fn get_routes(&self) -> Result<RouteTable>;
}

impl Server for Table {
    fn run_server(&self, lua: &Lua, source: &str) -> Result<()> {
        let routes = self.get_routes()?;
        let config: ServerConfig = self.get("config")?;

        // Generate OpenAPI spec if docs enabled
        let openapi_spec = if config.docs {
            let model = analyze(source);
            Some(generate_spec(&model, "API", "1.0.0"))
        } else {
            None
        };

        rover_server::run(lua.clone(), routes, config, openapi_spec);
        Ok(())
    }

    fn get_routes(&self) -> Result<RouteTable> {
        fn extract_recursive(
            table: &Table,
            current_path: &str,
            param_names: &mut Vec<String>,
            routes: &mut Vec<Route>,
        ) -> Result<()> {
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;

                // Skip internal rover fields and API helpers
                if let Value::String(ref key_str) = key {
                    let key_str_val = key_str.to_str()?;
                    if key_str_val.starts_with("__rover_")
                        || key_str_val == "config"
                        || key_str_val == "json"
                        || key_str_val == "text"
                        || key_str_val == "html"
                        || key_str_val == "redirect"
                        || key_str_val == "error"
                        || key_str_val == "no_content"
                        || key_str_val == "raw"
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

                        let method = HttpMethod::from_str(&key_string).ok_or_else(|| {
                            anyhow!(
                                "Unknown HTTP method '{}' at path '{}'. Valid methods: {}",
                                key_string,
                                path,
                                HttpMethod::valid_methods().join(", ")
                            )
                        })?;

                        let route = Route {
                            method,
                            pattern: Bytes::from(path.to_string()),
                            param_names: param_names.clone(),
                            handler: func,
                            is_static: param_names.is_empty(),
                        };
                        routes.push(route);
                    }
                    (Value::String(key_str), Value::Table(nested_table)) => {
                        let key_string = key_str.to_str()?.to_string();

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

                        extract_recursive(&nested_table, &new_path, param_names, routes)?;

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

        let mut routes = Vec::new();
        let mut param_names = Vec::new();
        extract_recursive(self, "", &mut param_names, &mut routes)?;

        // Sort routes: static routes first (for exact-match priority)
        routes.sort_by(|a, b| match (a.is_static, b.is_static) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        Ok(RouteTable { routes })
    }
}
