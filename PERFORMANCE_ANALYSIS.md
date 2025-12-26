# Rover Server Performance Analysis & Optimization Recommendations

**Current Performance:** 182,000 req/s | 0.49ms avg latency | 0.67ms p99

## Executive Summary

Rover has solid performance fundamentals, but there are **significant opportunities** to improve throughput and reduce latency. The analysis identifies 8 key optimization areas, with the **single-threaded event loop** being the #1 bottleneck preventing you from reaching Bun-level performance.

**Potential Impact:** With the recommended changes, you could achieve **300k-500k+ req/s** (2-3x improvement).

---

## 🔴 CRITICAL: Single-Threaded Event Loop Bottleneck

**File:** `rover_server/src/event_loop.rs:12-183`

### The Problem

```rust
pub fn run(lua: Lua, routes: Vec<Route>, mut rx: Receiver<LuaRequest>, _config: ServerConfig) {
    tokio::spawn(async move {
        let fast_router = FastRouter::from_routes(routes).expect("Failed to build router");

        while let Some(req) = rx.recv().await {  // ⚠️ Sequential processing
            // ... route matching ...
            let result: Value = match handler.call_async(ctx).await {  // ⚠️ Blocks entire loop
                Ok(r) => r,
                Err(e) => { /* ... */ }
            };
            // ... response handling ...
        }
    });
}
```

**Impact:**
- ❌ Only **1 CPU core** utilized for all Lua execution
- ❌ Long-running handlers (DB queries, external APIs) **block all subsequent requests**
- ❌ Can't leverage modern multi-core CPUs
- ❌ Throughput limited by single-thread execution time

### The Solution: Per-Request Lua States or Worker Pool

**Option A: Spawn Lua State Per Request (Recommended)**
```rust
while let Some(req) = rx.recv().await {
    let lua = Lua::new();  // Create new Lua state
    let handler = handler.clone();

    tokio::spawn(async move {
        // Execute handler in parallel
        let result = handler.call_async(ctx).await;
        // Send response back
    });
}
```

**Pros:**
- ✅ Complete isolation between requests
- ✅ Utilizes all CPU cores
- ✅ Simple to implement
- ✅ No blocking between requests

**Cons:**
- ⚠️ Lua state creation overhead (~5-10µs per request)
- ⚠️ Can't share global state easily

**Option B: Worker Pool with Multiple Lua States**
```rust
// Create pool of N Lua states (e.g., N = num_cpus)
let pool = create_lua_worker_pool(num_cpus::get());

while let Some(req) = rx.recv().await {
    let worker = pool.get_available_worker().await;
    tokio::spawn(async move {
        worker.execute(handler, ctx).await;
        pool.return_worker(worker);
    });
}
```

**Pros:**
- ✅ Reuses Lua states (no creation overhead)
- ✅ Utilizes all CPU cores
- ✅ Better for shared state scenarios

**Cons:**
- ⚠️ More complex implementation
- ⚠️ Need worker availability management

**Expected Impact:** **2-4x throughput improvement** (400k-700k req/s)

---

## 🟡 HIGH PRIORITY: JSON Serialization Inefficiencies

**File:** `rover_server/src/to_json.rs:27-61`

### The Problem

```rust
fn detect_and_collect(table: &Table) -> mlua::Result<(TableType, Vec<(Value, Value)>)> {
    let mut pairs = Vec::new();  // ⚠️ Allocates Vec
    let mut max_index = 0;
    let mut has_sequential = true;
    let mut count = 0;

    for pair in table.pairs::<Value, Value>() {  // ⚠️ Iterates entire table
        let (key, value) = pair?;
        count += 1;
        // ... type detection logic ...
        pairs.push((key, value));  // ⚠️ Stores ALL pairs in memory
    }
    // ...
}
```

**Issues:**
1. **Double iteration:** First iteration to detect type, second to serialize
2. **Unnecessary allocation:** Stores all pairs in Vec before serialization
3. **Sequential detection:** Can't determine type without scanning all keys

### The Solution: Stream-Based Serialization with Lookahead

