---
sidebar_position: 5
---

# WebSocket Server

Build real-time WebSocket endpoints with Rover's server-side WebSocket DSL.

## Basic Structure

Define a WebSocket endpoint using the `ws` method on a route:

```lua
local api = rover.server {}

function api.echo.ws(ws)
  function ws.join(ctx)
    ws.send.welcome { message = "connected" }
    return {}
  end

  function ws.listen.echo(msg, ctx, state)
    ws.send.echo { text = msg.text }
  end

  function ws.leave(state)
    -- cleanup
  end
end

return api
```

## Lifecycle Hooks

### `ws.join(ctx)`

Called when a client connects. Use to:
- Send welcome message
- Initialize connection state
- Subscribe to topics

```lua
function ws.join(ctx)
  local user_id = ctx:query().user_id or "anon"
  
  ws.send.welcome {
    message = "Welcome to the chat",
    timestamp = os.time()
  }
  
  return { user_id = user_id }
end
```

**Parameters:**
- `ctx` - Request context with `params()`, `query()`, `headers()`

**Returns:**
- Return a table to initialize connection state
- State is passed to all subsequent handlers

### `ws.listen.<event>(msg, ctx, state)`

Called for each typed message. Event name matches `type` field in JSON:

```lua
-- Client sends: {"type":"chat","text":"hello"}
function ws.listen.chat(msg, ctx, state)
  ws.send.chat {
    user_id = state.user_id,
    text = msg.text,
    timestamp = os.time()
  }
end
```

**Parameters:**
- `msg` - Parsed JSON message (table)
- `ctx` - Request context
- `state` - Connection state from `join` or previous handler

**Returns:**
- Return a table to update connection state
- Return `nil` to keep current state

### `ws.listen.message(msg, ctx, state)`

Fallback handler for untyped messages or when no typed handler matches:

```lua
function ws.listen.message(msg, ctx, state)
  local kind = msg.type
  if kind == "identify" then
    return ws.listen.identify(msg, ctx, state)
  end
  if kind == "chat" then
    return ws.listen.chat(msg, ctx, state)
  end
end
```

### `ws.leave(state)`

Called when client disconnects. Use to:
- Broadcast user left notification
- Clean up resources
- Unsubscribe from topics

```lua
function ws.leave(state)
  ws.send.user_left():all {
    user_id = state.user_id,
    timestamp = os.time()
  }
end
```

**Parameters:**
- `state` - Final connection state

## Event Dispatch

### Send to Current Client

```lua
ws.send.<event>(payload)
```

Send a typed event to the current client:

```lua
ws.send.welcome { message = "connected" }
-- Client receives: {"type":"welcome","message":"connected"}
```

### Broadcast to All

```lua
ws.send.<event>():all(payload)
```

Send to all connected clients:

```lua
ws.send.chat():all {
  user_id = state.user_id,
  text = msg.text,
  timestamp = os.time()
}
```

### Broadcast Except Sender

```lua
ws.send.<event>():except(payload)
```

Send to all clients except the sender:

```lua
ws.send.typing():except {
  user_id = state.user_id
}
```

### Send to Topic

```lua
ws.send.<event>():to(topic)(payload)
```

Send to clients subscribed to a topic:

```lua
ws.send.chat():to("room:" .. state.room_id) {
  user_id = state.user_id,
  text = msg.text
}
```

### Topic Subscriptions

Subscribe to topics in `join`:

```lua
function ws.join(ctx)
  local room_id = ctx:params().room_id
  
  -- Subscribe to room topic
  ws.listen("room:" .. room_id)
  
  return { room_id = room_id }
end
```

## Path Parameters

Use `p_<name>` prefix for dynamic routes:

```lua
function api.chat.p_room_id.ws(ws)
  function ws.join(ctx)
    local room_id = ctx:params().room_id
    -- ...
  end
end
```

Connect: `ws://localhost:4242/chat/general`

## Error Handling

### Validation Errors

Validate messages before processing:

```lua
function ws.listen.chat(msg, ctx, state)
  if msg.text == nil or msg.text == "" then
    ws.send.error { message = "text required" }
    return
  end
  
  ws.send.chat():all {
    user_id = state.user_id,
    text = msg.text
  }
end
```

### State Management

Return updated state from handlers:

```lua
function ws.listen.identify(msg, ctx, state)
  local user_id = msg.user_id or "anon"
  
  ws.send.user_joined():all {
    user_id = user_id
  }
  
  -- Update state
  return { user_id = user_id }
end
```

### Connection Errors

Handle errors in `leave` hook:

```lua
function ws.leave(state)
  if state.user_id then
    ws.send.user_left():all {
      user_id = state.user_id
    }
  end
end
```

## Complete Example

Multi-room chat with topics and broadcasts:

