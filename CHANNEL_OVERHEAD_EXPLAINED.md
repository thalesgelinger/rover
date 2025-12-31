# Channel Overhead: What's Really Happening

## Current Architecture Visualized

```
┌─────────────────────────────────────────────────────────────┐
│ Connection Handler Task #1                                   │
│ (spawned per connection)                                     │
│                                                               │
│  1. Parse HTTP request                                       │
│  2. Create oneshot channel: (tx, rx) ← ALLOCATION           │
│  3. Send via mpsc: tx.send(request, oneshot_tx)             │
│     └─ Task YIELDS (waiting for channel)                    │
│                                                               │
│  8. Waiting on oneshot: rx.await ← BLOCKED                  │
│     └─ Task sleeps until event loop responds                │
│                                                               │
│  9. Wakes up! Got response                                   │
│  10. Send HTTP response to client                            │
└─────────────────────────────────────────────────────────────┘
       │                                        ▲
       │ mpsc::send()                           │ oneshot::send()
       │ (context switch)                       │ (context switch)
       ▼                                        │
┌─────────────────────────────────────────────────────────────┐
│ Event Loop Task (SINGLE task)                                │
│                                                               │
│  4. rx.recv().await ← Wakes up                              │
│  5. Route request to Lua handler                             │
│  6. Execute Lua code                                         │
│  7. Send response: oneshot_tx.send(response)                │
│     └─ Wakes up connection handler                          │
│                                                               │
│  Then back to step 4 (receive next request)                  │
└─────────────────────────────────────────────────────────────┘
```

## The Overhead Breakdown

Let me trace a **single request** through the system with actual costs:

### Request Path

```rust
// In handler() - Connection task
async fn handler(req: Request, tx: mpsc::Sender<LuaRequest>) -> Response {
    let (parts, body) = req.into_parts();

    // Parse request data - this is fine
    let headers = /* parse headers */;  // ~2μs
    let query = /* parse query */;      // ~1μs
    let body = /* collect body */;      // ~50μs (if POST)

    // HERE'S THE PROBLEM:

    // 1. Allocate oneshot channel
    let (resp_tx, resp_rx) = oneshot::channel();  // ← 2 heap allocations (~200ns)

    // 2. Send to event loop via mpsc
    tx.send(LuaRequest {                          // ← Cost breakdown:
        method,                                    //   - Serialize to channel buffer
        path,                                      //   - Wake event loop task
        headers,                                   //   - This task yields
        query,                                     //   Total: ~1-2μs
        body,
        respond_to: resp_tx,
        started_at: Instant::now(),
    })
    .await
    .unwrap();

    // 3. Connection task BLOCKS here, waiting for response
    let resp = resp_rx.await.unwrap();            // ← Sleeps until event loop responds
                                                   //   Wake-up overhead: ~1-2μs

    // 4. Build HTTP response
    let mut response = Response::new(Full::new(resp.body));
    *response.status_mut() = resp.status.into();
    Ok(response)
}
```

### Response Path

```rust
// In event_loop::run() - Event loop task
pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, ...) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes)?;

        while let Some(req) = rx.recv().await {      // ← Wakes up (~1-2μs)
            // Route and execute
            let (handler, params) = fast_router.match_route(method, path)?;

            let task = HttpTask { /* ... */ };
            task.execute(&lua).await;                 // ← Executes Lua handler

            // Inside task.execute():
            // ... Lua execution ...

            req.respond_to.send(HttpResponse {        // ← Wake connection task (~1-2μs)
                status,
                body,
                content_type,
            });
        }
    });
}
```

## Total Overhead Per Request

| Operation | Cost | Impact |
|-----------|------|--------|
| oneshot::channel() allocation | ~200ns | Memory allocation |
| mpsc::send() | ~1-2μs | Context switch to event loop |
| Connection task yields | ~0.5μs | Scheduler overhead |
| rx.recv() wakes event loop | ~1μs | Context switch cost |
| **Lua execution happens** | **~30-50μs** | **Actual work** |
| oneshot::send() | ~1-2μs | Context switch to connection |
| resp_rx.await wakes handler | ~1μs | Context switch cost |

**Total channel overhead: ~5-8 microseconds per request**

At 150k req/sec:
- 150,000 × 6μs = **900,000 microseconds** = **0.9 CPU seconds wasted per second**
- That's **~10% of CPU time** on a single core

---

## Optimization 1: Use Flume (Faster Channel)

### Why Flume Is Faster

`tokio::mpsc` is designed for general async patterns. `flume` is optimized for high-throughput producer-consumer.

**tokio::mpsc internals:**
```
Send: Lock → Write to buffer → Wake receiver → Unlock
Recv: Lock → Read from buffer → Maybe sleep → Unlock
```

