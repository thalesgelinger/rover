# Lazy Context: Reality Check

You asked a critical question: **Are we 100% sure lazy context will help?**

Honest answer: **No, we need to measure it first.** Here's why I have doubts:

---

## The Theory (Why It Should Help)

Current code creates 4 closures per request:

```rust
fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    // Simple fields - cheap
    ctx.set("method", std::str::from_utf8(&task.method)?)?;
    ctx.set("path", std::str::from_utf8(&task.path)?)?;

    // Expensive? Creates 4 closures regardless of usage
    let headers_fn = lua.create_function(move |lua, ()| { ... })?;  // Closure 1
    let query_fn = lua.create_function(move |lua, ()| { ... })?;    // Closure 2
    let params_fn = lua.create_function(move |lua, ()| { ... })?;   // Closure 3
    let body_fn = lua.create_function(move |lua, ()| { ... })?;     // Closure 4

    ctx.set("headers", headers_fn)?;
    ctx.set("query", query_fn)?;
    ctx.set("params", params_fn)?;
    ctx.set("body", body_fn)?;

    Ok(ctx)
}
```

**If** creating Lua closures is expensive (~5μs each), and **if** handlers only use 1-2 fields, then lazy creation saves 15-20μs per request.

At 150k req/sec × 15μs = **2.25 CPU seconds wasted per wall-clock second** → Should show as 20-30% improvement.

---

## The Doubt (Why It Might Not Help)

### 1. Closure Creation Might Be Cheap

Lua closure creation involves:
1. Capturing Arc (just ref-count bump - nanoseconds)
2. Registering Rust function with mlua (could be optimized)
3. Creating Lua function object (Lua is fast)

**What if it's only ~1μs per closure instead of 5μs?**

Then total overhead = 4μs per request
At 150k req/sec = 0.6 CPU seconds wasted
→ Only **5-8% improvement**, not 20-30%

### 2. Metatable Adds Overhead

Lazy context uses `__index` metamethod:

```rust
metatable.set("__index", lua.create_function(|lua, (table, key): (Table, String)| {
    // This runs on EVERY field access
    match key.as_str() {
        "headers" => { /* create closure */ }
        "query" => { /* create closure */ }
        "params" => { /* create closure */ }
        "body" => { /* create closure */ }
    }
})?)?;
```

**Overhead per field access:**
1. Lua checks table for field (miss)
2. Lua calls `__index` metamethod
3. String matching on key
4. Create closure
5. Return to Lua

**What if the `__index` overhead is 2-3μs?**

Then for a handler using 1 field:
- Current: 4 closures × 1μs = **4μs overhead**
- Lazy: 1 `__index` call × 3μs = **3μs overhead**

→ Only **25% improvement** (not 75% as claimed)

### 3. Real Handlers Might Use Multiple Fields

Example real-world handler:

```lua
function api.users.p_id.posts.post(ctx)
    local params = ctx:params()    -- Access 1
    local body = ctx:body()        -- Access 2
    local headers = ctx:headers()  -- Access 3 (check auth token)

    if not headers["authorization"] then
        return api.error(401, "Unauthorized")
    end

    -- Create post for user...
end
```

**Uses 3 out of 4 closures!**

- Current: 4 closures × 1μs = **4μs**
- Lazy: 3 `__index` calls × 3μs = **9μs**

→ **Slower with lazy context!** 😱

---

## The Benchmark Test

Your actual benchmark handler:

```lua
-- From tests/perf/main.lua
function api.echo.get(ctx)
    return api.json {
        message = "Echo GET",
        method = ctx.method  -- ← Simple field, not a closure
    }
end
```

**This handler creates 4 closures but uses ZERO of them.**

So lazy context **should** help on this specific benchmark... but is this representative of real usage?

---

## What We Need to Do: Profile It

I've created `profile_context_creation.sh` to test different patterns:

```bash
./profile_context_creation.sh
```

This tests:
1. **No context fields** → Baseline (measures other overhead)
2. **Only method** (simple field) → Should match baseline
3. **Only params** (1 closure) → If slower than #2, closures are expensive
4. **All fields** (4 closures) → Maximum closure overhead
5. **Real-world example** → Typical usage pattern

### What to Look For

**Scenario A: Closures ARE expensive**
```
No context:     160k req/s
Only method:    160k req/s  (same - method is simple field)
Only params:    140k req/s  (slower - created 4 closures, used 1)
All fields:     140k req/s  (same as params - all closures created regardless)
```
→ **Lazy context will help ~15%** (160k vs 140k)

**Scenario B: Closures are NOT expensive**
```
No context:     160k req/s
Only method:    160k req/s
Only params:    157k req/s  (barely slower)
All fields:     157k req/s
```
→ **Lazy context will help ~2%** (160k vs 157k) - not worth complexity

