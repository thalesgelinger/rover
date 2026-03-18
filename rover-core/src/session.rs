use anyhow::Result;
use mlua::{AnyUserData, Function, Lua, ObjectLike, Table, UserData, Value};
use rover_server::session::{SameSite, SessionConfig, SessionStore};
use rover_server::store::StoreValue;
use std::cell::RefCell;

/// Wrapper for SessionStore that can be shared with Lua
#[derive(Clone)]
struct LuaSessionStore {
    store: SessionStore,
}

impl UserData for LuaSessionStore {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("create", |_lua, this, ()| {
            let session = this.store.create_session();
            Ok(LuaSession::new(session))
        });

        methods.add_method("get_or_create", |_lua, this, session_id: Option<String>| {
            let session = this
                .store
                .get_or_create(session_id.as_deref())
                .map_err(mlua::Error::external)?;
            Ok(LuaSession::new(session))
        });

        methods.add_method("get", |_lua, this, session_id: String| {
            match this
                .store
                .get_session(&session_id)
                .map_err(mlua::Error::external)?
            {
                Some(session) => Ok(Some(LuaSession::new(session))),
                None => Ok(None),
            }
        });

        methods.add_method("delete", |_lua, this, session_id: String| {
            let deleted = this
                .store
                .delete_session(&session_id)
                .map_err(mlua::Error::external)?;
            Ok(deleted)
        });

        methods.add_method("exists", |_lua, this, session_id: String| {
            let exists = this
                .store
                .session_exists(&session_id)
                .map_err(mlua::Error::external)?;
            Ok(exists)
        });

        methods.add_method("cookie_name", |_lua, this, ()| {
            Ok(this.store.cookie_name().to_string())
        });
    }
}

/// Wrapper for Session that can be used in Lua
struct LuaSession {
    session: RefCell<rover_server::session::Session>,
}

impl LuaSession {
    fn new(session: rover_server::session::Session) -> Self {
        Self {
            session: RefCell::new(session),
        }
    }
}

impl UserData for LuaSession {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_lua, this, ()| {
            Ok(this.session.borrow().id().to_string())
        });

        methods.add_method("get", |_lua, this, key: String| {
            match this.session.borrow().get(&key) {
                Some(value) => Ok(Some(StoreValueWrapper(value.clone()))),
                None => Ok(None),
            }
        });

        methods.add_method("set", |_lua, this, (key, value): (String, Value)| {
            let store_value = lua_value_to_store_value(value)?;
            this.session.borrow_mut().set(key, store_value);
            Ok(())
        });

        methods.add_method("remove", |_lua, this, key: String| {
            match this.session.borrow_mut().remove(&key) {
                Some(value) => Ok(Some(StoreValueWrapper(value))),
                None => Ok(None),
            }
        });

        methods.add_method("has", |_lua, this, key: String| {
            Ok(this.session.borrow().contains_key(&key))
        });

        methods.add_method("save", |_lua, this, ()| {
            this.session
                .borrow_mut()
                .save()
                .map_err(mlua::Error::external)
        });

        methods.add_method("destroy", |_lua, this, ()| {
            this.session
                .borrow_mut()
                .destroy()
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        methods.add_method("regenerate", |_lua, this, ()| {
            let new_id = this
                .session
                .borrow_mut()
                .regenerate_id()
                .map_err(mlua::Error::external)?;
            Ok(new_id)
        });

        methods.add_method("cookie", |_lua, this, ()| {
            Ok(this.session.borrow().cookie_string())
        });

        methods.add_method("created_at", |_lua, this, ()| {
            Ok(this.session.borrow().created_at())
        });

        methods.add_method("last_accessed", |_lua, this, ()| {
            Ok(this.session.borrow().last_accessed())
        });

        methods.add_method("len", |_lua, this, ()| Ok(this.session.borrow().len()));

        methods.add_method("is_empty", |_lua, this, ()| {
            Ok(this.session.borrow().is_empty())
        });

        // Session lifecycle methods
        methods.add_method("is_expired", |_lua, this, ()| {
            Ok(this.session.borrow().is_expired())
        });

        methods.add_method("is_valid", |_lua, this, ()| {
            Ok(this.session.borrow().is_valid())
        });

        methods.add_method("state", |_lua, this, ()| {
            let state = this.session.borrow().state();
            let state_str = match state {
                rover_server::session::SessionState::Active => "active",
                rover_server::session::SessionState::Expired => "expired",
                rover_server::session::SessionState::Invalidated => "invalidated",
            };
            Ok(state_str.to_string())
        });

        methods.add_method_mut("refresh", |_lua, this, ()| {
            this.session
                .borrow_mut()
                .refresh()
                .map_err(mlua::Error::external)
        });

        methods.add_method_mut("invalidate", |_lua, this, ()| {
            this.session
                .borrow_mut()
                .invalidate()
                .map_err(mlua::Error::external)
        });
    }
}

