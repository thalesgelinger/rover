use mlua::{Table, Value};

pub trait Inpesct {
    fn inspect(&self);
}

impl Inpesct for Table {
    fn inspect(&self) {
        print_table(self, 0);
    }
}

fn print_table(table: &Table, indent: usize) {
    let prefix = "  ".repeat(indent);

    for pair in table.pairs::<Value, Value>() {
        if let Ok((key, value)) = pair {
            let key_str = format_value(&key);

            match value {
                Value::Table(nested) => {
                    println!("{}{} = {{", prefix, key_str);
                    print_table(&nested, indent + 1);
                    println!("{}}}", prefix);
                }
                _ => {
                    println!("{}{} = {}", prefix, key_str, format_value(&value));
                }
            }
        }
    }
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if let Ok(str_val) = s.to_str() {
                format!("\"{}\"", str_val)
            } else {
                "\"\"".to_string()
            }
        }
        Value::Table(_) => "table".to_string(),
        Value::Function(_) => "function".to_string(),
        Value::Thread(_) => "thread".to_string(),
        Value::UserData(_) => "userdata".to_string(),
        Value::LightUserData(_) => "lightuserdata".to_string(),
        Value::Error(e) => format!("error: {}", e),
        _ => "unknown".to_string(),
    }
}
