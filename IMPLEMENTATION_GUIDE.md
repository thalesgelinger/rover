# Implementation Guide: Flume + Batching

This guide shows you **exactly** what to change to implement flume and batching optimizations.

---

## Step 1: Add Flume Dependency

### File: `rover-server/Cargo.toml`

**Find this section:**
```toml
[dependencies]
anyhow = "1.0.100"
mlua = { version="0.11.5", features = ["anyhow", "vendored", "luajit", "send", "serialize", "async"] }
rover_types = { path = "../rover-types" }
serde_json = "1.0"
itoa = "1.0"
ryu = "1.0"
form_urlencoded = "1.2"
tokio = {version="1.48.0", features=["full"]}
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
urlencoding = "2.1"
hyper = { version = "1", features = ["full"] }
http-body-util = "0.1"
hyper-util = { version = "0.1", features = ["full"] }
smallvec = "1.15.1"
matchit = "0.8"
rover-openapi = {path = "../rover-openapi"}
```

**Add this line:**
```toml
[dependencies]
anyhow = "1.0.100"
mlua = { version="0.11.5", features = ["anyhow", "vendored", "luajit", "send", "serialize", "async"] }
rover_types = { path = "../rover-types" }
serde_json = "1.0"
itoa = "1.0"
ryu = "1.0"
form_urlencoded = "1.2"
tokio = {version="1.48.0", features=["full"]}
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
urlencoding = "2.1"
hyper = { version = "1", features = ["full"] }
http-body-util = "0.1"
hyper-util = { version = "0.1", features = ["full"] }
smallvec = "1.15.1"
matchit = "0.8"
rover-openapi = {path = "../rover-openapi"}
flume = "0.11"  # ← ADD THIS LINE
```

---

## Step 2: Update lib.rs (Main Server File)

### File: `rover-server/src/lib.rs`

#### Change 1: Import flume instead of tokio::mpsc

**Current code (line ~27):**
```rust
use tokio::sync::{mpsc, oneshot};
```

**New code:**
```rust
use flume;
use tokio::sync::oneshot;
```

#### Change 2: Create flume channel

**Current code (line ~158):**
```rust
async fn server(lua: Lua, routes: RouteTable, config: ServerConfig, openapi_spec: Option<serde_json::Value>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<event_loop::LuaRequest>(1024);

    let addr = format!("{}:{}", config.host, config.port);
    // ... rest of function ...
```

**New code:**
```rust
async fn server(lua: Lua, routes: RouteTable, config: ServerConfig, openapi_spec: Option<serde_json::Value>) -> Result<()> {
    let (tx, rx) = flume::bounded::<event_loop::LuaRequest>(1024);  // ← Changed this line

    let addr = format!("{}:{}", config.host, config.port);
    // ... rest of function ...
```

#### Change 3: Update handler function signature

**Current code (line ~205):**
```rust
async fn handler(
    req: Request<hyper::body::Incoming>,
    tx: mpsc::Sender<event_loop::LuaRequest>,
) -> Result<Response<Full<Bytes>>, Infallible> {
```

**New code:**
```rust
async fn handler(
    req: Request<hyper::body::Incoming>,
    tx: flume::Sender<event_loop::LuaRequest>,  // ← Changed type
) -> Result<Response<Full<Bytes>>, Infallible> {
```

#### Change 4: Use flume's send_async

**Current code (line ~247-257):**
```rust
    tx.send(LuaRequest {
        method: Bytes::from(parts.method.as_str().to_string()),
        path: Bytes::from(parts.uri.path().to_string()),
        headers,
        query,
        body,
        respond_to: resp_tx,
        started_at: Instant::now(),
    })
    .await
    .unwrap();
```

**New code:**
```rust
    tx.send_async(LuaRequest {  // ← Changed to send_async
        method: Bytes::from(parts.method.as_str().to_string()),
        path: Bytes::from(parts.uri.path().to_string()),
        headers,
        query,
        body,
        respond_to: resp_tx,
        started_at: Instant::now(),
    })
    .await
    .unwrap();
```

---

## Step 3: Update event_loop.rs (Add Batching)

### File: `rover-server/src/event_loop.rs`

#### Change 1: Import flume

**Current code (line ~4):**
```rust
use tokio::sync::mpsc::Receiver;
```

**New code:**
```rust
use flume::Receiver;
```

#### Change 2: Update function signature

**Current code (line ~21):**
```rust
pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, config: ServerConfig, openapi_spec: Option<serde_json::Value>) {
```