/// Wrapper for StoreValue to implement UserData
struct StoreValueWrapper(StoreValue);

impl UserData for StoreValueWrapper {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("as_string", |_lua, this, ()| {
            Ok(this.0.as_string().map(|s| s.to_string()))
        });

        methods.add_method("as_integer", |_lua, this, ()| Ok(this.0.as_integer()));

        methods.add_method("as_bool", |_lua, this, ()| Ok(this.0.as_bool()));
    }
}

/// Convert Lua Value to StoreValue
fn lua_value_to_store_value(value: Value) -> mlua::Result<StoreValue> {
    match value {
        Value::String(s) => Ok(StoreValue::String(s.to_str()?.to_string())),
        Value::Integer(i) => Ok(StoreValue::Integer(i)),
        Value::Number(n) => Ok(StoreValue::Integer(n as i64)),
        Value::Boolean(b) => Ok(StoreValue::Boolean(b)),
        _ => Err(mlua::Error::RuntimeError(
            "Cannot convert value to session store type".to_string(),
        )),
    }
}

/// Parse SameSite from string
fn parse_same_site(s: &str) -> mlua::Result<SameSite> {
    match s.to_lowercase().as_str() {
        "strict" => Ok(SameSite::Strict),
        "lax" => Ok(SameSite::Lax),
        "none" => Ok(SameSite::None),
        _ => Err(mlua::Error::RuntimeError(format!(
            "Invalid SameSite value: {}. Use 'strict', 'lax', or 'none'",
            s
        ))),
    }
}

/// Create a new session store
fn create_store(lua: &Lua, opts: Option<Table>) -> mlua::Result<Table> {
    let mut config = SessionConfig::default();

    if let Some(opts) = opts {
        if let Value::String(s) = opts.get("cookie_name")? {
            config.cookie_name = s.to_str()?.to_string();
        }

        match opts.get("ttl")? {
            Value::Integer(i) if i > 0 => config.ttl_secs = i as u64,
            Value::Number(n) if n > 0.0 => config.ttl_secs = n as u64,
            Value::Nil => {}
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "ttl must be a positive number".to_string(),
                ));
            }
        }

        if let Value::Boolean(b) = opts.get("secure")? {
            config.secure = b;
        }

        if let Value::Boolean(b) = opts.get("http_only")? {
            config.http_only = b;
        }

        if let Value::String(s) = opts.get("same_site")? {
            config.same_site =
                parse_same_site(s.to_str().map_err(mlua::Error::external)?.as_ref())?;
        }

        if let Value::String(s) = opts.get("domain")? {
            config.domain = Some(s.to_str()?.to_string());
        }

        if let Value::String(s) = opts.get("path")? {
            config.path = s.to_str()?.to_string();
        }
    }

    let store = SessionStore::new(config);
    let lua_store = LuaSessionStore { store };

    let table = lua.create_table()?;
    table.set("_store", lua_store)?;

    // Bind methods
    table.set(
        "create",
        lua.create_function(|_lua, table: Table| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            let session = store.store.create_session();
            Ok(LuaSession::new(session))
        })?,
    )?;

    table.set(
        "get_or_create",
        lua.create_function(|_lua, (table, session_id): (Table, Option<String>)| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            let session = store
                .store
                .get_or_create(session_id.as_deref())
                .map_err(mlua::Error::external)?;
            Ok(LuaSession::new(session))
        })?,
    )?;

    table.set(
        "get",
        lua.create_function(|_lua, (table, session_id): (Table, String)| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            match store
                .store
                .get_session(&session_id)
                .map_err(mlua::Error::external)?
            {
                Some(session) => Ok(Some(LuaSession::new(session))),
                None => Ok(None),
            }
        })?,
    )?;

    table.set(
        "delete",
        lua.create_function(|_lua, (table, session_id): (Table, String)| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            let deleted = store
                .store
                .delete_session(&session_id)
                .map_err(mlua::Error::external)?;
            Ok(deleted)
        })?,
    )?;

    table.set(
        "exists",
        lua.create_function(|_lua, (table, session_id): (Table, String)| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            let exists = store
                .store
                .session_exists(&session_id)
                .map_err(mlua::Error::external)?;
            Ok(exists)
        })?,
    )?;

    table.set(
        "cookie_name",
        lua.create_function(|_lua, table: Table| {
            let store: mlua::UserDataRef<LuaSessionStore> = table.get("_store")?;
            Ok(store.store.cookie_name().to_string())
        })?,
    )?;

    Ok(table)
}