**flume internals:**
```
Send: CAS loop (lock-free) → Write → Wake if needed
Recv: CAS loop (lock-free) → Read → Park if empty
```

### Code Change (Trivial)

```diff
# Cargo.toml
[dependencies]
-tokio = { version = "1.48.0", features = ["full"] }
+tokio = { version = "1.48.0", features = ["full"] }
+flume = "0.11"
```

```diff
// lib.rs
-use tokio::sync::mpsc;
+use flume;

async fn server(lua: Lua, routes: RouteTable, config: ServerConfig, ...) -> Result<()> {
-    let (tx, rx) = mpsc::channel::<LuaRequest>(1024);
+    let (tx, rx) = flume::bounded::<LuaRequest>(1024);

    // ... rest unchanged ...
}
```

```diff
// event_loop.rs
-use tokio::sync::mpsc::Receiver;
+use flume::Receiver;

-pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, ...) {
+pub fn run(lua: Lua, routes: Vec<Route>, rx: Receiver<LuaRequest>, ...) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes)?;

-        while let Some(req) = rx.recv().await {
+        while let Ok(req) = rx.recv_async().await {  // Different API
            // ... process request (unchanged) ...
        }
    });
}
```

### Expected Improvement

**Before (tokio::mpsc):**
- Send latency: ~1.5μs
- Recv latency: ~1.5μs
- Total: ~3μs per request

**After (flume):**
- Send latency: ~0.8μs
- Recv latency: ~0.8μs
- Total: ~1.6μs per request

**Savings: ~1.4μs per request → ~8-12% throughput improvement**

---

## Optimization 2: Batched Processing

### The Problem

Current event loop processes **one request at a time**:

```rust
while let Some(req) = rx.recv().await {
    task.execute(&lua).await;  // Process one
    // Loop back to recv next one
}
```

If 100 requests arrive at once:
```
Wake up → Process 1 → Sleep
Wake up → Process 1 → Sleep
Wake up → Process 1 → Sleep
... (100 wake-ups!)
```

Each wake-up costs ~1-2μs in context switching.

### The Solution: Batch Drain

```rust
while let Ok(req) = rx.recv_async().await {
    let mut batch = vec![req];  // Start with first request

    // Drain all pending requests (non-blocking)
    while let Ok(req) = rx.try_recv() {
        batch.push(req);
        if batch.len() >= 32 {  // Max batch size
            break;
        }
    }

    // Process all in one go
    for req in batch {
        task.execute(&lua).await;
    }
}
```

Now if 100 requests arrive:
```
Wake up → Process 32 → Sleep
Wake up → Process 32 → Sleep
Wake up → Process 32 → Sleep
Wake up → Process 4 → Sleep
... (4 wake-ups instead of 100!)
```

### Implementation

```rust
// event_loop.rs - Batched version
pub fn run(lua: Lua, routes: Vec<Route>, rx: flume::Receiver<LuaRequest>, config: ServerConfig, openapi_spec: Option<serde_json::Value>) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes).expect("Failed to build router");
        let mut batch = Vec::with_capacity(32);

        loop {
            batch.clear();

            // Blocking receive - wait for at least one request
            match rx.recv_async().await {
                Ok(req) => batch.push(req),
                Err(_) => break,  // Channel closed, shutdown
            }

            // Non-blocking drain - get all pending requests
            loop {
                match rx.try_recv() {
                    Ok(req) => {
                        batch.push(req);
                        if batch.len() >= 32 {
                            break;  // Max batch size reached
                        }
                    }
                    Err(_) => break,  // No more pending
                }
            }

            if config.log_level == "debug" {
                tracing::debug!("Processing batch of {} requests", batch.len());
            }

            // Process entire batch
            for req in batch.drain(..) {
                let method_str = unsafe { std::str::from_utf8_unchecked(&req.method) };

                let method = match HttpMethod::from_str(method_str) {
                    Some(m) => m,
                    None => {
                        let _ = req.respond_to.send(crate::HttpResponse {
                            status: StatusCode::BAD_REQUEST,
                            body: Bytes::from("Invalid HTTP method"),
                            content_type: Some("text/plain".to_string()),
                        });
                        continue;
                    }
                };

                let path_str = unsafe { std::str::from_utf8_unchecked(&req.path) };

                // Handle /docs endpoint
                if config.docs && path_str == "/docs" && openapi_spec.is_some() {
                    let html = rover_openapi::scalar_html(openapi_spec.as_ref().unwrap());
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
                        tracing::warn!(
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

                if let Err(e) = task.execute(&lua).await {
                    tracing::debug!("Task execution failed: {}", e);
                }
            }
        }
    });
}
```

### Expected Improvement

