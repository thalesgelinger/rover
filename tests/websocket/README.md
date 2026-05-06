# WebSocket Server Tests

This directory contains tests for WebSocket server functionality.

## Test Files

### Lua Test Scripts

- `test_echo.lua` - Basic echo server test
- `test_chat.lua` - Chat server with state management
- `test_multi_room.lua` - Multi-room chat with topics

### Integration Tests

- `websocket_integration_test.rs` - Rust integration tests (requires running server)

### Test Script

- `test_websocket_servers.sh` - Automated test script

## Running Tests

### Syntax Check

```bash
cargo run -p rover_cli -- check tests/websocket/test_echo.lua
cargo run -p rover_cli -- check tests/websocket/test_chat.lua
cargo run -p rover_cli -- check tests/websocket/test_multi_room.lua
```

### Format Check

```bash
cargo run -p rover_cli -- fmt tests/websocket/test_echo.lua --check
cargo run -p rover_cli -- fmt tests/websocket/test_chat.lua --check
cargo run -p rover_cli -- fmt tests/websocket/test_multi_room.lua --check
```

### Run Servers

```bash
# Echo server
cargo run -p rover_cli -- run tests/websocket/test_echo.lua

# Chat server
cargo run -p rover_cli -- run tests/websocket/test_chat.lua

# Multi-room chat
cargo run -p rover_cli -- run tests/websocket/test_multi_room.lua
```

### Integration Tests

Integration tests require a running WebSocket server:

```bash
# Terminal 1: Start server
cargo run -p rover_cli -- run examples/websocket/chat.lua

# Terminal 2: Run tests
cargo test -p rover_server --test websocket_integration_test
```

## Test Coverage

The tests cover:

1. **Lifecycle Hooks**
   - `ws.join(ctx)` - Connection initialization
   - `ws.listen.<event>(msg, ctx, state)` - Message handling
   - `ws.listen.message(msg, ctx, state)` - Fallback handler
   - `ws.leave(state)` - Disconnection cleanup

2. **Event Dispatch**
   - `ws.send.<event>(payload)` - Send to current client
   - `ws.send.<event>():all(payload)` - Broadcast to all
   - `ws.send.<event>():except(payload)` - Broadcast except sender
   - `ws.send.<event>():to(topic)` - Send to topic
   - `ws.listen(topic)` - Subscribe to topic

3. **Error Handling**
   - Message validation
   - State management
   - Connection errors

## Documentation

See `/docs/docs/api-reference/ws-server.md` for complete WebSocket server documentation.