/// Create session middleware that automatically manages sessions
fn create_session_middleware(
    lua: &Lua,
    (store_table, _opts): (Table, Option<Table>),
) -> mlua::Result<Function> {
    let cookie_name: String = store_table
        .get::<Function>("cookie_name")?
        .call(store_table.clone())?;

    let middleware = lua.create_function(move |_lua, ctx: Value| {
        // Get request headers
        let headers: Table = match &ctx {
            Value::UserData(ud) => {
                let headers_fn: Function = ud.get("headers")?;
                headers_fn.call(())?
            }
            Value::Table(table) => {
                let headers_fn: Function = table.get("headers")?;
                headers_fn.call((table.clone(),))?
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "middleware ctx must be table or userdata".to_string(),
                ));
            }
        };

        // Get cookie header
        let cookie_header: Option<String> = headers.get("cookie")?;
        let session_id: Option<String> = cookie_header.and_then(|h: String| {
            h.split(';').find_map(|cookie: &str| {
                let mut parts = cookie.trim().splitn(2, '=');
                let name = parts.next()?;
                let value = parts.next()?;
                if name == cookie_name {
                    Some(value.to_string())
                } else {
                    None
                }
            })
        });

        // Get or create session
        let session = store_table
            .get::<Function>("get_or_create")?
            .call::<AnyUserData>((store_table.clone(), session_id))?;

        // Store session in context
        match &ctx {
            Value::UserData(ud) => {
                let set_fn: Function = ud.get("set")?;
                set_fn.call::<()>(("session", session))?;
            }
            Value::Table(table) => {
                let set_fn: Function = table.get("set")?;
                set_fn.call::<()>((table.clone(), "session", session))?;
            }
            _ => {}
        }

        Ok(())
    })?;

    Ok(middleware)
}

