use core::fmt;

use anyhow::{Result, anyhow};
use axum::{Router, extract::Request, response::IntoResponse, routing::any};
use mlua::Function;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "get" => Ok(HttpMethod::Get),
            "post" => Ok(HttpMethod::Post),
            "put" => Ok(HttpMethod::Put),
            "delete" => Ok(HttpMethod::Delete),
            "patch" => Ok(HttpMethod::Patch),
            _ => Err(anyhow!("Unknown HTTP method: '{}'", s)),
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Patch => write!(f, "PATCH"),
        }
    }
}

pub struct ServerRoute {
    method: HttpMethod,
    path: String,
    handler: Function,
}

impl ServerRoute {
    pub fn new(method: HttpMethod, path: String, handler: Function) -> Self {
        ServerRoute {
            method,
            path,
            handler,
        }
    }
}

async fn server() {
    // let (tx, rx) = mpsc::channel(1024);
    // build our application with a single route
    let app = Router::new().fallback(any({
        // move |req| handle_all(req, tx.clone());
    }));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// fn handle_all(req: Request, tx: mpsc::Sender<LuaRequest>) -> impl IntoResponse {
//     let (resp_tx, resp_rx) = oneshot::channel();
// }

pub fn run(routes: &Vec<ServerRoute>) {
    for route in routes {
        println!(" {} {}", route.method.to_string(), route.path);
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(server());
}
