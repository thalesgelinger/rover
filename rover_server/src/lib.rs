use std::collections::HashMap;

use axum::{
    Router, body, extract::Request, http::StatusCode, response::IntoResponse, routing::any,
};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value};
use tokio::sync::{mpsc, oneshot};

struct LuaRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Option<String>,
    respond_to: oneshot::Sender<LuaResponse>,
}

struct LuaResponse {
    status: StatusCode,
    body: String,
}

fn lua_table_to_json(lua: &Lua, table: Table) -> mlua::Result<String> {
    let json_value: serde_json::Value = lua.from_value(Value::Table(table))?;
    Ok(serde_json::to_string(&json_value).unwrap())
}

fn build_lua_context(lua: &Lua, req: &LuaRequest) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("method", req.method.as_str())?;
    ctx.set("path", req.path.as_str())?;

    let headers = lua.create_table()?;
    for (k, v) in &req.headers {
        headers.set(k.as_str(), v.as_str())?;
    }
    ctx.set("headers", headers)?;

    let query = lua.create_table()?;
    for (k, v) in &req.query {
        query.set(k.as_str(), v.as_str())?;
    }
    ctx.set("query", query)?;

    if let Some(body) = &req.body {
        ctx.set("body", body.as_str())?;
    }

    Ok(ctx)
}

async fn server(lua: Lua, routes: Routes) {
    let (tx, rx) = mpsc::channel(1024);
    event_loop(lua, routes, rx);

    let app = Router::new().fallback(any(move |req| handle_all(req, tx.clone())));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn event_loop(lua: Lua, routes: Routes, mut rx: mpsc::Receiver<LuaRequest>) {
    std::thread::spawn(move || {
        while let Some(req) = rx.blocking_recv() {
            let handler = match routes.get(&(req.method.clone(), req.path.clone())) {
                Some(h) => h,
                None => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::NOT_FOUND,
                        body: "Route not found".to_string(),
                    });
                    continue;
                }
            };

            let ctx = match build_lua_context(&lua, &req) {
                Ok(c) => c,
                Err(e) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: format!("Failed to build context: {}", e),
                    });
                    continue;
                }
            };

            let result: Value = match handler.call(ctx) {
                Ok(r) => r,
                Err(e) => {
                    let _ = req.respond_to.send(LuaResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        body: format!("Lua error: {}", e),
                    });
                    continue;
                }
            };

            let (status, body) = match result {
                Value::String(ref s) => (StatusCode::OK, s.to_str().unwrap().to_string()),

                Value::Table(table) => {
                    if let Ok(status_code) = table.get::<u16>("status") {
                        if status_code >= 400 {
                            let message = table
                                .get::<String>("message")
                                .unwrap_or_else(|_| "Error".to_string());
                            (
                                StatusCode::from_u16(status_code)
                                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                                message,
                            )
                        } else {
                            let body = lua_table_to_json(&lua, table).unwrap_or_else(|e| {
                                format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
                            });
                            (
                                StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                                body,
                            )
                        }
                    } else {
                        let json = lua_table_to_json(&lua, table).unwrap_or_else(|e| {
                            format!("{{\"error\":\"Failed to serialize: {}\"}}", e)
                        });
                        (StatusCode::OK, json)
                    }
                }

                Value::Integer(i) => (StatusCode::OK, i.to_string()),
                Value::Number(n) => (StatusCode::OK, n.to_string()),

                Value::Boolean(b) => (StatusCode::OK, b.to_string()),

                Value::Nil => (StatusCode::NO_CONTENT, String::new()),

                Value::Error(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),

                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unsupported return type".to_string(),
                ),
            };

            let _ = req.respond_to.send(LuaResponse { status, body });
        }
    });
}

async fn handle_all(req: Request, tx: mpsc::Sender<LuaRequest>) -> impl IntoResponse {
    let (parts, body_stream) = req.into_parts();

    let headers: HashMap<String, String> = parts
        .headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    let query: HashMap<String, String> = parts
        .uri
        .query()
        .map(|q| {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();

    let body_bytes = body::to_bytes(body_stream, usize::MAX).await.unwrap();
    let body_str = if !body_bytes.is_empty() {
        Some(String::from_utf8_lossy(&body_bytes).to_string())
    } else {
        None
    };

    let (resp_tx, resp_rx) = oneshot::channel();

    tx.send(LuaRequest {
        method: parts.method.to_string().to_lowercase(),
        path: parts.uri.path().to_string(),
        headers,
        query,
        body: body_str,
        respond_to: resp_tx,
    })
    .await
    .unwrap();

    let resp = resp_rx.await.unwrap();
    (resp.status, resp.body)
}

pub fn run(lua: Lua, routes: Routes) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(server(lua, routes));
}

pub type Routes = HashMap<(String, String), Function>;
