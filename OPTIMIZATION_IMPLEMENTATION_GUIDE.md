# Rover Performance Optimization - Implementation Guide

This document provides detailed, code-level implementation guidance for each optimization identified in PERFORMANCE_ANALYSIS.md.

---

## 1. Multi-Threaded Lua Workers

### Current Architecture Problem

**File**: `rover-server/src/event_loop.rs:21-101`

```rust
pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, ...) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes)?;

        // BOTTLENECK: Single thread processes ALL requests
        while let Some(req) = rx.recv().await {
            // ... routing logic ...
            task.execute(&lua).await;  // Sequential execution
        }
    });
}
```

### Proposed Solution: Worker Pool

```rust
use tokio::sync::mpsc;
use std::sync::Arc;

pub fn run(
    lua_template: Lua,
    routes: Vec<Route>,
    mut rx: Receiver<LuaRequest>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>
) {
    let router = Arc::new(FastRouter::from_routes(routes).expect("Failed to build router"));
    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // Create worker pool
    for worker_id in 0..num_workers {
        let lua = create_lua_vm(&lua_template); // Clone Lua state
        let router = router.clone();
        let config = config.clone();
        let openapi_spec = openapi_spec.clone();
        let mut rx = rx.clone();  // Multiple receivers (requires broadcast or shared channel)

        tokio::spawn(async move {
            tracing::debug!("Worker {} started", worker_id);

            while let Some(req) = rx.recv().await {
                // ... existing routing logic ...
                if let Err(e) = task.execute(&lua).await {
                    tracing::debug!("Worker {} task failed: {}", worker_id, e);
                }
            }
        });
    }
}

fn create_lua_vm(template: &Lua) -> Lua {
    // Clone Lua VM state (need to implement)
    // For now, could recreate from scratch
    let lua = Lua::new();
    // Re-register all global functions, tables, etc.
    lua
}
```

### Alternative: Channel Per Worker

```rust
pub fn run(
    lua_template: Lua,
    routes: Vec<Route>,
    config: ServerConfig,
    openapi_spec: Option<serde_json::Value>
) -> Vec<mpsc::Sender<LuaRequest>> {
    let router = Arc::new(FastRouter::from_routes(routes).expect("Failed to build router"));
    let num_workers = num_cpus::get();
    let mut senders = Vec::new();

    for worker_id in 0..num_workers {
        let (tx, mut rx) = mpsc::channel::<LuaRequest>(256);  // Smaller buffer per worker
        senders.push(tx);

        let lua = create_lua_vm(&lua_template);
        let router = router.clone();
        let config = config.clone();
        let openapi_spec = openapi_spec.clone();

        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                // Existing logic
                let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };
                let method = match HttpMethod::from_str(method_str) { /* ... */ };
                let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

                let (handler, params) = match router.match_route(method, path_str) {
                    Some(r) => r,
                    None => { /* 404 response */ continue; }
                };

                let task = HttpTask { /* ... */ };
                let _ = task.execute(&lua).await;
            }
        });
    }

    senders
}
```

### Handler Selection Strategy

**File**: `rover-server/src/lib.rs:205-275`

```rust
async fn handler(
    req: Request<hyper::body::Incoming>,
    workers: Arc<Vec<mpsc::Sender<LuaRequest>>>,
    worker_counter: Arc<AtomicUsize>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // Round-robin worker selection
    let worker_id = worker_counter.fetch_add(1, Ordering::Relaxed) % workers.len();

    // ... existing request parsing ...

    workers[worker_id]
        .send(LuaRequest { /* ... */ })
        .await
        .unwrap();

    let resp = resp_rx.await.unwrap();
    // ... existing response building ...
}
```

**Expected Impact**: 40-60% throughput increase

---

## 2. Lazy Context Creation

### Current Implementation

**File**: `rover-server/src/http_task.rs:159-256`

```rust
fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    let ctx = lua.create_table()?;

    // PROBLEM: Always creates ALL closures, even if unused
    let req_data = Arc::new(RequestData { /* ... */ });

    let req_data_headers = req_data.clone();  // Arc clone
    let headers_fn = lua.create_function(move |lua, ()| { /* ... */ })?;  // Closure creation
    ctx.set("headers", headers_fn)?;

    // ... same for query, params, body ...

    Ok(ctx)
}
```

### Solution A: Metatable with __index