**Optimization 1: Use Lua Table Length Hint**
```rust
fn detect_table_type(table: &Table) -> mlua::Result<TableType> {
    let raw_len = table.raw_len();  // O(1) operation in Lua

    // If raw_len > 0, it's likely an array
    if raw_len > 0 {
        // Quick check: verify keys are 1..n
        for i in 1..=raw_len {
            if table.raw_get::<_, Value>(i)?.is_nil() {
                return Ok(TableType::Object);  // Has gaps, treat as object
            }
        }
        return Ok(TableType::Array { len: raw_len });
    }

    // Otherwise, it's an object
    Ok(TableType::Object)
}

fn serialize_table(table: &Table, buf: &mut Vec<u8>, depth: usize) -> mlua::Result<()> {
    let table_type = detect_table_type(table)?;  // Fast detection

    match table_type {
        TableType::Array { len } => {
            buf.push(b'[');
            for i in 1..=len {
                if i > 1 { buf.push(b','); }
                serialize_value(&table.get(i)?, buf, depth + 1)?;
            }
            buf.push(b']');
        }
        TableType::Object => {
            buf.push(b'{');
            let mut first = true;
            for pair in table.pairs::<Value, Value>() {  // Single iteration
                let (key, value) = pair?;
                if !first { buf.push(b','); }
                first = false;
                // ... serialize key-value ...
            }
            buf.push(b'}');
        }
    }
    Ok(())
}
```

**Expected Impact:** **20-30% faster JSON serialization**

---

## 🟡 HIGH PRIORITY: Context Data Cloning

**File:** `rover_server/src/event_loop.rs:200-203`

### The Problem

```rust
let headers_clone = req.headers.clone();  // ⚠️ Clone SmallVec
let query_clone = req.query.clone();      // ⚠️ Clone SmallVec
let params_clone = params.clone();        // ⚠️ Clone HashMap
let body_clone = req.body.clone();        // ⚠️ Clone Option<Bytes>

let headers_fn = lua.create_function(move |lua, ()| {
    // Uses cloned data
})?;
```

**Issues:**
- Creates 4 clones per request for closure captures
- While `Bytes` is `Arc`-based (cheap), `SmallVec` and `HashMap` involve allocation
- Clones happen **even if user never calls** `ctx:headers()` or `ctx:query()`

### The Solution: Lazy Evaluation with Arc

```rust
use std::sync::Arc;

// Wrap request data in Arc once
let req_data = Arc::new(RequestData {
    headers: req.headers,
    query: req.query,
    params: params,
    body: req.body,
});

// Clone only the Arc (cheap pointer increment)
let headers_data = Arc::clone(&req_data);
let headers_fn = lua.create_function(move |lua, ()| {
    if headers_data.headers.is_empty() {
        return lua.create_table();
    }
    let headers = lua.create_table_with_capacity(0, headers_data.headers.len())?;
    for (k, v) in &headers_data.headers {
        // ... convert to Lua table ...
    }
    Ok(headers)
})?;
```

**Expected Impact:** **5-10% latency reduction**, especially for requests with many headers/query params

---

## 🟡 MEDIUM PRIORITY: Validation String Allocations

**File:** `rover_core/src/guard.rs:294-316`

### The Problem

```rust
fn validate_table_internal(...) -> Result<Value, Vec<ValidationError>> {
    let pairs_vec: Vec<(String, Table)> = schema  // ⚠️ Collects all pairs
        .pairs()
        .collect::<Result<Vec<_>, _>>()?;

    for (field_name, validator_config) in pairs_vec {
        let full_field_name = if context.is_empty() {
            field_name.clone()  // ⚠️ String allocation
        } else {
            format!("{}.{}", context, field_name)  // ⚠️ String allocation
        };
        // ...
    }
}
```

**Issues:**
1. **Collects all schema pairs** into Vec (extra allocation)
2. **Path string allocations** for every field: `format!("{}.{}", context, field_name)`
3. **Error message formatting** creates strings eagerly

### The Solution: Stream Processing + String Interning

```rust
fn validate_table_internal(...) -> Result<Value, Vec<ValidationError>> {
    let result = lua.create_table()?;
    let mut all_errors = Vec::new();

    // Direct iteration - no Vec collection
    for pair in schema.pairs::<String, Table>() {
        let (field_name, validator_config) = pair?;

        // Use stack-allocated buffer for common case
        let full_field_name = if context.is_empty() {
            field_name.as_str()  // No allocation
        } else {
            // Could use SmallString or string builder
            format!("{}.{}", context, field_name)
        };

        // ... validation logic ...
    }
}
```

