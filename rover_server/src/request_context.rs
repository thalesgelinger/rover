use hyper::body::Bytes;
use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods};
use smallvec::SmallVec;
use std::collections::HashMap;

pub struct RequestContext {
    pub method: Bytes,
    pub path: Bytes,
    pub headers: SmallVec<[(Bytes, Bytes); 16]>,
    pub query: SmallVec<[(Bytes, Bytes); 8]>,
    pub params: HashMap<String, String>,
    pub body: Option<Bytes>,
}

impl UserData for RequestContext {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("method", |_, this| {
            std::str::from_utf8(&this.method)
                .map(|s| s.to_string())
                .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in method".to_string()))
        });
        
        fields.add_field_method_get("path", |_, this| {
            std::str::from_utf8(&this.path)
                .map(|s| s.to_string())
                .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in path".to_string()))
        });
        
        fields.add_field_method_get("body", |_, this| {
            match &this.body {
                Some(body) => std::str::from_utf8(body)
                    .map(|s| Some(s.to_string()))
                    .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in body".to_string())),
                None => Ok(None),
            }
        });
    }
    
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            match key.as_str() {
                "headers" => {
                    let tbl = lua.create_table()?;
                    for (k, v) in &this.headers {
                        let k_str = std::str::from_utf8(k)
                            .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in header name".to_string()))?;
                        let v_str = std::str::from_utf8(v)
                            .map_err(|_| mlua::Error::RuntimeError(format!("Invalid UTF-8 in header value for '{}'", k_str)))?;
                        tbl.set(k_str, v_str)?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                "query" => {
                    let tbl = lua.create_table()?;
                    for (k, v) in &this.query {
                        let k_str = std::str::from_utf8(k)
                            .map_err(|_| mlua::Error::RuntimeError("Invalid UTF-8 in query param name".to_string()))?;
                        let v_str = std::str::from_utf8(v)
                            .map_err(|_| mlua::Error::RuntimeError(format!("Invalid UTF-8 in query param '{}'", k_str)))?;
                        tbl.set(k_str, v_str)?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                "params" => {
                    let tbl = lua.create_table()?;
                    for (k, v) in &this.params {
                        tbl.set(k.as_str(), v.as_str())?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                _ => Ok(mlua::Value::Nil)
            }
        });
    }
}
