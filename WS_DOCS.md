# Rover WebSocket DSL - Documentation

## Overview

The Rover WebSocket DSL provides a clean, semantic way to build real-time applications in Lua. It uses **natural language patterns**: `listen` for declarations and `send` for actions.

**Core Philosophy:**
- `listen.<event>` - Function declarations (passive, waiting)
- `send.<event>()` - Method invocations (active, sending)
- Path params and query params available like HTTP routes
- Automatic type wrapping and event routing

---

## Quick Start

### Server (Simple Chat)

```lua
local api = rover.server {}

function api.chat.ws(ws)

  function ws.join(ctx)
    local user_id = ctx:params().user_id or "anon"

    ws.send.user_joined():all {
      user_id = user_id,
      timestamp = os.time()
    }

    return { user_id = user_id }
  end

  function ws.listen.chat(msg, ctx, state)
    ws.send.chat():all {
      user_id = state.user_id,
      text = msg.text,
      timestamp = os.time()
    }
  end

  function ws.leave(state)
    ws.send.user_left():all {
      user_id = state.user_id
    }
  end

end

return api
```

### Client (Simple Chat)

```lua
local client = ws.new("ws://localhost:3000/chat/ws?user_id=alice")

function client.join()
  print("Connected!")
  client.send.chat("Hello!")
end

function client.listen.chat(msg)
  print("[" .. msg.user_id .. "]: " .. msg.text)
end

function client.listen.user_joined(msg)
  print(msg.user_id .. " joined")
end

function client.listen.user_left(msg)
  print(msg.user_id .. " left")
end

function client.leave()
  print("Disconnected!")
end

client:connect()
while true do
  client:pump()
end
```

---

## Server API

### Lifecycle

```lua
function ws.join(ctx)
  -- Called when client connects
  -- ctx has: headers(), query(), params()
  -- Return state to pass to listeners and leave()

  return { user_id = "123", connected_at = os.time() }
end

function ws.leave(state)
  -- Called when client disconnects
  -- state is what you returned from join()
end
```

### Listening for Events

```lua
function ws.listen.chat(msg, ctx, state)
  -- Called when client sends event: { type = "chat", text = "..." }
  -- msg = payload without type field
  -- ctx = HTTP context
  -- state = what you returned from join()
end

function ws.listen.typing(msg, ctx, state)
  -- Another event listener
end

function ws.listen.message(msg, ctx, state)
  -- Catch-all for untyped messages
end
```

### Sending Events

```lua
-- Send to all connected clients
ws.send.chat():all {
  user_id = "123",
  text = "Hello everyone!"
}

-- Send to all except sender
ws.send.typing():except {
  user_id = "123",
  status = "typing"
}

-- Send to specific topic/room
ws.send.chat():to("room:lobby") {
  text = "Hello room!"
}

-- Send to all subscriptions of this connection
ws.send.notification():to_subscriptions {
  message = "Check this out"
}

-- Reply to this client only
ws.send.acknowledgement {
  success = true
}
```

### Subscriptions

```lua
function ws.join(ctx)
  -- Subscribe to room broadcasts
  ws.listen("room:" .. ctx:params().room_id)
  return { room_id = ctx:params().room_id }
end

function ws.listen.room(msg, ctx, state)
  -- Called when broadcast sent to "room:X" topic
  -- Routes it back to the client
  ws.send.room_update {
    event = msg.type,
    data = msg
  }
end
```

---

## Client API

### Lifecycle

```lua
function client.join()
  -- Called when connection opens
  client.send.chat("I'm here!")
end

function client.listen.chat(msg)
  -- Called when server sends chat event
  print(msg.text)
end

function client.leave()
  -- Called when connection closes
end
```

### Sending Events

```lua
-- Simple sends (automatically wraps with type)
client.send.chat("Hello!")
client.send.typing()
client.send.stop_typing()

-- Send with multiple parameters
client.send.edit(position, text, delete_count)
client.send.cursor_move(10)
client.send.selection_change(5, 15)

-- Send complex payload
client.send.message {
  type = "data",
  content = "something"
}

-- Send untyped raw message
client.send.raw { any = "data" }
```

### Listening for Events

```lua
function client.listen.chat(msg)
  -- Called when server broadcasts "chat" event
  print(msg.text)
end

function client.listen.user_joined(msg)
  -- Called when server broadcasts "user_joined" event
  print(msg.name)
end

function client.listen.message(msg)
  -- Catch-all for untyped events
end
```

### Connection Management

```lua
client:connect()        -- Open connection
client:pump()           -- Process messages (call in loop)
client:disconnect()     -- Close connection
```

---

## Complete Examples

### Example 1: Multi-Room Chat

**Server:**

```lua
local api = rover.server {}

function api.chat.p_room_id.ws(ws)

  function ws.join(ctx)
    local room_id = ctx:params().room_id
    local user_id = ctx:params().user_id
    local name = ctx:query().name or "Guest"

    -- Subscribe to room topic
    ws.listen("room:" .. room_id)

    -- Announce user joined
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
      name = state.name,
      room_id = state.room_id
    }
  end

  function ws.listen.room(msg, ctx, state)
    -- Room subscription broadcasts
    ws.send.room_message {
      event_type = msg.type,
      data = msg
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

**Client:**

```lua
local client = ws.new("ws://localhost:3000/chat/general/ws?user_id=alice&name=Alice")

function client.join()
  print("[CONNECTED] You joined #general")
end

function client.listen.chat(msg)
  print("[" .. msg.name .. "]: " .. msg.text)
