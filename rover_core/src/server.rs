use anyhow::Result;
use mlua::{Lua, Table, Value};

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
}

impl Server for Table {
    fn run_server(&self) -> Result<()> {
        println!("SERVER TABLE: {:?}", self);
        Ok(())
    }
}