```rust
fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("method", std::str::from_utf8(&task.method)?)?;
    ctx.set("path", std::str::from_utf8(&task.path)?)?;

    // Store raw data in userdata (opaque to Lua)
    let req_data = Arc::new(RequestData {
        headers: task.headers.clone(),
        query: task.query.clone(),
        params: task.params.clone(),
        body: task.body.clone(),
    });

    // Create metatable with lazy __index
    let metatable = lua.create_table()?;
    metatable.set("__index", lua.create_function(|lua, (table, key): (Table, String)| {
        // Retrieve stored data
        let req_data: Arc<RequestData> = table.raw_get("__req_data")?;

        match key.as_str() {
            "headers" => {
                if req_data.headers.is_empty() {
                    return lua.create_table();
                }
                let headers = lua.create_table_with_capacity(0, req_data.headers.len())?;
                for (k, v) in &req_data.headers {
                    headers.set(
                        std::str::from_utf8(k)?,
                        std::str::from_utf8(v)?
                    )?;
                }
                Ok(Value::Table(headers))
            }
            "query" => {
                if req_data.query.is_empty() {
                    return lua.create_table();
                }
                let query = lua.create_table_with_capacity(0, req_data.query.len())?;
                for (k, v) in &req_data.query {
                    query.set(
                        std::str::from_utf8(k)?,
                        std::str::from_utf8(v)?
                    )?;
                }
                Ok(Value::Table(query))
            }
            "params" => {
                if req_data.params.is_empty() {
                    return lua.create_table();
                }
                let params = lua.create_table_with_capacity(0, req_data.params.len())?;
                for (k, v) in &req_data.params {
                    params.set(k.as_str(), v.as_str())?;
                }
                Ok(Value::Table(params))
            }
            "body" => {
                if let Some(ref body) = req_data.body {
                    let body_str = std::str::from_utf8(body)?;
                    Ok(Value::String(lua.create_string(body_str)?))
                } else {
                    Err(mlua::Error::RuntimeError("Request has no body".to_string()))
                }
            }
            _ => Ok(Value::Nil)
        }
    })?)?;

    // Store data and set metatable
    ctx.raw_set("__req_data", req_data)?;
    ctx.set_metatable(Some(metatable));

    Ok(ctx)
}
```

**Pros**:
- Only creates data when accessed
- Simpler than closure approach

**Cons**:
- Creates tables on every access (not cached)
- Could cache results in ctx table

### Solution B: Cached Lazy Loading

```rust
fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("method", std::str::from_utf8(&task.method)?)?;
    ctx.set("path", std::str::from_utf8(&task.path)?)?;

    let req_data = Arc::new(RequestData { /* ... */ });
    ctx.raw_set("__req_data", req_data)?;

    // Metatable with caching __index
    let metatable = lua.create_table()?;
    metatable.set("__index", lua.create_function(|lua, (table, key): (Table, String)| {
        // Check cache first
        if let Ok(cached) = table.raw_get::<_, Value>(&key) {
            if !cached.is_nil() {
                return Ok(cached);
            }
        }

        let req_data: Arc<RequestData> = table.raw_get("__req_data")?;

        let value = match key.as_str() {
            "headers" => {
                let headers = create_headers_table(lua, &req_data.headers)?;
                Value::Table(headers)
            }
            // ... other fields ...
            _ => Value::Nil
        };

        // Cache the result
        table.raw_set(&key, value.clone())?;
        Ok(value)
    })?)?;

    ctx.set_metatable(Some(metatable));
    Ok(ctx)
}
```

**Expected Impact**: 15-25% improvement for typical handlers

---

## 3. Eliminate Channel Architecture

### Current Two-Channel Design

**File**: `rover-server/src/lib.rs:247-259`

```rust
let (resp_tx, resp_rx) = oneshot::channel();  // Channel 1

tx.send(LuaRequest {                          // Channel 2 (mpsc)
    /* ... */
    respond_to: resp_tx,
}).await.unwrap();

let resp = resp_rx.await.unwrap();            // Wait on Channel 1
```

### Proposed: Direct Execution Pool

```rust
// New structure: LuaPool
pub struct LuaPool {
    workers: Vec<LuaWorker>,
    counter: AtomicUsize,
}

struct LuaWorker {
    lua: Mutex<Lua>,  // Or use thread-local storage
    router: Arc<FastRouter>,
}

impl LuaPool {
    pub async fn execute(&self, request: LuaRequest) -> HttpResponse {
        let worker_id = self.counter.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        let worker = &self.workers[worker_id];

        // Direct execution - no channels!
        let lua = worker.lua.lock().await;

        let method = HttpMethod::from_str(/* ... */)?;
        let (handler, params) = worker.router.match_route(method, path)?;

        let task = HttpTask { /* ... */ };
        task.execute_sync(&lua)  // Synchronous execution
    }
}
```

