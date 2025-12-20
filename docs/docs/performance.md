---
sidebar_position: 10
---

# Performance

Rover is built for speed with zero-copy response handling and optimized Lua integration.

## Benchmarks

Built-in performance test results:

```
Requests/sec:   182,000
Latency (avg):  0.49ms
Latency (p99):  0.67ms
```

## Running Performance Tests

Test with [wrk](https://github.com/wg/wrk):

### Built-in Performance Test

```bash
# Run the built-in perf test
./target/release/rover tests/perf/main.lua &

# Run the test script
cd tests/perf && bash test.sh
```

### Custom Benchmark

Create your own benchmark:

```bash
wrk -t4 -c100 -d30s http://localhost:4242/endpoint
```

**Options:**
- `-t4`: Use 4 threads
- `-c100`: Keep 100 HTTP connections open
- `-d30s`: Run for 30 seconds

## Performance Features

### Pre-Serialized Responses

All response builders use pre-serialization for near-zero overhead:

```lua
-- These are optimized for performance
return api.json { data = "..." }
return api.text("Hello")
return api.html("<h1>Hi</h1>")
```

### Zero-Copy Response Handling

Rover uses zero-copy techniques to minimize memory allocations and data copying, resulting in:
- Lower latency
- Higher throughput
- Better memory efficiency

### Optimized Lua Integration

The Lua runtime is tightly integrated with the HTTP server, avoiding unnecessary data conversions and copies.

## Best Practices

1. **Use response builders**: They're optimized for performance
2. **Minimize allocations**: Reuse tables when possible
3. **Keep handlers simple**: Complex logic should be in separate modules
4. **Benchmark your code**: Use `wrk` to test your endpoints

## Next Steps

- [Backend Server](/guides/backend-server) - Build performant APIs
- [Response Builders](/guides/response-builders) - Use optimized responses
