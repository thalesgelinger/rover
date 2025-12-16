use anyhow::{Result, anyhow};
use mlua::{Function, Lua, Table, Value};
use rover_server::{HttpMethod, ServerRoute};
use std::fmt;

use crate::{app_type::AppType, auto_table::AutoTable};

pub trait AppServer {
    fn create_server(&self) -> Result<Table>;
}

impl AppServer for Lua {
    fn create_server(&self) -> Result<Table> {
        let server = self.create_auto_table()?;
        let _ = server.set("__rover_app_type", Value::Integer(AppType::Server.to_i64()))?;
        Ok(server)
    }
}

pub trait Server {
    fn run_server(&self) -> Result<()>;
    fn get_routes(&self) -> Result<Vec<ServerRoute>>;
}

impl Server for Table {
    fn run_server(&self) -> Result<()> {
        let routes = self.get_routes()?;
        rover_server::run(&routes);
        Ok(())
    }

    fn get_routes(&self) -> Result<Vec<ServerRoute>> {
        fn extract_recursive(
            table: &Table,
            current_path: &str,
            routes: &mut Vec<ServerRoute>,
        ) -> Result<()> {
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair?;

                // Skip internal rover fields
                if let Value::String(ref key_str) = key {
                    if key_str.to_str()?.starts_with("__rover_") {
                        continue;
                    }
                }

                match (key, value) {
                    (Value::String(key_str), Value::Function(func)) => {
                        let key_string = key_str.to_str()?.to_string();
                        let method = HttpMethod::from_str(&key_string).map_err(|_| {
                            anyhow!(
                                "Unknown HTTP method '{}' at path '{}'",
                                key_string,
                                if current_path.is_empty() {
                                    "/"
                                } else {
                                    current_path
                                }
                            )
                        })?;

                        let path = if current_path.is_empty() {
                            "/".to_string()
                        } else {
                            current_path.to_string()
                        };

                        routes.push(ServerRoute::new(method, path, func));
                    }
                    (Value::String(key_str), Value::Table(nested_table)) => {
                        let key_string = key_str.to_str()?.to_string();
                        let new_path = if current_path.is_empty() {
                            format!("/{}", key_string)
                        } else {
                            format!("{}/{}", current_path, key_string)
                        };

                        extract_recursive(&nested_table, &new_path, routes)?;
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
        let mut routes = vec![];
        extract_recursive(self, "", &mut routes)?;
        Ok(routes)
    }
}