**Handler changes**:

```rust
async fn handler(
    req: Request<hyper::body::Incoming>,
    pool: Arc<LuaPool>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // Parse request
    let lua_req = LuaRequest { /* ... */ };

    // Direct execution - no channels
    let http_resp = pool.execute(lua_req).await;

    // Build response
    let mut response = Response::new(Full::new(http_resp.body));
    *response.status_mut() = http_resp.status.into();
    Ok(response)
}
```

**Expected Impact**: 20-30% latency reduction

---

## 4. Lazy Body Collection

### Current: Eager Collection

**File**: `rover-server/src/lib.rs:235-243`

```rust
// Always collects body, even for GET requests
let body_bytes = http_body_util::BodyExt::collect(body_stream)
    .await
    .unwrap()
    .to_bytes();
```

### Proposed: Lazy Collection

```rust
pub enum BodyState {
    Stream(hyper::body::Incoming),
    Collected(Bytes),
}

pub struct LuaRequest {
    // ... existing fields ...
    pub body: Option<BodyState>,  // Changed type
}

// In handler
let body = if parts.method != hyper::Method::GET {
    Some(BodyState::Stream(body_stream))
} else {
    None
};
```

**Context body() method changes**:

```rust
let req_data_body = req_data.clone();
let body_fn = lua.create_function(move |lua, ()| {
    let mut body_state = req_data_body.body.lock().unwrap();  // Need Mutex

    match body_state.as_mut() {
        Some(BodyState::Collected(bytes)) => {
            // Already collected
            let body_str = std::str::from_utf8(bytes)?;
            Ok(Value::String(lua.create_string(body_str)?))
        }
        Some(BodyState::Stream(stream)) => {
            // Collect now (requires async)
            // PROBLEM: Can't await in sync function
            Err(mlua::Error::RuntimeError("Body collection requires async".into()))
        }
        None => {
            Err(mlua::Error::RuntimeError("Request has no body".into()))
        }
    }
})?;
```

**Challenge**: Lua functions are sync, body collection is async

**Solution**: Pre-collect for POST/PUT/PATCH, skip for GET/DELETE

```rust
let body = match parts.method {
    Method::POST | Method::PUT | Method::PATCH => {
        let bytes = http_body_util::BodyExt::collect(body_stream)
            .await
            .unwrap()
            .to_bytes();
        if !bytes.is_empty() {
            Some(bytes)
        } else {
            None
        }
    }
    _ => None,  // Skip collection for GET/DELETE
};
```

**Expected Impact**: 10-15% for GET-heavy workloads

---

## 5. Reduce String Allocations

### Current Allocations

**File**: `rover-server/src/lib.rs:217-231`

```rust
let headers: SmallVec<[(Bytes, Bytes); 8]> = parts
    .headers
    .iter()
    .filter_map(|(k, v)| {
        v.to_str().ok().map(|v_str| {
            (
                Bytes::from(k.as_str().to_string()),  // ALLOCATION
                Bytes::from(v_str.to_string()),       // ALLOCATION
            )
        })
    })
    .collect();
```

### Optimization: Pre-allocate Capacity

```rust
let headers: SmallVec<[(Bytes, Bytes); 8]> = if parts.headers.is_empty() {
    SmallVec::new()
} else {
    let mut headers = SmallVec::with_capacity(parts.headers.len().min(8));

    for (k, v) in parts.headers.iter() {
        if let Ok(v_str) = v.to_str() {
            // Avoid intermediate String allocation
            let k_bytes = Bytes::copy_from_slice(k.as_str().as_bytes());
            let v_bytes = Bytes::copy_from_slice(v_str.as_bytes());
            headers.push((k_bytes, v_bytes));
        }
    }

    headers
};
```

### Further: Intern Common Headers

