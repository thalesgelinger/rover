use anyhow::{Result, anyhow};
use mlua::{Lua, Table, Value};
use rover_server::{Bytes, HttpMethod, Route, RouteTable, ServerConfig};

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

        let json_call = self.create_function(|_lua, (_self, data): (Table, Table)| {
            data.set("__rover_status", 200)?;
            Ok(data)
        })?;

        let status_fn = self.create_function(|lua, (_self, status_code): (Table, u16)| {
            let builder_call =
                lua.create_function(move |_lua, (_builder, data): (Table, Table)| {
                    data.set("__rover_status", status_code)?;
                    Ok(data)
                })?;

            let builder = lua.create_table()?;
            let builder_meta = lua.create_table()?;
            builder_meta.set("__call", builder_call)?;
            let _ = builder.set_metatable(Some(builder_meta));

            Ok(builder)
        })?;

        json_helper.set("status", status_fn)?;

        let json_meta = self.create_table()?;
        json_meta.set("__call", json_call)?;
        let _ = json_helper.set_metatable(Some(json_meta));

        server.set("json", json_helper)?;

        Ok(server)
    }
}

pub trait Server {
    fn run_server(&self, lua: &Lua) -> Result<()>;
    fn get_routes(&self) -> Result<RouteTable>;
}

impl Server for Table {
    fn run_server(&self, lua: &Lua) -> Result<()> {
        let routes = self.get_routes()?;
        let config: ServerConfig = self.get("config")?;
        rover_server::run(lua.clone(), routes, config);
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

                // Skip internal rover fields
                if let Value::String(ref key_str) = key {
                    let key_str_val = key_str.to_str()?;
                    if key_str_val.starts_with("__rover_")
                        || key_str_val == "config"
                        || key_str_val == "json"
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
