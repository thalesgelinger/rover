use crate::SharedSignalRuntime;
use crate::lua::derived::LuaDerived;
use crate::lua::signal::LuaSignal;
use mlua::{Lua, MetaMethod, Result, UserDataMethods, Value};

/// Helper to create a derived signal from a binary operation
fn create_binary_op_derived(
    lua: &Lua,
    lhs: Value,
    rhs: Value,
    op: &'static str,
) -> Result<LuaDerived> {
    // Create a closure that captures lhs and rhs and performs the operation
    let compute_fn = lua.create_function(move |lua, ()| {
        let lhs_val = get_signal_value(lua, lhs.clone())?;
        let rhs_val = get_signal_value(lua, rhs.clone())?;

        match op {
            "add" => perform_add(lhs_val, rhs_val),
            "sub" => perform_sub(lhs_val, rhs_val),
            "mul" => perform_mul(lhs_val, rhs_val),
            "div" => perform_div(lhs_val, rhs_val),
            "mod" => perform_mod(lhs_val, rhs_val),
            "pow" => perform_pow(lhs_val, rhs_val),
            "concat" => perform_concat(lua, lhs_val, rhs_val),
            "eq" => Ok(Value::Boolean(values_eq(lhs_val, rhs_val))),
            "lt" => perform_lt(lhs_val, rhs_val),
            "le" => perform_le(lhs_val, rhs_val),
            _ => Err(mlua::Error::RuntimeError(format!(
                "Unknown operation: {}",
                op
            ))),
        }
    })?;

    let key = lua.create_registry_value(compute_fn)?;

    let runtime = get_runtime(lua)?;

    let id = runtime.create_derived(key);
    Ok(LuaDerived::new(id))
}

/// Helper to create a derived signal from a unary operation
fn create_unary_op_derived(lua: &Lua, operand: Value, op: &'static str) -> Result<LuaDerived> {
    let compute_fn = lua.create_function(move |lua, ()| {
        let val = get_signal_value(lua, operand.clone())?;

        match op {
            "unm" => perform_unm(val),
            _ => Err(mlua::Error::RuntimeError(format!(
                "Unknown operation: {}",
                op
            ))),
        }
    })?;

    let key = lua.create_registry_value(compute_fn)?;

    let runtime = get_runtime(lua)?;

    let id = runtime.create_derived(key);
    Ok(LuaDerived::new(id))
}

/// Get signal value (from signal or derived)
fn get_signal_value(lua: &Lua, value: Value) -> Result<Value> {
    match value {
        Value::UserData(ref ud) => {
            if let Ok(signal) = ud.borrow::<LuaSignal>() {
                let runtime = lua.app_data_ref::<SharedSignalRuntime>().ok_or_else(|| {
                    mlua::Error::RuntimeError("Signal runtime not initialized".into())
                })?;
                runtime.get_signal(lua, signal.id)
            } else if let Ok(derived) = ud.borrow::<LuaDerived>() {
                let runtime = lua.app_data_ref::<SharedSignalRuntime>().ok_or_else(|| {
                    mlua::Error::RuntimeError("Signal runtime not initialized".into())
                })?;
                runtime
                    .get_derived(lua, derived.id)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
            } else {
                Ok(value)
            }
        }
        _ => Ok(value),
    }
}

/// Get runtime from Lua app_data
fn get_runtime(lua: &Lua) -> Result<mlua::AppDataRef<'_, SharedSignalRuntime>> {
    lua.app_data_ref::<SharedSignalRuntime>()
        .ok_or_else(|| mlua::Error::RuntimeError("Signal runtime not initialized".into()))
}

// Arithmetic operations
fn perform_add(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 + b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a + b as f64)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot add non-numeric values".to_string(),
        )),
    }
}

fn perform_sub(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 - b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a - b as f64)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot subtract non-numeric values".to_string(),
        )),
    }
}

fn perform_mul(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 * b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a * b as f64)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot multiply non-numeric values".to_string(),
        )),
    }
}

fn perform_div(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Number(a as f64 / b as f64)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 / b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a / b as f64)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot divide non-numeric values".to_string(),
        )),
    }
}

fn perform_mod(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number(a as f64 % b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a % b as f64)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot mod non-numeric values".to_string(),
        )),
    }
}

fn perform_pow(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Number((a as f64).powf(b as f64))),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(b))),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Number((a as f64).powf(b as f64))),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Number(a.powf(b as f64))),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot pow non-numeric values".to_string(),
        )),
    }
}

fn perform_unm(operand: Value) -> Result<Value> {
    match operand {
        Value::Integer(a) => Ok(Value::Integer(-a)),
        Value::Number(a) => Ok(Value::Number(-a)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot negate non-numeric value".to_string(),
        )),
    }
}