```rust
// At startup, create static Bytes for common headers
lazy_static! {
    static ref COMMON_HEADERS: HashMap<&'static str, Bytes> = {
        let mut map = HashMap::new();
        map.insert("content-type", Bytes::from_static(b"content-type"));
        map.insert("accept", Bytes::from_static(b"accept"));
        map.insert("user-agent", Bytes::from_static(b"user-agent"));
        // ... more common headers ...
        map
    };
}

// Use in header parsing
let k_bytes = COMMON_HEADERS
    .get(k.as_str())
    .cloned()
    .unwrap_or_else(|| Bytes::copy_from_slice(k.as_str().as_bytes()));
```

**Expected Impact**: 5-10%

---

## 6. Router Optimizations

### Current Static Route Lookup

**File**: `rover-server/src/fast_router.rs:66-68`

Already optimal for static routes:

```rust
let path_hash = hash_path(path);
if let Some(&handler_idx) = self.static_routes.get(&(path_hash, method)) {
    return Some((&self.handlers[handler_idx], HashMap::new()));
}
```

### Optimization: Cache Decoded Params

```rust
use lru::LruCache;

pub struct FastRouter {
    router: Router<SmallVec<[(HttpMethod, usize); 2]>>,
    handlers: Vec<Function>,
    static_routes: HashMap<(u64, HttpMethod), usize>,
    param_cache: Mutex<LruCache<(u64, HttpMethod), HashMap<String, String>>>,  // NEW
}

pub fn match_route(&self, method: HttpMethod, path: &str) -> Option<(&Function, HashMap<String, String>)> {
    // Static route fast path
    let path_hash = hash_path(path);
    if let Some(&handler_idx) = self.static_routes.get(&(path_hash, method)) {
        return Some((&self.handlers[handler_idx], HashMap::new()));
    }

    // Check param cache
    if let Ok(mut cache) = self.param_cache.lock() {
        if let Some(params) = cache.get(&(path_hash, method)) {
            let handler_idx = /* ... */;
            return Some((&self.handlers[handler_idx], params.clone()));
        }
    }

    // ... existing matchit logic ...

    // Cache the result
    if let Ok(mut cache) = self.param_cache.lock() {
        cache.put((path_hash, method), params.clone());
    }

    Some((handler, params))
}
```

**Trade-off**: Memory usage vs CPU time. Use for high-traffic routes.

**Expected Impact**: 2-5%

---

## Benchmarking Each Optimization

### Test Script

```bash
#!/bin/bash

BASELINE="baseline_results.txt"
CURRENT="current_results.txt"

# Run benchmark
wrk -t4 -c1000 -d30s --latency -s tests/perf/benchmark.lua http://localhost:3000/echo > "$CURRENT"

# Extract key metrics
grep "Requests/sec" "$CURRENT"
grep "Latency" "$CURRENT"

# Compare with baseline (if exists)
if [ -f "$BASELINE" ]; then
    echo "=== COMPARISON WITH BASELINE ==="

    baseline_rps=$(grep "Requests/sec" "$BASELINE" | awk '{print $2}')
    current_rps=$(grep "Requests/sec" "$CURRENT" | awk '{print $2}')

    improvement=$(echo "scale=2; ($current_rps - $baseline_rps) / $baseline_rps * 100" | bc)
    echo "Throughput improvement: ${improvement}%"
fi
```

### Metrics to Track

For each optimization, measure:

1. **Throughput**: requests/sec
2. **Latency**: P50, P95, P99, P99.9
3. **Memory**: RSS, heap allocations
4. **CPU**: user time, system time

```bash
# Memory profiling
/usr/bin/time -v ./target/release/rover tests/perf/main.lua

# CPU profiling
perf record -F 99 -g ./target/release/rover tests/perf/main.lua
perf report
```

---

## Implementation Order

### Week 1: Foundation
1. Multi-threaded Lua workers (biggest impact)
2. Benchmark and validate

### Week 2: Context Optimization
3. Lazy context creation with metatable
4. Benchmark and validate

### Week 3: Architecture
5. Eliminate channel overhead
6. Benchmark and validate

### Week 4: Polish
7. Reduce string allocations
8. Lazy body collection for GET requests
9. Final benchmarks

---

## Expected Final Results

After all optimizations:

| Metric | Current | Target | Improvement |
|--------|---------|--------|-------------|
| Throughput (1000 conn) | 157k req/s | 220k req/s | +40% |
| P50 Latency | 6.33ms | 4.5ms | -29% |
| P99 Latency | 7.17ms | 5.2ms | -27% |
| Mean Latency | 6.34ms | 4.8ms | -24% |

This would establish Rover as one of the fastest HTTP frameworks across all languages.
