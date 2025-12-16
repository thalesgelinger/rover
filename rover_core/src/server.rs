use std::collections::HashMap;

use anyhow::{Result, anyhow};
use mlua::{Lua, Table, Value};
use rover_server::Routes;

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
    fn run_server(&self, lua: &Lua) -> Result<()>;
    fn get_routes(&self) -> Result<Routes>;
}

impl Server for Table {
    fn run_server(&self, lua: &Lua) -> Result<()> {
        let routes = self.get_routes()?;
        rover_server::run(lua.clone(), routes);
        Ok(())
    }

    fn get_routes(&self) -> Result<Routes> {
        fn extract_recursive(table: &Table, current_path: &str, routes: &mut Routes) -> Result<()> {
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

                        let path = if current_path.is_empty() {
                            "/"
                        } else {
                            current_path
                        };

                        let valid_methods = vec!["get", "post", "patch", "put", "delete"];

                        if !valid_methods.contains(&key_string.as_str()) {
                            return Err(anyhow!("Unknown HTTP method '{}' at path '{}'", key_string, path));
                        }

                        let path = path.to_string();
                        routes.insert((key_string, path), func);
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
        let mut routes = HashMap::new();
        extract_recursive(self, "", &mut routes)?;
        Ok(routes)
    }
}