```lua
local api = rover.server {}

function api.chat.p_room_id.ws(ws)
  
  function ws.join(ctx)
    local room_id = ctx:params().room_id
    local user_id = ctx:query().user_id or "anon"
    local name = ctx:query().name or "Guest"
    
    -- Subscribe to room
    ws.listen("room:" .. room_id)
    
    -- Announce to room
    ws.send.user_joined():to("room:" .. room_id) {
      user_id = user_id,
      name = name,
      room_id = room_id
    }
    
    return {
      user_id = user_id,
      name = name,
      room_id = room_id
    }
  end
  
  function ws.listen.chat(msg, ctx, state)
    if msg.text == nil or msg.text == "" then
      ws.send.error { message = "text required" }
      return
    end
    
    ws.send.chat():to("room:" .. state.room_id) {
      user_id = state.user_id,
      name = state.name,
      text = msg.text,
      timestamp = os.time()
    }
  end
  
  function ws.listen.typing(msg, ctx, state)
    ws.send.typing():except {
      user_id = state.user_id,
      name = state.name
    }
  end
  
  function ws.leave(state)
    ws.send.user_left():to("room:" .. state.room_id) {
      user_id = state.user_id,
      name = state.name
    }
  end
  
end

return api
```

## Testing

Connect with any WebSocket client:

```bash
# Using wscat
wscat -c "ws://localhost:4242/chat/general?user_id=alice&name=Alice"

# Send message
> {"type":"chat","text":"hello"}

# Receive broadcast
< {"type":"chat","user_id":"alice","name":"Alice","text":"hello","timestamp":1234567890}
```

## Best Practices

1. **Initialize state in `join`** - Return a table with user/session data
2. **Validate messages** - Check required fields before processing
3. **Use topics for rooms** - Subscribe in `join`, broadcast with `:to()`
4. **Broadcast with `:except`** - Avoid echoing to sender
5. **Clean up in `leave`** - Broadcast departure, unsubscribe from topics

## Runtime Semantics

The following behavior contracts are verified by tests:

### Lifecycle Hooks

| Contract | Test |
|----------|------|
| `ws.join` called on connect, receives `ctx` | `test_websocket_join_hook`, `test_websocket_connect_flow_ctx_methods` (integration) |
| `ws.join` return value becomes initial state | `test_websocket_state_management`, `test_websocket_message_flow_state_propagation` (integration) |
| `ws.leave` called on disconnect, receives `state` | `test_websocket_leave_hook`, `test_websocket_close_flow_state_cleanup` (integration) |
| Complete connect/message/close flow | `test_websocket_complete_lifecycle` (integration) |

### Event Dispatch

| Contract | Test |
|----------|------|
| Typed handler `ws.listen.<event>` matches `type` field | `test_websocket_echo_server` (integration) |
| Fallback `ws.listen.message` for untyped messages | `test_websocket_fallback_handler` (integration) |
| Handler receives `(msg, ctx, state)` | `test_handler_receives_msg_ctx_state_arguments` (unit), `test_websocket_message_flow_state_propagation` (integration) |
| Handler return updates state | `test_websocket_state_management`, `test_websocket_state_update_chain` (integration) |

### Send Operations

| Contract | Test |
|----------|------|
| `ws.send.<event>(data)` sends to current client | `test_serialize_event_json_injects_type` (unit) |
| `ws.send.<event>():all(data)` broadcasts to all | `test_websocket_broadcast` (integration) |
| `ws.send.<event>():except(data)` excludes sender | `test_websocket_multiple_clients` (integration) |
| `ws.send.<event>():to(topic)(data)` sends to topic | `test_websocket_topic_subscription` (integration) |

### Message Format

| Contract | Test |
|----------|------|
| Outgoing JSON includes `type` field | `test_serialize_event_json_injects_type` (unit) |
| Empty payload produces `{"type":"event"}` | `test_serialize_event_json_empty_object` (unit) |
| Nested objects preserved | `test_serialize_event_json_preserves_nested_object` (unit) |
| Arrays wrapped as `{"type":"event","data":[...]}` | `test_serialize_event_json_array_wrapped` (unit) |

### Topic Subscriptions

| Contract | Test |
|----------|------|
| `ws.listen(topic)` subscribes connection | `test_websocket_topic_subscription` (integration) |
| Topic members receive broadcasts | `test_subscribe_and_topic_members` (unit) |
| Duplicate subscribe is idempotent | `test_subscribe_idempotent` (unit) |

### Path Parameters

| Contract | Test |
|----------|------|
| `p_<name>` extracts path segment | `test_websocket_path_parameters` (integration) |

### Error Handling

| Contract | Test |
|----------|------|
| `ws.error(code, msg)` rejects connection | `test_ws_error_sets_error_code_and_msg` (unit) |
| Validation errors sent via `ws.send.error` | `test_websocket_validation` (integration) |

## Related

- [WebSocket Client](/docs/api-reference/ws-client) - Client-side WebSocket API
- [Backend Server](/docs/guides/backend-server) - HTTP server basics
- [Context API](/docs/guides/context-api) - Request context methods
