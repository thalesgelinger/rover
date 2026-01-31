use mlua::{Lua, RegistryKey, Value};
use smartstring::alias::String as SmartString;

use crate::lua::signal::LuaSignal;
use crate::lua::derived::LuaDerived;

/// SignalValue represents the value stored in a signal
#[derive(Debug)]
pub enum SignalValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(SmartString),
    Table(RegistryKey),
}

impl SignalValue {
    /// Compare values for change detection
    /// Tables compare by reference (RegistryKey) only
    pub fn eq_value(&self, other: &Self) -> bool {
        match (self, other) {
            (SignalValue::Nil, SignalValue::Nil) => true,
            (SignalValue::Bool(a), SignalValue::Bool(b)) => a == b,
            (SignalValue::Int(a), SignalValue::Int(b)) => a == b,
            (SignalValue::Float(a), SignalValue::Float(b)) => {
                // Handle NaN properly
                if a.is_nan() && b.is_nan() {
                    true
                } else {
                    a == b
                }
            }
            (SignalValue::String(a), SignalValue::String(b)) => a == b,
            (SignalValue::Table(a), SignalValue::Table(b)) => {
                // Table reference equality
                std::ptr::eq(a, b)
            }
            _ => false,
        }
    }

    /// Convert SignalValue to Lua value
    pub fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        match self {
            SignalValue::Nil => Ok(Value::Nil),
            SignalValue::Bool(b) => Ok(Value::Boolean(*b)),
            SignalValue::Int(i) => Ok(Value::Integer(*i)),
            SignalValue::Float(f) => Ok(Value::Number(*f)),
            SignalValue::String(s) => Ok(Value::String(lua.create_string(s.as_str())?)),
            SignalValue::Table(key) => Ok(Value::Table(lua.registry_value(key)?)),
        }
    }

    /// Convert Lua value to SignalValue
    pub fn from_lua(lua: &Lua, value: Value) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(SignalValue::Nil),
            Value::Boolean(b) => Ok(SignalValue::Bool(b)),
            Value::Integer(i) => Ok(SignalValue::Int(i)),
            Value::Number(n) => Ok(SignalValue::Float(n)),
            Value::String(s) => Ok(SignalValue::String(s.to_str()?.to_string().into())),
            Value::Table(t) => {
                let key = lua.create_registry_value(t)?;
                Ok(SignalValue::Table(key))
            }
            Value::UserData(ref ud) => {
                // If we receive a Signal or Derived, compute its value and recurse
                if ud.is::<LuaSignal>() {
                    let signal = ud.borrow::<LuaSignal>()?;
                    let runtime = lua.app_data_ref::<crate::SharedSignalRuntime>()
                        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))?;
                    let computed = runtime.get_signal(lua, signal.id)?;
                    Self::from_lua(lua, computed)  // Recurse with the computed value
                } else if ud.is::<LuaDerived>() {
                    let derived = ud.borrow::<LuaDerived>()?;
                    let runtime = lua.app_data_ref::<crate::SharedSignalRuntime>()
                        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))?;
                    let computed = runtime.get_derived(lua, derived.id)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                    Self::from_lua(lua, computed)  // Recurse with the computed value
                } else {
                    Err(mlua::Error::FromLuaConversionError {
                        from: "userdata",
                        to: "SignalValue".to_string(),
                        message: Some("Unsupported value type for signal".to_string()),
                    })
                }
            }
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "SignalValue".to_string(),
                message: Some("Unsupported value type for signal".to_string()),
            }),
        }
    }

    /// Check if value is truthy (for conditional evaluation)
    pub fn is_truthy(&self) -> bool {
        match self {
            SignalValue::Nil => false,
            SignalValue::Bool(false) => false,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nil_equality() {
        assert!(SignalValue::Nil.eq_value(&SignalValue::Nil));
    }

    #[test]
    fn test_bool_equality() {
        assert!(SignalValue::Bool(true).eq_value(&SignalValue::Bool(true)));
        assert!(!SignalValue::Bool(true).eq_value(&SignalValue::Bool(false)));
    }

    #[test]
    fn test_int_equality() {
        assert!(SignalValue::Int(42).eq_value(&SignalValue::Int(42)));
        assert!(!SignalValue::Int(42).eq_value(&SignalValue::Int(43)));
    }

    #[test]
    fn test_float_equality() {
        assert!(SignalValue::Float(3.14).eq_value(&SignalValue::Float(3.14)));
        assert!(!SignalValue::Float(3.14).eq_value(&SignalValue::Float(3.15)));

        // NaN handling
        assert!(SignalValue::Float(f64::NAN).eq_value(&SignalValue::Float(f64::NAN)));
    }

    #[test]
    fn test_string_equality() {
        assert!(SignalValue::String("hello".into()).eq_value(&SignalValue::String("hello".into())));
        assert!(
            !SignalValue::String("hello".into()).eq_value(&SignalValue::String("world".into()))
        );
    }

    #[test]
    fn test_truthy() {
        assert!(!SignalValue::Nil.is_truthy());
        assert!(!SignalValue::Bool(false).is_truthy());
        assert!(SignalValue::Bool(true).is_truthy());
        assert!(SignalValue::Int(0).is_truthy());
        assert!(SignalValue::Int(42).is_truthy());
        assert!(SignalValue::String("".into()).is_truthy());
    }
}
