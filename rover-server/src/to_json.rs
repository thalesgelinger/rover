use mlua::{Table, Value};

const MAX_DEPTH: usize = 64;

pub trait ToJson {
    fn to_json(&self, buf: &mut Vec<u8>) -> mlua::Result<()>;

    fn to_json_string(&self) -> mlua::Result<String> {
        let mut buf = Vec::with_capacity(256);
        self.to_json(&mut buf)?;
        Ok(unsafe { String::from_utf8_unchecked(buf) })
    }
}

impl ToJson for Table {
    fn to_json(&self, buf: &mut Vec<u8>) -> mlua::Result<()> {
        serialize_table(self, buf, 0)
    }
}

#[inline]
fn serialize_table(table: &Table, buf: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    if depth >= MAX_DEPTH {
        return Err(mlua::Error::SerializeError(
            "Maximum recursion depth exceeded (64 levels)".to_string(),
        ));
    }

    let first_key: Value = table.raw_get(1)?;

    if !matches!(first_key, Value::Nil) {
        if try_serialize_as_array(table, buf, depth).is_ok() {
            return Ok(());
        }
    }

    serialize_object_direct(table, buf, depth)
}

fn try_serialize_as_array(table: &Table, buf: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    let first_key: Value = table.raw_get(1)?;

    match first_key {
        Value::Nil => return Err(mlua::Error::RuntimeError("Not an array".to_string())),
        _ => {}
    }

    let mut i = 1;

    while !matches!(table.raw_get(i)?, Value::Nil) {
        i += 1;
        if i > 1000 {
            break;
        }
    }

    if i == 1 {
        return Err(mlua::Error::RuntimeError("Not an array".to_string()));
    }

    buf.push(b'[');

    let mut first = true;
    for j in 1..i {
        if !first {
            buf.push(b',');
        }
        first = false;

        let value: Value = table.raw_get(j)?;
        serialize_value(&value, buf, depth + 1)?;
    }

    buf.push(b']');
    Ok(())
}

// Direct serialization - no Vec allocation, single pass through table
fn serialize_object_direct(table: &Table, buf: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    buf.push(b'{');

    let mut first = true;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;

        // Skip internal rover markers (performance optimization - no serialization)
        if let Value::String(ref s) = key {
            if let Ok(key_str) = s.to_str() {
                if key_str.starts_with("__rover_") {
                    continue;
                }
            }
        }

        if !first {
            buf.push(b',');
        }
        first = false;

        match key {
            Value::String(s) => {
                serialize_str(s.to_str()?, buf);
            }
            Value::Integer(i) => {
                buf.push(b'"');
                let mut buffer = itoa::Buffer::new();
                let result = buffer.format(i);
                buf.extend_from_slice(result.as_bytes());
                buf.push(b'"');
            }
            Value::Number(n) => {
                buf.push(b'"');
                let mut buffer = ryu::Buffer::new();
                let result = buffer.format(n);
                buf.extend_from_slice(result.as_bytes());
                buf.push(b'"');
            }
            _ => {
                return Err(mlua::Error::SerializeError(format!(
                    "Unsupported key type: {:?}",
                    key.type_name()
                )));
            }
        }

        buf.push(b':');
        serialize_value(&value, buf, depth + 1)?;
    }

    buf.push(b'}');
    Ok(())
}

#[inline]
fn serialize_value(value: &Value, buf: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    match value {
        Value::Nil => buf.extend_from_slice(b"null"),

        Value::Boolean(true) => buf.extend_from_slice(b"true"),
        Value::Boolean(false) => buf.extend_from_slice(b"false"),

        Value::Integer(i) => {
            let mut buffer = itoa::Buffer::new();
            let result = buffer.format(*i);
            buf.extend_from_slice(result.as_bytes());
        }

        Value::Number(n) => {
            if n.is_finite() {
                let mut buffer = ryu::Buffer::new();
                let result = buffer.format(*n);
                buf.extend_from_slice(result.as_bytes());
            } else {
                buf.extend_from_slice(b"null");
            }
        }

        Value::String(s) => {
            serialize_str(s.to_str()?, buf);
        }

        Value::Table(t) => {
            serialize_table(t, buf, depth)?;
        }

        _ => {
            return Err(mlua::Error::SerializeError(format!(
                "Unsupported value type: {}",
                value.type_name()
            )));
        }
    }

    Ok(())
}

#[inline]
fn serialize_str<S: AsRef<str>>(s: S, buf: &mut Vec<u8>) {
    let s = s.as_ref();
    buf.push(b'"');

    let bytes = s.as_bytes();
    let mut start = 0;

    for (i, &byte) in bytes.iter().enumerate() {
        let escape = match byte {
            b'"' => b'"',
            b'\\' => b'\\',
            b'\n' => b'n',
            b'\r' => b'r',
            b'\t' => b't',
            b'\x08' => b'b',
            b'\x0C' => b'f',
            _ => {
                if byte < 0x20 {
                    if start < i {
                        buf.extend_from_slice(&bytes[start..i]);
                    }
                    buf.extend_from_slice(b"\\u00");
                    buf.push(HEX_DIGITS[(byte >> 4) as usize]);
                    buf.push(HEX_DIGITS[(byte & 0x0F) as usize]);
                    start = i + 1;
                }
                continue;
            }
        };

        if start < i {
            buf.extend_from_slice(&bytes[start..i]);
        }

        buf.push(b'\\');
        buf.push(escape);
        start = i + 1;
    }

    if start < bytes.len() {
        buf.extend_from_slice(&bytes[start..]);
    }

    buf.push(b'"');
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    #[test]
    fn test_simple_object() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("name", "test").unwrap();
        table.set("value", 42).unwrap();

        let json = table.to_json_string().unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"value\":42"));
    }

    #[test]
    fn test_array() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set(1, "a").unwrap();
        table.set(2, "b").unwrap();
        table.set(3, "c").unwrap();

        let json = table.to_json_string().unwrap();
        assert_eq!(json, "[\"a\",\"b\",\"c\"]");
    }

    #[test]
    fn test_nested() {
        let lua = Lua::new();
        let inner = lua.create_table().unwrap();
        inner.set("x", 10).unwrap();

        let outer = lua.create_table().unwrap();
        outer.set("inner", inner).unwrap();
        outer.set("y", 20).unwrap();

        let json = outer.to_json_string().unwrap();
        assert!(json.contains("\"inner\":{"));
        assert!(json.contains("\"x\":10"));
        assert!(json.contains("\"y\":20"));
    }

    #[test]
    fn test_escape_strings() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("text", "hello\nworld\"test\"").unwrap();

        let json = table.to_json_string().unwrap();
        assert!(json.contains("\\n"));
        assert!(json.contains("\\\""));
    }

    #[test]
    fn test_boolean_and_numbers() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("flag", true).unwrap();
        table.set("count", 0).unwrap();
        table.set("ratio", 3.14).unwrap();

        let json = table.to_json_string().unwrap();
        assert!(json.contains("\"flag\":true"));
        assert!(json.contains("\"count\":0"));
        assert!(json.contains("\"ratio\":3.14"));
    }

    #[test]
    fn test_array_with_gap() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set(1, "first").unwrap();
        table.set(2, Value::Nil).unwrap();
        table.set(3, "third").unwrap();

        let json = table.to_json_string().unwrap();
        assert_eq!(json, "[\"first\"]");
    }
}
