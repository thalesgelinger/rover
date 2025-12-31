# Rover Performance Analysis & Optimization Recommendations

## Benchmark Results Summary

Based on the December 30, 2025 benchmark comparison:

### Current Performance
- **Throughput**: 152k-157k req/sec (best among tested frameworks)
- **Latency P50**: 6.33ms @ 1000 connections
- **Latency P99**: 7.17ms @ 1000 connections
- **Mean Latency**: 6.34ms @ 1000 connections

### Competitive Position
Rover outperforms Bun, Deno, and Go in throughput by ~30-70%, with competitive latency.

---

## Architecture Analysis

### Current Request Flow
1. TCP accept → Spawn connection task
2. HTTP request parsing (Hyper)
3. Header/query/body collection
4. Send to event loop via mpsc channel (1024 buffer)
5. Router matching (FastRouter with static route optimization)
6. Build Lua context (create tables + 4 closures)
7. Execute Lua handler (async FFI call)
8. Convert response & send via oneshot channel
9. Return HTTP response

### Existing Optimizations ✅
1. **Hyper HTTP library** - Industry-leading performance
2. **SmallVec** - Stack allocations for small collections (8 items)
3. **Custom JSON serializer** - Zero-copy, direct buffer writes
4. **Static route HashMap** - O(1) lookup for routes without params
5. **LTO + aggressive compiler optimizations** - Release profile maximized
6. **Arc-based request data sharing** - Single ref-count vs 4x clones

---

## Identified Performance Bottlenecks

### 1. **Channel-Based Architecture** 🔴 HIGH IMPACT
**Location**: `rover-server/src/lib.rs:158, 247-257`

**Issue**: Every request goes through:
- mpsc::send() to event loop
- oneshot channel creation
- oneshot::recv() for response
- Context switches between tasks

**Impact**: Adds ~2-3ms latency overhead

**Optimization**:
```rust
// Current: Channel-based
tx.send(LuaRequest { ... }).await  // Context switch
let resp = resp_rx.await           // Another context switch

// Proposed: Direct execution pool
lua_pool.execute(request).await    // No channel overhead
```

**Implementation Strategy**:
- Replace single event loop with pool of Lua VMs
- Use work-stealing scheduler or round-robin
- Eliminate mpsc/oneshot channels for fast path
- Potential improvement: **20-30% latency reduction**

---

### 2. **Lua Context Creation Overhead** 🔴 HIGH IMPACT
**Location**: `rover-server/src/http_task.rs:159-256`

**Issue**: For every request, creates:
- 1 new Lua table (ctx)
- 4 Lua closures (headers, query, params, body)
- 4 Arc clones for shared data
- All closures created even if unused

**Current Code**:
```rust
let ctx = lua.create_table()?;
let headers_fn = lua.create_function(move |lua, ()| { ... })?;
let query_fn = lua.create_function(move |lua, ()| { ... })?;
let params_fn = lua.create_function(move |lua, ()| { ... })?;
let body_fn = lua.create_function(move |lua, ()| { ... })?;
```

**Optimizations**:

#### Option A: Lazy Context Creation
```rust
// Only create closures when accessed via __index metamethod
let ctx = lua.create_table()?;
ctx.set_metatable(lazy_access_metatable)?;
// Closures created on-demand
```
**Potential improvement**: 15-20% for handlers that don't access all fields

#### Option B: Pre-registered Functions
```rust
// Register context functions once at startup
lua.globals().set("__ctx_headers", headers_fn)?;

// In handler, reuse pre-registered functions
ctx.set_readonly("headers", call_with_data)?;
```
**Potential improvement**: 10-15%

#### Option C: LuaJIT FFI Direct Access
```rust
// Use LuaJIT FFI to access Rust data directly without closures
// Requires LuaJIT-specific implementation
```
**Potential improvement**: 25-35%

---

### 3. **String Allocations** 🟡 MEDIUM IMPACT
**Location**: `rover-server/src/lib.rs:220-231`

**Issue**:
```rust
// Every header key/value converted to String
Bytes::from(k.as_str().to_string())  // Allocation
Bytes::from(v_str.to_string())       // Allocation

// Query parameters
Bytes::from(k.into_owned())          // Allocation
Bytes::from(v.into_owned())          // Allocation
```

**Optimization**:
```rust
// Use Bytes::from_static() or copy_from_slice() for known headers
// Pre-allocate SmallVec capacity based on average request size
let mut headers: SmallVec<[(Bytes, Bytes); 8]> =
    SmallVec::with_capacity(parts.headers.len());
```

**Potential improvement**: 5-10%

---

### 4. **Body Collection Strategy** 🟡 MEDIUM IMPACT
**Location**: `rover-server/src/lib.rs:235-243`

**Issue**:
```rust
// Always collects entire body even if handler doesn't use it
let body_bytes = http_body_util::BodyExt::collect(body_stream)
    .await
    .unwrap()
    .to_bytes();
```

**Optimization**:
- Implement lazy body collection
- Only collect body when `ctx:body()` is called
- Requires architectural change to pass body_stream to event loop