**Alternative: Cow Strings**
```rust
use std::borrow::Cow;

fn validate_field<'a>(
    lua: &Lua,
    field_name: Cow<'a, str>,  // Can be borrowed or owned
    value: Value,
    config: &Table,
) -> Result<Value, Vec<ValidationError>> {
    // Only allocate when building error messages (rare case)
}
```

**Expected Impact:** **10-15% faster validation**, especially for nested objects

---

## 🟢 MEDIUM PRIORITY: Route Parameter Decoding

**File:** `rover_server/src/fast_router.rs:81-88`

### The Problem

```rust
let mut params = HashMap::with_capacity(matched.params.len());
for (name, value) in matched.params.iter() {
    let decoded = urlencoding::decode(value).ok()?.into_owned();  // ⚠️ Allocates String
    if decoded.is_empty() {
        return None;
    }
    params.insert(name.to_string(), decoded);  // ⚠️ Another allocation
}
```

**Issues:**
- URL decoding creates new String (allocation)
- `name.to_string()` allocates for HashMap key
- Happens **on every dynamic route request**

### The Solution: Decode Only If Needed + String Interning

```rust
// Option 1: Lazy decoding in Lua
let params_fn = lua.create_function(move |lua, ()| {
    let params_table = lua.create_table_with_capacity(0, params_clone.len())?;
    for (k, v) in &params_clone {
        // Decode only when accessed
        let decoded = urlencoding::decode(v).unwrap_or(Cow::Borrowed(v));
        params_table.set(k.as_str(), decoded.as_ref())?;
    }
    Ok(params_table)
})?;

// Option 2: Cache decoded params if same route pattern repeats
// (useful if same user ID accessed frequently)
```

**Expected Impact:** **2-5% improvement** for dynamic routes

---

## 🟢 LOW PRIORITY: Request Body Parsing

**File:** `rover_server/src/lib.rs:242-250`

### Current Implementation

```rust
let body_bytes = http_body_util::BodyExt::collect(body_stream)
    .await
    .unwrap()  // ⚠️ Panic on error
    .to_bytes();
let body = if !body_bytes.is_empty() {
    Some(body_bytes)
} else {
    None
};
```

**Issues:**
- Always collects body, even for GET requests
- `.unwrap()` can panic (should be handled gracefully)

### The Solution: Conditional Body Reading

```rust
let body = if parts.method == hyper::Method::GET
            || parts.method == hyper::Method::HEAD {
    None  // Skip body collection for GET/HEAD
} else {
    let body_bytes = http_body_util::BodyExt::collect(body_stream)
        .await
        .map_err(|e| /* log error */)?
        .to_bytes();

    if !body_bytes.is_empty() {
        Some(body_bytes)
    } else {
        None
    }
};
```

**Expected Impact:** **5-10% improvement** for GET requests

---

## 🟢 LOW PRIORITY: SmallVec Capacity Tuning

**File:** `rover_server/src/lib.rs:213-240`

### Current Implementation

```rust
let headers: SmallVec<[(Bytes, Bytes); 8]> = ...;  // Inline capacity: 8
let query: SmallVec<[(Bytes, Bytes); 8]> = ...;    // Inline capacity: 8
```

**Analysis:**
- Most HTTP requests have **2-4 headers** (Host, User-Agent, Accept, Content-Type)
- Most query strings have **0-2 params**
- Current capacity (8) is reasonable but could be optimized

### Optimization: Profile-Guided Tuning

```rust
// Option 1: Reduce header capacity if most requests have few headers
let headers: SmallVec<[(Bytes, Bytes); 4]> = ...;  // Saves 64 bytes per request

// Option 2: Increase if your app has many headers
let headers: SmallVec<[(Bytes, Bytes); 16]> = ...;  // Avoids heap allocation
```

**Recommendation:** Profile your production traffic to determine optimal capacity.

**Expected Impact:** **1-3% improvement** (minor)

---

## 🟢 OPTIMIZATION: Pre-allocate Response Buffers

**File:** `rover_server/src/to_json.rs:9`

### Current Implementation

```rust
fn to_json_string(&self) -> mlua::Result<String> {
    let mut buf = Vec::with_capacity(256);  // Fixed 256-byte capacity
    self.to_json(&mut buf)?;
    Ok(unsafe { String::from_utf8_unchecked(buf) })
}
```

**Issues:**
- 256 bytes might be too small for large responses (reallocation needed)
- Might be too large for small responses (wasted memory)

