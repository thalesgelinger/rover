use axum::{Router, routing::get};

async fn server() {
    // build our application with a single route
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

pub fn run() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(server());
}