/// Create the rover.session module
pub fn create_session_module(lua: &Lua) -> Result<Table> {
    let session_module = lua.create_table()?;

    // rover.session.new(opts?) - Create a new session store
    session_module.set("new", lua.create_function(create_store)?)?;

    // rover.session.middleware(store, opts?) - Create session middleware
    session_module.set(
        "middleware",
        lua.create_function(create_session_middleware)?,
    )?;

    Ok(session_module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_store() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        // Test creating a session using Lua
        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            assert(session:id() ~= nil)
            assert(#session:id() > 0)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_get_set() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            
            session:set("user_id", "123")
            session:set("count", 42)
            session:set("active", true)
            
            local user_id = session:get("user_id")
            assert(user_id:as_string() == "123")
            
            local count = session:get("count")
            assert(count:as_integer() == 42)
            
            local active = session:get("active")
            assert(active:as_bool() == true)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_save_and_retrieve() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local id = session:id()
            
            session:set("key", "value")
            session:save()
            
            local retrieved = store:get(id)
            assert(retrieved ~= nil)
            local value = retrieved:get("key")
            assert(value:as_string() == "value")
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_destroy() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local id = session:id()
            
            session:set("key", "value")
            session:save()
            
            assert(store:exists(id) == true)
            session:destroy()
            assert(store:exists(id) == false)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_regenerate() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local old_id = session:id()
            
            session:set("key", "value")
            session:save()
            
            local new_id = session:regenerate()
            assert(old_id ~= new_id)
            assert(session:id() == new_id)
            
            assert(store:exists(old_id) == false)
            
            local retrieved = store:get(new_id)
            local value = retrieved:get("key")
            assert(value:as_string() == "value")
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_cookie() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local cookie = session:cookie()
            
            assert(cookie:find("rover_session=") ~= nil)
            assert(cookie:find("Path=/") ~= nil)
            assert(cookie:find("SameSite=Lax") ~= nil)
            assert(cookie:find("HttpOnly") ~= nil)
            assert(cookie:find("Secure") ~= nil)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_config() {
        let lua = Lua::new();
        let opts = lua.create_table().unwrap();
        opts.set("cookie_name", "my_session").unwrap();
        opts.set("ttl", 7200i64).unwrap();
        opts.set("secure", false).unwrap();
        opts.set("http_only", false).unwrap();
        opts.set("same_site", "strict").unwrap();
        opts.set("domain", "example.com").unwrap();
        opts.set("path", "/api").unwrap();

        let store = create_store(&lua, Some(opts)).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local cookie = session:cookie()
            
            assert(cookie:find("my_session=") ~= nil)
            assert(cookie:find("Path=/api") ~= nil)
            assert(cookie:find("SameSite=Strict") ~= nil)
            assert(cookie:find("HttpOnly") == nil)
            assert(cookie:find("Secure") == nil)
            assert(cookie:find("Domain=example.com") ~= nil)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_module() {
        let lua = Lua::new();
        let module = create_session_module(&lua).unwrap();

        // Test that module has new function
        let new_fn: Function = module.get("new").unwrap();
        assert!(new_fn.call::<Table>(()).is_ok());

        // Test that module has middleware function
        let _: Function = module.get("middleware").unwrap();
    }

    #[test]
    fn test_session_get_or_create() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            
            -- Create new session with no ID
            local session1 = store:get_or_create(nil)
            assert(session1:id() ~= nil)
            
            -- Create new session with non-existent ID
            local session2 = store:get_or_create("nonexistent")
            assert(session2:id() ~= "nonexistent")
            
            -- Create and save a session
            local session3 = store:create()
            local id = session3:id()
            session3:set("data", "test")
            session3:save()
            
            -- Get existing session
            local session4 = store:get_or_create(id)
            assert(session4:id() == id)
            local data = session4:get("data")
            assert(data:as_string() == "test")
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_has_and_remove() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            
            assert(session:has("key") == false)
            
            session:set("key", "value")
            assert(session:has("key") == true)
            
            local removed = session:remove("key")
            assert(removed ~= nil)
            assert(removed:as_string() == "value")
            assert(session:has("key") == false)
            
            -- Removing non-existent key returns nil
            local removed2 = session:remove("nonexistent")
            assert(removed2 == nil)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_len_and_is_empty() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            
            assert(session:is_empty() == true)
            assert(session:len() == 0)
            
            session:set("key1", "value1")
            session:set("key2", "value2")
            
            assert(session:is_empty() == false)
            assert(session:len() == 2)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_timestamps() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            
            local created = session:created_at()
            local accessed = session:last_accessed()
            
            assert(created > 0)
            assert(accessed >= created)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_lifecycle_state() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            
            -- New session should be active
            assert(session:state() == "active")
            assert(session:is_valid() == true)
            assert(session:is_expired() == false)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_invalidate() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local id = session:id()
            
            session:set("key", "value")
            session:save()
            
            -- Invalidate session
            session:invalidate()
            
            -- Should be invalidated
            assert(session:state() == "invalidated")
            assert(session:is_valid() == false)
            
            -- Retrieve and verify state persisted
            local retrieved = store:get(id)
            assert(retrieved ~= nil)
            assert(retrieved:state() == "invalidated")
            assert(retrieved:is_valid() == false)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_refresh() {
        let lua = Lua::new();
        let store = create_store(&lua, None).unwrap();

        let result: mlua::Result<()> = lua
            .load(
                r#"
            local store = ...
            local session = store:create()
            local id = session:id()
            local old_accessed = session:last_accessed()
            
            session:set("key", "value")
            session:save()
            
            -- Small delay
            -- (In Lua we can't sleep easily, but refresh should still work)
            
            -- Refresh session
            session:refresh()
            
            -- Session should still be valid
            assert(session:is_valid() == true)
            assert(session:state() == "active")
            
            -- Retrieve and verify still valid
            local retrieved = store:get(id)
            assert(retrieved ~= nil)
            assert(retrieved:is_valid() == true)
        "#,
            )
            .call(store);
        assert!(result.is_ok());
    }
}
