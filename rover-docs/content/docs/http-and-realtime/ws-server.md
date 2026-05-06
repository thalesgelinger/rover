---
weight: 12
title: WebSocket Server
aliases:
  - /docs/server/ws-server/
  - /docs/http-and-realtime/ws-server/
---

Build real-time WebSocket endpoints with Rover's server-side WebSocket DSL.

## Basic Structure

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

Called when a client connects. Return a table to initialize connection state.

### `ws.listen.<event>(msg, ctx, state)`

Called for each typed message. Event name matches `type` in incoming JSON.

### `ws.listen.message(msg, ctx, state)`

Fallback handler for untyped messages or unmatched typed handlers.

### `ws.leave(state)`

Called when the client disconnects.

## Send Operations

- `ws.send.<event>(payload)` sends to current client
- `ws.send.<event>():all(payload)` broadcasts to all connected clients
- `ws.send.<event>():except(payload)` broadcasts to all except sender
- `ws.send.<event>():to(topic)(payload)` sends to topic subscribers

## Topic Subscriptions

Subscribe inside `join`:

```lua
function ws.join(ctx)
  local room_id = ctx:params().room_id
  ws.listen("room:" .. room_id)
  return { room_id = room_id }
end
```

## Path Parameters

```lua
function api.chat.p_room_id.ws(ws)
  function ws.join(ctx)
    local room_id = ctx:params().room_id
    return { room_id = room_id }
  end
end
```

Route above maps to `/chat/:room_id`.

## Error Handling

Use explicit validation in handlers and return typed `error` events when payloads are invalid.

```lua
function ws.listen.chat(msg, ctx, state)
  if msg.text == nil or msg.text == "" then
    ws.send.error { message = "text required" }
    return
  end

  ws.send.chat():all {
    user_id = state.user_id,
    text = msg.text,
  }
end
```

## Runtime Semantics

Contracts below are verified by tests in this repo:

- `join` called on connect and return value becomes initial state.
- `leave` called on disconnect with final state.
- Typed dispatch uses `type`; fallback uses `ws.listen.message`.
- Handler return value updates state for subsequent events.
- Outgoing JSON always includes `type`.

## Testing

```bash
wscat -c "ws://localhost:4242/chat/general?user_id=alice"
```

Then send:

```json
{"type":"chat","text":"hello"}
```