fn perform_concat(lua: &Lua, lhs: Value, rhs: Value) -> Result<Value> {
    let lhs_str = value_to_string(lhs)?;
    let rhs_str = value_to_string(rhs)?;
    Ok(Value::String(
        lua.create_string(&format!("{}{}", lhs_str, rhs_str))?,
    ))
}

fn value_to_string(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Nil => Ok("nil".to_string()),
        _ => Ok(format!("{:?}", value)),
    }
}

fn values_eq(lhs: Value, rhs: Value) -> bool {
    match (lhs, rhs) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a.as_bytes() == b.as_bytes(),
        _ => false,
    }
}

fn perform_lt(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a < b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Boolean(a < b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Boolean((a as f64) < b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Boolean(a < b as f64)),
        (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a.as_bytes() < b.as_bytes())),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot compare values".to_string(),
        )),
    }
}

fn perform_le(lhs: Value, rhs: Value) -> Result<Value> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b)),
        (Value::Number(a), Value::Number(b)) => Ok(Value::Boolean(a <= b)),
        (Value::Integer(a), Value::Number(b)) => Ok(Value::Boolean((a as f64) <= b)),
        (Value::Number(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b as f64)),
        (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a.as_bytes() <= b.as_bytes())),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot compare values".to_string(),
        )),
    }
}

/// Add metamethods to LuaSignal
pub fn add_signal_metamethods<M: UserDataMethods<LuaSignal>>(methods: &mut M) {
    // Arithmetic
    methods.add_meta_method(MetaMethod::Add, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "add",
        )
    });

    methods.add_meta_method(MetaMethod::Sub, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "sub",
        )
    });

    methods.add_meta_method(MetaMethod::Mul, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "mul",
        )
    });

    methods.add_meta_method(MetaMethod::Div, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "div",
        )
    });

    methods.add_meta_method(MetaMethod::Mod, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "mod",
        )
    });

    methods.add_meta_method(MetaMethod::Pow, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "pow",
        )
    });

    // Unary
    methods.add_meta_method(MetaMethod::Unm, |lua, this, ()| {
        create_unary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            "unm",
        )
    });

    // Concatenation
    methods.add_meta_function(MetaMethod::Concat, |lua, (lhs, rhs): (Value, Value)| {
        let lhs_val = match &lhs {
            Value::UserData(ud) => {
                if let Ok(signal) = ud.borrow::<LuaSignal>() {
                    Value::UserData(lua.create_userdata(LuaSignal::new(signal.id))?)
                } else {
                    lhs
                }
            }
            _ => lhs,
        };
        create_binary_op_derived(lua, lhs_val, rhs, "concat")
    });

    // Comparison
    methods.add_meta_method(MetaMethod::Eq, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "eq",
        )
    });

    methods.add_meta_method(MetaMethod::Lt, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "lt",
        )
    });

    methods.add_meta_method(MetaMethod::Le, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaSignal::new(this.id))?),
            other,
            "le",
        )
    });
}

/// Add metamethods to LuaDerived
pub fn add_derived_metamethods<M: UserDataMethods<LuaDerived>>(methods: &mut M) {
    // Same metamethods as LuaSignal
    methods.add_meta_method(MetaMethod::Add, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "add",
        )
    });

    methods.add_meta_method(MetaMethod::Sub, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "sub",
        )
    });

    methods.add_meta_method(MetaMethod::Mul, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "mul",
        )
    });

    methods.add_meta_method(MetaMethod::Div, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "div",
        )
    });

    methods.add_meta_method(MetaMethod::Mod, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "mod",
        )
    });

    methods.add_meta_method(MetaMethod::Pow, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "pow",
        )
    });

    // Unary
    methods.add_meta_method(MetaMethod::Unm, |lua, this, ()| {
        create_unary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            "unm",
        )
    });

    // Concatenation
    methods.add_meta_function(MetaMethod::Concat, |lua, (lhs, rhs): (Value, Value)| {
        let lhs_val = match &lhs {
            Value::UserData(ud) => {
                if let Ok(derived) = ud.borrow::<LuaDerived>() {
                    Value::UserData(lua.create_userdata(LuaDerived::new(derived.id))?)
                } else {
                    lhs
                }
            }
            _ => lhs,
        };
        create_binary_op_derived(lua, lhs_val, rhs, "concat")
    });

    // Comparison
    methods.add_meta_method(MetaMethod::Eq, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "eq",
        )
    });

    methods.add_meta_method(MetaMethod::Lt, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "lt",
        )
    });

    methods.add_meta_method(MetaMethod::Le, |lua, this, other: Value| {
        create_binary_op_derived(
            lua,
            Value::UserData(lua.create_userdata(LuaDerived::new(this.id))?),
            other,
            "le",
        )
    });
}
