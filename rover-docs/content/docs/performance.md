---
weight: 10
title: Performance
---

Rover is built for speed with zero-copy response handling and optimized Lua integration.

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

- [Backend Server](/docs/guides/backend-server) - Build performant APIs
- [Response Builders](/docs/guides/response-builders) - Use optimized responses
