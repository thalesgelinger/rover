use mlua::{Lua, Table, Value};
use rover_core::http::create_http_module;
use rover_core::server::{AppServer, Server};
use rover_server::{HttpMethod, RoverResponse};

fn create_ctx(lua: &Lua, headers: &[(&str, &str)]) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;
    let headers_table = lua.create_table()?;
    for (key, value) in headers {
        headers_table.set(*key, *value)?;
    }

    let headers_clone = headers_table.clone();
    ctx.set(
        "headers",
        lua.create_function(move |_lua, _self: Table| Ok(headers_clone.clone()))?,
    )?;

    let state = lua.create_table()?;
    let state_set = state.clone();
    ctx.set(
        "set",
        lua.create_function(move |_lua, (_self, key, value): (Table, String, Value)| {
            state_set.set(key, value)?;
            Ok(())
        })?,
    )?;

    let state_get = state.clone();
    ctx.set(
        "get",
        lua.create_function(move |_lua, (_self, key): (Table, String)| {
            state_get.get::<Value>(key)
        })?,
    )?;

    Ok(ctx)
}

fn parse_response(value: Value) -> (u16, String) {
    let response_ud = value
        .as_userdata()
        .expect("middleware wrapper must return RoverResponse userdata");
    let response = response_ud
        .borrow::<RoverResponse>()
        .expect("response userdata must be RoverResponse");
    let body = std::str::from_utf8(&response.body)
        .expect("response body must be utf-8")
        .to_string();
    (response.status, body)
}

#[test]
fn should_enforce_protected_admin_route_end_to_end() {
    let lua = Lua::new();
    let rover = lua.create_table().expect("create rover table");
    rover
        .set(
            "server",
            lua.create_function(|lua, opts: Table| Ok(lua.create_server(opts)?))
                .expect("create server fn"),
        )
        .expect("set rover.server");
    lua.globals().set("rover", rover).expect("set global rover");

    let script = r#"
        local api = rover.server {}

        function api.before.global(ctx)
          ctx:set("request_scope", "global")
        end

        function api.admin.before.authn(ctx)
          if not ctx:headers().Authorization then
            return api:error(401, "Unauthorized: missing Authorization header")
          end
          ctx:set("role", "admin")
        end

        function api.admin.users.get(ctx)
          return api.json {
            scope = ctx:get("request_scope"),
            role = ctx:get("role"),
          }
        end

        return api
    "#;

    let app: Table = lua.load(script).eval().expect("load app");
    let routes = app.get_routes(&lua).expect("get routes");
    let route = routes
        .routes
        .iter()
        .find(|route| {
            route.method == HttpMethod::Get
                && std::str::from_utf8(route.pattern.as_ref()).ok() == Some("/admin/users")
        })
        .expect("route /admin/users must exist");

    let missing_auth_ctx = create_ctx(&lua, &[]).expect("create ctx without auth");
    let deny = route
        .handler
        .call::<Value>(missing_auth_ctx)
        .expect("call route without auth");
    let (deny_status, deny_body) = parse_response(deny);
    assert_eq!(deny_status, 401);
    assert!(deny_body.contains("Unauthorized: missing Authorization header"));

    let allowed_ctx =
        create_ctx(&lua, &[("Authorization", "Bearer test")]).expect("create ctx with auth");
    let ok = route
        .handler
        .call::<Value>(allowed_ctx)
        .expect("call route with auth");
    let (ok_status, ok_body) = parse_response(ok);
    assert_eq!(ok_status, 200);
    assert!(ok_body.contains("\"scope\":\"global\""));
    assert!(ok_body.contains("\"role\":\"admin\""));
}

#[test]
fn should_block_outbound_requests_to_private_networks_from_lua_http_module() {
    let lua = Lua::new();
    let http = create_http_module(&lua).expect("create http module");
    lua.globals().set("http", http).expect("set global http");

    let (ok, err): (bool, String) = lua
        .load(
            r#"
            local ok, err = pcall(function()
              return http.get("http://127.0.0.1:8080")
            end)

            return ok, tostring(err)
        "#,
        )
        .eval()
        .expect("execute outbound request script");

    assert!(!ok);
    assert!(err.contains("Blocked outbound IP address '127.0.0.1'"));
}
