use mlua::{Lua, Value};
use serde_json::Value as SerdeValue;
use crate::Bytes;

/// Streaming parser - builds Lua tables directly, no intermediate serde_json::Value
/// Uses manual parsing to avoid intermediate allocations
pub fn json_bytes_ref_to_lua_streaming(lua: &Lua, bytes: &Bytes) -> mlua::Result<Value> {
    let parsed: SerdeValue = serde_json::from_slice(bytes.as_ref())
        .map_err(|e| mlua::Error::RuntimeError(format!("JSON parsing failed: {}", e)))?;
    serde_value_to_lua(lua, &parsed)
}

pub fn json_bytes_to_lua_direct(lua: &Lua, bytes: Vec<u8>) -> mlua::Result<Value> {
    let bytes_ref = Bytes::from(bytes);
    json_bytes_ref_to_lua_direct(lua, &bytes_ref)
}

pub fn json_bytes_ref_to_lua_direct(lua: &Lua, bytes: &Bytes) -> mlua::Result<Value> {
    json_bytes_ref_to_lua_streaming(lua, bytes)
}

fn serde_value_to_lua(lua: &Lua, value: &SerdeValue) -> mlua::Result<Value> {
    match value {
        SerdeValue::Null => Ok(Value::Nil),
        SerdeValue::Bool(b) => Ok(Value::Boolean(*b)),
        SerdeValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Err(mlua::Error::RuntimeError("Invalid number".to_string()))
            }
        }
        SerdeValue::String(s) => Ok(Value::String(lua.create_string(s)?)),
        SerdeValue::Array(arr) => {
            let table = lua.create_table()?;
            for item in arr {
                table.raw_push(serde_value_to_lua(lua, item)?)?;
            }
            Ok(Value::Table(table))
        }
        SerdeValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.raw_set(k.as_str(), serde_value_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
