local ws = rover.ws_client("ws://echo.websocket.org")
ws.connect()

function ws.on.connect()
    print("Connected successfully")
    -- Join a topic
    ws.join.chat_lobby({user = "client_user"})
end

function ws.on.disconnect(code, reason)
    print("Disconnected: " .. code .. " - " .. reason)
    -- Attempt reconnection
    ws.reconnect()
end

-- Custom event handlers
function ws.on.join_ack(payload)
    print("Joined topic: " .. payload.topic)
    -- Send initial message
    ws.send.new_msg({text = "Hello from client!", user = "client_user"})
end

function ws.on.join_error(payload)
    print("Failed to join: " .. payload.reason)
end

function ws.on.leave_ack(payload)
    print("Left topic: " .. payload.topic)
end

function ws.on.new_msg(payload)
    print("Received message: " .. payload.text .. " from " .. payload.user)
    -- Leave room after receiving message (example)
    -- ws.leave.chat_lobby()
end

function ws.on.user_typing(payload)
    print(payload.user .. " is typing...")
end

function ws.on.pong(payload)
    print("Pong received at " .. payload.timestamp)
end

function ws.on.kicked(payload)
    print("Kicked from " .. payload.topic .. ": " .. payload.reason)
end

-- Error handler
function ws.on.error(err)
    print("WebSocket error: " .. err)
end

-- Send periodic ping
ws.send.ping({})