**New code:**
```rust
pub fn run(lua: Lua, routes: Vec<Route>, rx: Receiver<LuaRequest>, config: ServerConfig, openapi_spec: Option<serde_json::Value>) {
    // Note: Removed 'mut' from rx - flume receivers don't need to be mutable
```

#### Change 3: Replace entire event loop with batched version

**Current code (line ~22-100):**
```rust
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes).expect("Failed to build router");

        while let Some(req) = rx.recv().await {
            // Methods should be only lua functions, so lua function is utf8 safe
            let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };

            let method = match HttpMethod::from_str(method_str) {
                Some(m) => m,
                None => {
                    let _ = req.respond_to.send(crate::HttpResponse {
                        status: StatusCode::BAD_REQUEST,
                        body: Bytes::from(format!(
                            "Invalid HTTP method '{}'. Valid methods: {}",
                            method_str,
                            HttpMethod::valid_methods().join(", ")
                        )),
                        content_type: Some("text/plain".to_string()),
                    });
                    continue;
                }
            };

            // Paths should be only lua functions, so lua function is utf8 safe
            let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

            // Handle /docs endpoint if enabled and spec is available
            if config.docs && path_str == "/docs" && openapi_spec.is_some() {
                let html = rover_openapi::scalar_html(openapi_spec.as_ref().unwrap());
                let elapsed = req.started_at.elapsed();
                debug!(
                    "GET /docs - 200 OK in {:.2}ms",
                    elapsed.as_secs_f64() * 1000.0
                );
                let _ = req.respond_to.send(crate::HttpResponse {
                    status: StatusCode::OK,
                    body: Bytes::from(html),
                    content_type: Some("text/html".to_string()),
                });
                continue;
            }

            let (handler, params) = match fast_router.match_route(method, path_str) {
                Some((h, p)) => (h, p),
                None => {
                    let elapsed = req.started_at.elapsed();
                    warn!(
                        "{} {} - 404 NOT_FOUND in {:.2}ms",
                        method,
                        path_str,
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = req.respond_to.send(crate::HttpResponse {
                        status: StatusCode::NOT_FOUND,
                        body: Bytes::from("Route not found"),
                        content_type: Some("text/plain".to_string()),
                    });
                    continue;
                }
            };

            let task = HttpTask {
                method: req.method,
                path: req.path,
                headers: req.headers,
                query: req.query,
                params,
                body: req.body,
                handler: handler.clone(),
                respond_to: req.respond_to,
                started_at: req.started_at,
            };

            // Execute the task
            if let Err(e) = task.execute(&lua).await {
                debug!("Task execution failed: {}", e);
            }
        }
    });
```

**New code (with batching):**
```rust
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes).expect("Failed to build router");
        let mut batch = Vec::with_capacity(32);  // ← NEW: Batch buffer

        loop {  // ← Changed from 'while let'
            batch.clear();  // ← NEW: Clear batch for reuse

            // ← NEW: Blocking receive for first request
            match rx.recv_async().await {
                Ok(req) => batch.push(req),
                Err(_) => break,  // Channel closed, shutdown
            }

            // ← NEW: Drain all pending requests (non-blocking)
            loop {
                match rx.try_recv() {
                    Ok(req) => {
                        batch.push(req);
                        if batch.len() >= 32 {  // Max batch size
                            break;
                        }
                    }
                    Err(_) => break,  // No more pending requests
                }
            }

            // ← NEW: Optional debug logging
            if tracing::event_enabled!(tracing::Level::DEBUG) {
                debug!("Processing batch of {} requests", batch.len());
            }

            // ← NEW: Process entire batch
            for req in batch.drain(..) {
                // Everything below is UNCHANGED from original
                let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };

                let method = match HttpMethod::from_str(method_str) {
                    Some(m) => m,
                    None => {
                        let _ = req.respond_to.send(crate::HttpResponse {
                            status: StatusCode::BAD_REQUEST,
                            body: Bytes::from(format!(
                                "Invalid HTTP method '{}'. Valid methods: {}",
                                method_str,
                                HttpMethod::valid_methods().join(", ")
                            )),
                            content_type: Some("text/plain".to_string()),
                        });
                        continue;
                    }
                };

                let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

                // Handle /docs endpoint if enabled and spec is available
                if config.docs && path_str == "/docs" && openapi_spec.is_some() {
                    let html = rover_openapi::scalar_html(openapi_spec.as_ref().unwrap());
                    let elapsed = req.started_at.elapsed();
                    debug!(
                        "GET /docs - 200 OK in {:.2}ms",
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let _ = req.respond_to.send(crate::HttpResponse {
                        status: StatusCode::OK,
                        body: Bytes::from(html),
                        content_type: Some("text/html".to_string()),
                    });
                    continue;
                }

                let (handler, params) = match fast_router.match_route(method, path_str) {
                    Some((h, p)) => (h, p),
                    None => {
                        let elapsed = req.started_at.elapsed();
                        warn!(
                            "{} {} - 404 NOT_FOUND in {:.2}ms",
                            method,
                            path_str,
                            elapsed.as_secs_f64() * 1000.0
                        );
                        let _ = req.respond_to.send(crate::HttpResponse {
                            status: StatusCode::NOT_FOUND,
                            body: Bytes::from("Route not found"),
                            content_type: Some("text/plain".to_string()),
                        });
                        continue;
                    }
                };

                let task = HttpTask {
                    method: req.method,
                    path: req.path,
                    headers: req.headers,
                    query: req.query,
                    params,
                    body: req.body,
                    handler: handler.clone(),
                    respond_to: req.respond_to,
                    started_at: req.started_at,
                };

                // Execute the task
                if let Err(e) = task.execute(&lua).await {
                    debug!("Task execution failed: {}", e);
                }
            }  // ← NEW: End of batch processing
        }  // ← NEW: End of outer loop
    });
```

