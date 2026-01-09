## Summary

This PR represents a comprehensive performance overhaul of the Rover web framework, achieving dramatic throughput improvements through a custom event-driven architecture. The changes replace the async runtime with a synchronous, mio-based event loop optimized for Lua coroutine integration.

## Key Performance Improvements

**652x throughput increase**: From ~201 req/s (tiny_http) to 131k+ req/s with the initial mio implementation, with further optimizations pushing beyond 130k RPS.

## Major Changes

### Event Loop Architecture
- **Replaced tokio/hyper with mio**: Custom event loop built on mio for lower-level control
- **Lua coroutine integration**: Proper yielding for I/O operations with coroutine pooling
- **Event-driven design**: Eliminated polling timeout in favor of event-driven architecture with timer-based timeout checking
- **Dynamic mio interest registration**: Eliminated O(n) scans and spurious wakeups
- **Direct handler calls**: Removed coroutine overhead by calling handlers directly when possible

### Memory Optimizations
- **Zero-copy parsing**: HTTP request parsing without intermediate allocations
- **Buffer pooling**: Comprehensive pooling for headers, query strings, request contexts, and response buffers
- **Streaming JSON parser**: Eliminated `serde_json::Value` allocations in favor of streaming parser
- **JSON buffer pooling**: Pool and reuse JSON serialization buffers to reduce malloc overhead
- **Freeze-and-slice pattern**: Request buffers are frozen and sliced without copies
- **Connection pooling**: Zero-copy buffer optimizations for connection management

### Routing & Request Handling
- **Indexed route matching**: Added route cache support with LRU caching
- **Single-pass JSON serialization**: Optimized JSON handling
- **ahash for path hashing**: Faster hash operations for routing
- **Vectored I/O**: Response writing uses vectored I/O for efficiency
- **Inline response handling**: Reduced indirection in hot paths

### Data Structure Optimizations
- **HashMap instead of Slab**: O(1) coroutine lookups
- **Table pooling**: Lua table reuse to reduce allocations
- **Direct JSON parser**: Custom parser avoiding intermediate allocations
- **Offset-based RequestContext**: Reduced memory overhead

## Components Modified

- **rover-server**: Complete event loop rewrite, new modules for buffer pooling, connection management, streaming JSON parser, and table pooling
- **rover-core**: Updated HTTP, I/O, and server interfaces for sync architecture
- **Dependencies**: Removed tokio/hyper/reqwest, added mio, reduced serde dependencies

## Files Changed
- 32 files modified
- +2,635 insertions, -1,636 deletions
- New modules: `buffer_pool.rs`, `connection.rs`, `direct_json_parser.rs`, `table_pool.rs`

## Testing

Multiple benchmark scripts and test files have been added to validate performance improvements and correctness of the new implementation.

## Breaking Changes

⚠️ This is a fundamental architecture change. The sync-based API is incompatible with the previous async design. Existing code using tokio/async will need to be updated.