end

function client.listen.user_joined(msg)
  print("[+] " .. msg.name .. " joined #" .. msg.room_id)
end

function client.listen.user_left(msg)
  print("[-] " .. msg.name .. " left")
end

function client.listen.typing(msg)
  print("[...] " .. msg.name .. " is typing")
end

function client.listen.room_message(msg)
  -- Other room events
end

function client.leave()
  print("[DISCONNECTED]")
end

client:connect()

-- Send messages
client.send.chat("Hello everyone!")
client.send.typing()
client.send.chat("How is everyone?")
client.send.typing()

while true do
  client:pump()
end
```

---

### Example 2: Collaborative Document Editor

**Server:**

```lua
local api = rover.server {}

function api.editor.p_doc_id.ws(ws)

  function ws.join(ctx)
    local doc_id = ctx:params().doc_id
    local user_id = ctx:params().user_id

    ws.listen("doc:" .. doc_id)
    ws.listen("presence:" .. doc_id)

    ws.send.user_online():to("presence:" .. doc_id) {
      user_id = user_id,
      cursor = 0
    }

    return {
      doc_id = doc_id,
      user_id = user_id,
      cursor = 0,
      version = 0
    }
  end

  function ws.listen.edit(msg, ctx, state)
    state.version = state.version + 1

    ws.send.document_edit():all {
      doc_id = state.doc_id,
      user_id = state.user_id,
      position = msg.position,
      text = msg.text,
      delete_count = msg.delete_count or 0,
      version = state.version
    }
  end

  function ws.listen.cursor_move(msg, ctx, state)
    state.cursor = msg.position

    ws.send.cursor_update():except {
      user_id = state.user_id,
      cursor = msg.position
    }
  end

  function ws.listen.request_sync(msg, ctx, state)
    ws.send.full_sync {
      doc_id = state.doc_id,
      content = "[[document content here]]",
      version = state.version
    }
  end

  function ws.listen.doc(msg, ctx, state)
    ws.send.doc_update {
      type = msg.type,
      data = msg
    }
  end

  function ws.listen.presence(msg, ctx, state)
    ws.send.presence_update {
      users = msg.users
    }
  end

  function ws.leave(state)
    ws.send.user_offline():to("presence:" .. state.doc_id) {
      user_id = state.user_id
    }
  end

end

return api
```

**Client:**

```lua
local client = ws.new("ws://localhost:3000/editor/doc-123/ws?user_id=alice")

function client.join()
  print("[CONNECTED] Document opened")
  client.send.request_sync()
end

function client.listen.full_sync(msg)
  print("[SYNC] Version:", msg.version)
  print("Content:", msg.content)
end

function client.listen.document_edit(msg)
  print("[EDIT] User", msg.user_id, "at position", msg.position)
end

function client.listen.cursor_update(msg)
  print("[CURSOR] User", msg.user_id, "at", msg.cursor)
end

function client.listen.user_online(msg)
  print("[ONLINE] User", msg.user_id)
end

function client.listen.user_offline(msg)
  print("[OFFLINE] User", msg.user_id)
end

function client.listen.doc_update(msg)
  -- Document subscription updates
end

function client.listen.presence_update(msg)
  -- Presence updates
end

function client.leave()
  print("[DISCONNECTED] Document closed")
end

client:connect()

-- Simulate editing
client.send.edit(0, "Hello ", 0)
client.send.cursor_move(6)
client.send.edit(6, "World", 0)
client.send.cursor_move(11)

while true do
  client:pump()
end
```

---

## API Reference

### Server Functions

| Pattern | Purpose |
|---------|---------|
| `function ws.join(ctx)` | Lifecycle: client connects |
| `function ws.listen.<event>(msg, ctx, state)` | Listen for client event |
| `function ws.listen.topic(msg, ctx, state)` | Listen for subscription broadcasts |
| `ws.send.<event>():all { ... }` | Send to all clients |
| `ws.send.<event>():to(topic) { ... }` | Send to topic |
| `ws.send.<event>():except { ... }` | Send to all except sender |
| `ws.send.<event>():to_subscriptions { ... }` | Send to subscriptions |
| `ws.send.event { ... }` | Reply to this client only |
| `function ws.leave(state)` | Lifecycle: client disconnects |
| `ws.listen(topic)` | Subscribe to topic |
| `ws.error(code, msg)` | Reject connection at join |

### Client Functions

| Pattern | Purpose |
|---------|---------|
| `function client.join()` | Lifecycle: connection opens |
| `function client.listen.<event>(msg)` | Listen for server event |
| `client.send.<event>(args)` | Send event to server |
| `client:connect()` | Open connection |
| `client:pump()` | Process messages |
| `client:disconnect()` | Close connection |
| `function client.leave()` | Lifecycle: connection closes |

---

## Key Design Principles

✅ **Natural language** - `listen` and `send` are intuitive verbs
✅ **Declarations vs invocations** - `listen.X()` declares, `send.X()` invokes
✅ **Symmetric between server/client** - Both use same patterns
✅ **Type wrapped automatically** - Function name becomes message type
✅ **Path params like HTTP** - Use `/path/:param/ws` syntax
✅ **State threaded** - join() → listeners → leave()
✅ **Subscriptions built-in** - publish/subscribe patterns native
✅ **IDE friendly** - All code visible and autocomplete-able

---

## Running Examples

**Start server:**
```bash
rover run server.lua --port 3000
```

**Run client:**
```bash
lua client.lua
```

That's it! Real-time Lua has never been simpler. 🚀