### The Solution: Adaptive Buffer Sizing

```rust
// Option 1: Provide size hint from caller
fn to_json_string_with_capacity(&self, capacity: usize) -> mlua::Result<String> {
    let mut buf = Vec::with_capacity(capacity);
    self.to_json(&mut buf)?;
    Ok(unsafe { String::from_utf8_unchecked(buf) })
}

// Option 2: Pool pre-allocated buffers
thread_local! {
    static BUFFER_POOL: RefCell<Vec<Vec<u8>>> = RefCell::new(Vec::new());
}

fn to_json_string(&self) -> mlua::Result<String> {
    let mut buf = BUFFER_POOL.with(|pool| {
        pool.borrow_mut().pop().unwrap_or_else(|| Vec::with_capacity(512))
    });

    buf.clear();
    self.to_json(&mut buf)?;

    let result = unsafe { String::from_utf8_unchecked(buf.clone()) };

    BUFFER_POOL.with(|pool| {
        if buf.capacity() <= 4096 {  // Don't pool huge buffers
            pool.borrow_mut().push(buf);
        }
    });

    Ok(result)
}
```

**Expected Impact:** **3-5% improvement** for large JSON responses

---

## 📊 Summary: Prioritized Action Plan

| Priority | Optimization | File(s) | Expected Impact | Effort |
|----------|-------------|---------|-----------------|--------|
| 🔴 **1** | **Multi-threaded Lua execution** | `event_loop.rs` | **2-4x throughput** | High |
| 🟡 **2** | JSON serialization optimization | `to_json.rs` | 20-30% faster JSON | Medium |
| 🟡 **3** | Reduce context data cloning | `event_loop.rs` | 5-10% latency ↓ | Low |
| 🟡 **4** | Validation string optimizations | `guard.rs` | 10-15% faster validation | Medium |
| 🟢 **5** | Route param decoding optimization | `fast_router.rs` | 2-5% | Low |
| 🟢 **6** | Conditional body parsing | `lib.rs` | 5-10% for GET | Low |
| 🟢 **7** | SmallVec capacity tuning | `lib.rs` | 1-3% | Very Low |
| 🟢 **8** | Buffer pooling for JSON | `to_json.rs` | 3-5% | Medium |

---

## 🎯 To Beat Bun: Focus on These

**Bun's Performance Advantages:**
1. JavaScriptCore JIT compilation (Lua has simpler interpreter)
2. Highly optimized HTTP parser (Bun uses custom parser)
3. Multi-threaded request handling ✅ **YOU NEED THIS**
4. Zero-copy string handling
5. SIMD optimizations

**Your Best Opportunities:**

1. **Implement #1 (Multi-threaded Lua)** - This is non-negotiable
2. **Implement #2 (JSON optimization)** - Serialization is a hot path
3. **Consider LuaJIT** - JIT compilation could give 5-10x Lua execution speed
4. **Profile with `perf`** - Find actual bottlenecks in production workloads

---

## 🔬 Recommended Profiling Commands

```bash
# CPU profiling
cargo build --release
perf record -F 999 -g ./target/release/rover your_app.lua
perf report

# Flamegraph
cargo install flamegraph
cargo flamegraph -- your_app.lua

# Memory profiling
valgrind --tool=massif ./target/release/rover your_app.lua
ms_print massif.out.*

# Benchmark
wrk -t4 -c100 -d30s --latency http://localhost:4242/endpoint
```

---

## 💡 Additional Ideas

1. **HTTP/3 support** (QUIC) - Bun doesn't have this yet
2. **Connection pooling** for external services
3. **Response compression** (gzip/brotli) - currently missing
4. **Static file serving optimizations** (sendfile, mmap)
5. **Request batching** for analytics/logging

---

## Conclusion

Your biggest bottleneck is the **single-threaded Lua event loop**. Fixing this alone could **double or triple your throughput**. Combined with the JSON and validation optimizations, you could realistically achieve **400k-600k req/s**, putting you in the same ballpark as Bun for simple JSON endpoints.

**Lua's advantages over JavaScript:**
- ✅ Simpler, more predictable memory usage
- ✅ Smaller runtime footprint
- ✅ Easier to embed and sandbox
- ✅ Better for scripting and glue code

With the right optimizations, **Rover can absolutely compete with Bun's performance** while offering a cleaner, more opinionated developer experience.
