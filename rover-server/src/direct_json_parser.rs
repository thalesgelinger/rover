use mlua::{Lua, Value};
use serde_json::Value as SerdeValue;
use serde::de::{self, DeserializeSeed, Deserializer, Visitor, SeqAccess, MapAccess};
use std::fmt;
use crate::Bytes;

struct LuaDeserializeSeed<'a> {
    lua: &'a Lua,
}

impl<'de, 'a> DeserializeSeed<'de> for LuaDeserializeSeed<'a> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LuaValueVisitor { lua: self.lua })
    }
}

struct LuaValueVisitor<'a> {
    lua: &'a Lua,
}

impl<'de, 'a> Visitor<'de> for LuaValueVisitor<'a> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(Value::Boolean(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Value::Integer(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
        if v <= i64::MAX as u64 {
            Ok(Value::Integer(v as i64))
        } else {
            Ok(Value::Number(v as f64))
        }
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Value::Number(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match self.lua.create_string(v) {
            Ok(s) => Ok(Value::String(s)),
            Err(_) => Err(de::Error::custom("Lua string creation failed")),
        }
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match self.lua.create_string(v) {
            Ok(s) => Ok(Value::String(s)),
            Err(_) => Err(de::Error::custom("Lua string creation failed")),
        }
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Nil)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let size_hint = seq.size_hint().unwrap_or(0);
        let table = self.lua.create_table_with_capacity(size_hint, 0)
            .map_err(|_| de::Error::custom("Lua table creation failed"))?;

        while let Some(value) = seq.next_element_seed(LuaDeserializeSeed { lua: self.lua })? {
            table.raw_push(value)
                .map_err(|_| de::Error::custom("Lua table push failed"))?;
        }

        Ok(Value::Table(table))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let size_hint = map.size_hint().unwrap_or(0);
        let table = self.lua.create_table_with_capacity(0, size_hint)
            .map_err(|_| de::Error::custom("Lua table creation failed"))?;

        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(LuaDeserializeSeed { lua: self.lua })?;
            table.set(key.as_str(), value)
                .map_err(|_| de::Error::custom("Lua table set failed"))?;
        }

        Ok(Value::Table(table))
    }
}

/// Streaming parser - builds Lua tables directly, no intermediate serde_json::Value
pub fn json_bytes_ref_to_lua_streaming(lua: &Lua, bytes: &Bytes) -> mlua::Result<Value> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes.as_ref());
    let seed = LuaDeserializeSeed { lua };
    seed.deserialize(&mut deserializer)
        .map_err(|e| mlua::Error::RuntimeError(format!("JSON parsing failed: {}", e)))
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
