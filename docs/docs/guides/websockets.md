---
sidebar_position: 4
---

# WebSockets

:::warning Experimental Feature
WebSocket support is currently experimental and under active development. The API may change.
:::

Build real-time applications with Rover's WebSocket DSL for bi-directional communication.

## Simple Echo Server

Here's the simplest WebSocket server:

```lua
local api = rover.server {}

function api.echo.ws(ctx, ws)
    function ws.on.connect()
        print("Client connected")
    end

    function ws.read.message(payload)
        ws.reply.message({ text = payload.text })
    end
end

return api
```

## Simple Client

```lua
local ws = rover.ws_client("ws://localhost:3000/echo")
ws.connect()

function ws.on.connect()
    ws.send.message({ text = "Hello!" })
end

function ws.on.message(payload)
    print("Received: " .. payload.text)
end
```

## Topics and Broadcasting

Use topics to group clients and broadcast messages:

```lua
function api.chat.ws(ctx, ws)
    -- Client joins a topic
    function ws.join.lobby(payload)
        if not payload.user then
            return { status = "error", reason = "User required" }
        end
        return { status = "ok", topic = "lobby" }
    end

    -- Broadcast message to all in topic
    function ws.read.message(payload)
        ws.emit.lobby.message({
            user = payload.user,
            text = payload.text,
            timestamp = os.time()
        })
    end
end
```

Client side:

```lua
function ws.on.connect()
    ws.join.lobby({ user = "alice" })
end

function ws.on.join_ack(payload)
    print("Joined: " .. payload.topic)
    ws.send.message({ user = "alice", text = "Hi everyone!" })
end

function ws.on.join_error(payload)
    print("Failed to join: " .. payload.reason)
end

function ws.on.message(payload)
    print(payload.user .. ": " .. payload.text)
end
```

## Server API Reference

### Lifecycle Events

```lua
function ws.on.connect()
    -- Called when client connects
end

function ws.on.disconnect(code, reason)
    -- Called when client disconnects
end
```

### Topic Management

```lua
-- Handle join request
function ws.join.{topic}(payload)
    -- Authorize and return status
    return { status = "ok", topic = "topic_name" }
    -- Or reject
    return { status = "error", reason = "Not authorized" }
end

-- Handle leave request
function ws.leave.{topic}(payload)
    return { status = "ok", topic = "topic_name" }
end
```

### Receiving Events

```lua
-- Read event from client
function ws.read.{event}(payload)
    -- Handle incoming event
end
```

### Sending Events

```lua
-- Reply to current client only
ws.reply.{event}({ data = "value" })

-- Broadcast to all clients in topic
ws.emit.{topic}.{event}({ data = "value" })

-- Send to specific client
ws.send_to(connection_id).{event}({ data = "value" })
```

### Transform Outgoing Events

```lua
-- Filter or modify events before sending
function ws.map.{event}(payload)
    payload.server_time = os.time()
    return payload
end
```

### Connection Info

```lua
function api.chat.ws(ctx, ws)
    local id = ws.id -- Unique connection ID
end
```

## Client API Reference

### Connection

```lua
local ws = rover.ws_client("ws://localhost:3000/chat")
ws.connect()

ws.reconnect() -- Reconnect after disconnect
```

### Lifecycle Events

```lua
function ws.on.connect()
    -- Connected to server
end

function ws.on.disconnect(code, reason)
    -- Disconnected from server
end

function ws.on.error(err)
    -- Error occurred
end
```

### Topic Management

```lua
-- Request to join topic
ws.join.{topic}({ user = "alice" })

-- Leave topic
ws.leave.{topic}()

-- Handle responses
function ws.on.join_ack(payload)
    print("Joined: " .. payload.topic)
end

function ws.on.join_error(payload)
    print("Failed: " .. payload.reason)
end

function ws.on.leave_ack(payload)
    print("Left: " .. payload.topic)
end
```

### Sending Events

```lua
ws.send.{event}({ data = "value" })
```

### Receiving Events

```lua
function ws.on.{event}(payload)
    -- Handle incoming event
end
```

## Complete Chat Example

Server:

```lua
local api = rover.server {}

function api.chat.ws(ctx, ws)
    function ws.on.connect()
        print("User connected: " .. ws.id)
    end

    function ws.join.room(payload)
        if not payload.username then
            return { status = "error", reason = "Username required" }
        end
        -- Announce to room
        ws.emit.room.user_joined({ user = payload.username })
        return { status = "ok", topic = "room" }
    end

    function ws.read.chat_message(payload)
        ws.emit.room.chat_message({
            user = payload.user,
            text = payload.text,
            timestamp = os.time()
        })
    end

    function ws.read.typing(payload)
        ws.emit.room.user_typing({ user = payload.user })
    end

    function ws.leave.room(payload)
        ws.emit.room.user_left({ user = payload.user })
        return { status = "ok", topic = "room" }
    end

    function ws.on.disconnect()
        print("User disconnected")
    end
end

return api
```

Client:

```lua
local ws = rover.ws_client("ws://localhost:3000/chat")
local username = "alice"

ws.connect()

function ws.on.connect()
    ws.join.room({ username = username })
end

function ws.on.join_ack(payload)
    print("Joined chat room!")
    ws.send.chat_message({ user = username, text = "Hello everyone!" })
end

function ws.on.chat_message(payload)
    print(payload.user .. ": " .. payload.text)
end

function ws.on.user_joined(payload)
    print(payload.user .. " joined the room")
end

function ws.on.user_left(payload)
    print(payload.user .. " left the room")
end

function ws.on.user_typing(payload)
    print(payload.user .. " is typing...")
end

function ws.on.disconnect(code, reason)
    print("Disconnected: " .. reason)
    ws.reconnect()
end
```

## Advanced: Direct Messaging

Send message to specific user:

```lua
function ws.read.private_message(payload)
    local target_id = payload.target_connection_id
    ws.send_to(target_id).private_message({
        from = payload.from,
        text = payload.text
    })
end
```

## Advanced: Kicking Users

```lua
function ws.read.report_user(payload)
    local reported_id = payload.reported_connection_id
    ws.send_to(reported_id).kicked({
        reason = "Reported by moderator"
    })
end
```

Client handles kick:

```lua
function ws.on.kicked(payload)
    print("You were kicked: " .. payload.reason)
end
```

## Next Steps

- [Backend Server](/docs/guides/backend-server) - HTTP endpoints
- [Context API](/docs/guides/context-api) - Access request data