**Potential improvement**: 10-15% for GET-heavy workloads

---

### 5. **Single-Threaded Event Loop** 🟡 MEDIUM IMPACT
**Location**: `rover-server/src/event_loop.rs:22`

**Issue**:
```rust
tokio::spawn(async move {
    // Single task processes ALL Lua requests
    while let Some(req) = rx.recv().await {
        task.execute(&lua).await;  // Sequential
    }
});
```

**Optimization**:
```rust
// Spawn multiple Lua worker tasks
for _ in 0..num_cpus::get() {
    let lua = create_lua_vm();
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            task.execute(&lua).await;
        }
    });
}
```

**Potential improvement**: 40-60% throughput increase under high concurrency

---

### 6. **Router Matching Overhead** 🟢 LOW IMPACT
**Location**: `rover-server/src/fast_router.rs:61-91`

**Issue**:
- Static routes use HashMap (optimal)
- Dynamic routes use matchit + URL decoding on every request

**Current Optimization** (already good):
```rust
// Fast path for static routes
if let Some(&handler_idx) = self.static_routes.get(&(path_hash, method)) {
    return Some((&self.handlers[handler_idx], HashMap::new()));
}
```

**Further Optimization**:
- Cache decoded URL parameters (if same path pattern repeats)
- Use faster URL decoder (currently using `urlencoding` crate)

**Potential improvement**: 2-5%

---

### 7. **Lua FFI Boundary Crossing** 🟡 MEDIUM IMPACT
**Location**: `rover-server/src/http_task.rs:76`

**Issue**:
```rust
let result: Value = match self.handler.call_async(ctx).await {
    // FFI overhead for every call
}
```

**Optimization**:
- Use LuaJIT FFI for direct function calls
- Pre-compile Lua handlers to bytecode at startup
- Consider JIT warmup period

**Potential improvement**: 5-10%

---

## Recommended Implementation Priority

### Phase 1: High-Impact, Low-Risk (Target: 30-40% improvement)
1. **Multi-threaded Lua workers** (rover-server/src/event_loop.rs)
   - Spawn N worker tasks (N = CPU cores)
   - Each with own Lua VM
   - Share mpsc receiver or use multiple channels

2. **Lazy context creation** (rover-server/src/http_task.rs)
   - Implement `__index` metamethod for on-demand field access
   - Only create closures when handler accesses ctx fields

### Phase 2: Medium-Impact Optimizations (Target: 15-20% improvement)
3. **Eliminate channel architecture** (rover-server/src/lib.rs)
   - Direct Lua pool execution
   - Remove oneshot/mpsc overhead

4. **Lazy body collection** (rover-server/src/lib.rs)
   - Pass body stream to handler
   - Only collect when `ctx:body()` called

### Phase 3: Fine-Tuning (Target: 5-10% improvement)
5. **Reduce string allocations** (rover-server/src/lib.rs)
   - Pre-allocate with capacity
   - Reuse buffers where possible

6. **LuaJIT optimizations** (rover-server/src/http_task.rs)
   - FFI direct calls
   - Bytecode pre-compilation

---

## Benchmarking Strategy

After each optimization:

```bash
# Run benchmark suite
cd tests/perf && bash run_benchmark.sh

# Compare with baseline
# - Throughput (req/sec)
# - P50/P99 latency
# - Memory usage
# - CPU utilization
```

Expected results after all optimizations:
- **Throughput**: 200k-250k req/sec (+30-60%)
- **P50 Latency**: 4-5ms (-20-30%)
- **P99 Latency**: 5-6ms (-20-30%)

---

## Additional Observations

### Why Rover Already Performs Well

1. **Rust + Hyper foundation** - Optimal HTTP handling
2. **Custom JSON serializer** - Faster than serde_json for simple cases
3. **SmallVec usage** - Avoids heap allocations for common cases
4. **Static route optimization** - O(1) lookup for most common routes
5. **Aggressive compiler optimizations** - LTO, single codegen-unit

### Areas Where Competitors May Excel

- **Bun**: JIT-optimized JavaScript execution, but slower HTTP layer
- **Deno**: V8 optimization, but Rust-JS boundary overhead
- **Go**: Excellent concurrency model, but GC pauses affect tail latency

### Rover's Competitive Advantage

The combination of:
1. Rust's zero-cost abstractions
2. LuaJIT's fast FFI and JIT compilation
3. Hyper's HTTP performance
4. Custom optimizations (JSON, routing)

Creates a unique performance profile that can be further enhanced with the recommended optimizations.

---

## Conclusion

Rover is already performing excellently, beating established frameworks. The identified optimizations focus on:

1. **Reducing overhead** (channels, allocations)
2. **Improving concurrency** (multi-threaded Lua workers)
3. **Lazy evaluation** (context creation, body collection)

Implementing these changes could push Rover to **200k+ req/sec** with **sub-5ms P99 latency**, establishing it as one of the fastest HTTP frameworks across any language.