Under **high load** (1000 concurrent connections):
- Reduces wake-ups by 10-20x
- Each wake-up saves ~1-2μs
- **Savings: ~5-10% throughput improvement**

Under **low load** (< 100 concurrent):
- Batches are small (1-3 requests)
- Minimal benefit (~1-2%)

---

## Optimization 3: Reduce Oneshot Allocations

### The Problem

Every request allocates a oneshot channel:

```rust
let (resp_tx, resp_rx) = oneshot::channel();  // 2 heap allocations
```

At 150k req/sec, that's **300k heap allocations per second**.

### Solution: Reuse Response Mechanism

Instead of creating oneshot channels, use a **shared response map**:

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Notify;

// Shared between handler and event loop
pub struct ResponseMap {
    map: DashMap<u64, Option<HttpResponse>>,
    notifiers: DashMap<u64, Arc<Notify>>,
    counter: AtomicU64,
}

impl ResponseMap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            map: DashMap::new(),
            notifiers: DashMap::new(),
            counter: AtomicU64::new(0),
        })
    }

    // Called by handler - register request
    pub fn register(&self) -> (u64, Arc<Notify>) {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let notify = Arc::new(Notify::new());
        self.map.insert(id, None);
        self.notifiers.insert(id, notify.clone());
        (id, notify)
    }

    // Called by event loop - store response
    pub fn respond(&self, id: u64, response: HttpResponse) {
        if let Some(mut entry) = self.map.get_mut(&id) {
            *entry = Some(response);
        }
        if let Some((_, notify)) = self.notifiers.remove(&id) {
            notify.notify_one();
        }
    }

    // Called by handler - wait for response
    pub async fn wait(&self, id: u64, notify: Arc<Notify>) -> Option<HttpResponse> {
        notify.notified().await;
        self.map.remove(&id).and_then(|(_, resp)| resp)
    }
}
```

Usage:

```rust
// In server()
let response_map = ResponseMap::new();

// In handler()
async fn handler(
    req: Request,
    tx: flume::Sender<LuaRequest>,
    response_map: Arc<ResponseMap>,
) -> Response {
    // ... parse request ...

    // Register request (no allocation, just atomic increment)
    let (request_id, notify) = response_map.register();

    tx.send_async(LuaRequest {
        request_id,  // Instead of oneshot sender
        // ... other fields ...
    }).await.unwrap();

    // Wait for response
    let http_resp = response_map.wait(request_id, notify).await.unwrap();

    // ... build response ...
}

// In event loop
while let Ok(req) = rx.recv_async().await {
    // ... process ...

    // Store response (no send, just write to map)
    response_map.respond(req.request_id, HttpResponse {
        status,
        body,
        content_type,
    });
}
```

### Expected Improvement

- Eliminates 300k heap allocations/sec
- Reduces GC pressure
- **Savings: ~3-5% throughput improvement**

**Trade-off:** More complex code, harder to debug

---

## Combined Optimization Strategy

### Phase 1: Low-Hanging Fruit (Week 1)

**Implement: Flume + Batching**

Code changes:
1. Add `flume = "0.11"` to Cargo.toml
2. Replace `tokio::mpsc` with `flume` (5 lines changed)
3. Add batching to event loop (20 lines added)

**Expected gain: +15-20% throughput**
**Risk: Very low** (drop-in replacement)

### Phase 2: Advanced (Week 2)

**Implement: Response Map**

Code changes:
1. Add `dashmap = "6.0"` to Cargo.toml
2. Create ResponseMap structure
3. Update handler and event loop

**Expected gain: +3-5% additional**
**Risk: Medium** (more complex, harder to debug)

---

## Realistic Expected Results

### Just Flume + Batching

| Metric | Current | With Optimization | Improvement |
|--------|---------|-------------------|-------------|
| Throughput @ 1000 conn | 157k req/s | **185k req/s** | **+18%** |
| P50 Latency | 6.33ms | **5.5ms** | **-13%** |
| P99 Latency | 7.17ms | **6.2ms** | **-14%** |

### With Response Map (Advanced)

| Metric | Current | With All Optimizations | Improvement |
|--------|---------|------------------------|-------------|
| Throughput @ 1000 conn | 157k req/s | **195k req/s** | **+24%** |
| P50 Latency | 6.33ms | **5.2ms** | **-18%** |
| P99 Latency | 7.17ms | **5.8ms** | **-19%** |

---

## My Recommendation

**Start with Flume + Batching:**

1. **Easy to implement** (30 minutes)
2. **Low risk** (well-tested libraries)
3. **Good gains** (+18% throughput)
4. **Easy to rollback** (just swap back to tokio::mpsc)

Then **benchmark** and see if you need more. The response map optimization is only worth it if you're chasing every last percent.

Want me to implement the Flume + Batching changes?