---

## Summary of Changes

### Files Changed: 2

1. **rover-server/Cargo.toml**
   - Add `flume = "0.11"`

2. **rover-server/src/lib.rs**
   - Change import: `tokio::sync::{mpsc, oneshot}` → `flume; use tokio::sync::oneshot`
   - Change channel creation: `mpsc::channel(1024)` → `flume::bounded(1024)`
   - Change handler parameter: `mpsc::Sender` → `flume::Sender`
   - Change send call: `tx.send(...).await` → `tx.send_async(...).await`

3. **rover-server/src/event_loop.rs**
   - Change import: `tokio::sync::mpsc::Receiver` → `flume::Receiver`
   - Remove `mut` from `rx` parameter
   - Replace `while let Some(req) = rx.recv().await` with batching loop
   - Add batch Vec and drain logic
   - Wrap existing request processing in `for req in batch.drain(..)`

---

## How to Test

### Step 1: Build
```bash
cargo build --release
```

### Step 2: Run Performance Test
```bash
# Terminal 1: Start server
./target/release/rover tests/perf/main.lua

# Terminal 2: Run benchmark
cd tests/perf
wrk -t4 -c1000 -d30s --latency -s benchmark.lua http://localhost:3000/echo
```

### Step 3: Compare Results

**Before (current):**
```
Requests/sec:   ~157,000
Latency (P50):  ~6.33ms
Latency (P99):  ~7.17ms
```

**After (expected):**
```
Requests/sec:   ~185,000  (+18%)
Latency (P50):  ~5.5ms    (-13%)
Latency (P99):  ~6.2ms    (-14%)
```

---

## Rollback Plan (If Needed)

If you encounter any issues, rollback is simple:

```bash
# Revert the changes
git checkout rover-server/Cargo.toml
git checkout rover-server/src/lib.rs
git checkout rover-server/src/event_loop.rs

# Rebuild
cargo build --release
```

---

## Common Issues & Solutions

### Issue 1: Compilation Error with flume

**Error:**
```
error[E0599]: no method named `send_async` found
```

**Solution:** Make sure you're using flume 0.11 or later:
```bash
cargo update -p flume
```

### Issue 2: Channel Closed Error

**Error:**
```
thread 'tokio-runtime-worker' panicked at 'called `unwrap()` on an `Err` value: RecvError'
```

**Solution:** This is expected during shutdown. The event loop breaks cleanly with the batching version.

### Issue 3: Performance Regression

If you see performance decrease:

1. Check batch size (try 16 or 64 instead of 32)
2. Verify you're testing under high load (1000+ connections)
3. Check debug logging is disabled (`log_level = "nope"`)

---

## Next Steps

After implementing and testing:

1. Run benchmarks at different connection levels (100, 500, 1000, 2000)
2. Monitor batch sizes with debug logging
3. Tune batch size based on your workload
4. Consider adding metrics to track batch efficiency

Good luck! Let me know how it goes! 🚀