**Scenario C: Other overhead dominates**
```
No context:     160k req/s
Only method:    160k req/s
Only params:    160k req/s  (same - closure overhead is noise)
All fields:     160k req/s
```
→ **Lazy context won't help at all** - optimize something else

---

## The Real Question: Access Patterns

Even if closures are expensive, lazy context only helps if **handlers don't use all fields**.

Let's think about real handlers:

### Simple REST API (Common)
```lua
-- GET user
function api.users.p_id.get(ctx)
    return { user_id = ctx:params().id }  -- Only params
end

-- POST create user
function api.users.post(ctx)
    local body = ctx:body()  -- Only body
    -- ...
end

-- GET search users
function api.users.get(ctx)
    local query = ctx:query()  -- Only query (e.g., ?name=john)
    -- ...
end
```
→ **Each handler uses ~1 field** → Lazy context saves 3 closures each → **Good win**

### Complex API (Also Common)
```lua
function api.users.p_id.posts.post(ctx)
    local params = ctx:params()   -- Need user ID
    local body = ctx:body()       -- Need post data
    local headers = ctx:headers() -- Need auth token
    -- ...
end
```
→ **Uses 3 out of 4 fields** → Lazy context saves 1 closure → **Small win or even slower**

---

## My Honest Assessment

### Best Case (Optimistic)
- Closure creation is expensive (5μs each)
- Most handlers use 1-2 fields
- `__index` overhead is small (1μs)

**Result**: **+20-25% improvement**

### Realistic Case (My Guess)
- Closure creation is moderate (2μs each)
- Handlers use 2-3 fields on average
- `__index` overhead is moderate (2μs)

**Result**: **+8-12% improvement**

### Worst Case (Pessimistic)
- Closure creation is cheap (1μs each)
- Handlers use 3-4 fields
- `__index` overhead is significant (3μs)

**Result**: **+0-5% improvement** or even slight regression

---

## Recommendation: Measure First, Then Decide

### Step 1: Run Profiling Script (5 minutes)

```bash
./profile_context_creation.sh
```

This tells us if closure creation is actually expensive.

### Step 2: Implement Lazy Context (1-2 hours if worthwhile)

If profiling shows >10% difference, implement it.

### Step 3: Benchmark Real Workload

Test with your actual API handlers, not synthetic benchmarks.

---

## Alternative: Simpler Optimization

If closure creation turns out to be expensive, there's a **simpler fix** than lazy loading:

### Pre-Register Functions (No Metatable Complexity)

```rust
// At startup, create reusable accessor functions
fn initialize_lua_vm(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // Create ONE headers function that takes request data as argument
    let headers_fn = lua.create_function(|lua, req_data: Arc<RequestData>| {
        let headers = lua.create_table_with_capacity(0, req_data.headers.len())?;
        for (k, v) in &req_data.headers {
            headers.set(std::str::from_utf8(k)?, std::str::from_utf8(v)?)?;
        }
        Ok(headers)
    })?;

    globals.set("__get_headers", headers_fn)?;
    // ... same for query, params, body ...

    Ok(())
}

// Per request - just pass data, not create new functions
fn build_lua_context(lua: &Lua, task: &HttpTask) -> Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("method", std::str::from_utf8(&task.method)?)?;
    ctx.set("path", std::str::from_utf8(&task.path)?)?;

    let req_data = Arc::new(RequestData { /* ... */ });
    ctx.raw_set("__req_data", req_data)?;

    // Metatable that calls pre-registered functions
    let metatable = lua.create_table()?;
    metatable.set("__index", lua.create_function(|lua, (table, key): (Table, String)| {
        let req_data: Arc<RequestData> = table.raw_get("__req_data")?;
        let globals = lua.globals();

        match key.as_str() {
            "headers" => {
                let get_fn: mlua::Function = globals.get("__get_headers")?;
                let result = get_fn.call(req_data)?;
                Ok(result)
            }
            // ... other fields ...
            _ => Ok(Value::Nil)
        }
    })?)?;

    ctx.set_metatable(Some(metatable));
    Ok(ctx)
}
```

**Benefits:**
- No closure creation per request
- Functions are created once at startup
- Still lazy (only called when accessed)
- Simpler than managing multiple VMs

**Trade-off:**
- Still has `__index` overhead
- Slightly more complex than current code

---

## Bottom Line

**I'm not 100% sure lazy context will help** because:

1. We don't know if closure creation is the bottleneck
2. We don't know real handler access patterns
3. Metatable overhead might offset savings

**But we can find out in 5 minutes** by running the profiling script.

Want to run the test and see what the actual numbers say?
