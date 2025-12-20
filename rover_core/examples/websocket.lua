local api = rover.server {}

function api.chat.ws(ctx, ws)
    local connection_id = ws.id -- Connection ID for sending to specific clients
    
    -- Lifecycle handlers
    function ws.on.connect()
        print("Client connected: " .. connection_id)
    end

    function ws.on.disconnect(code, reason)
        print("Client disconnected: " .. code .. " - " .. reason)
    end

    -- Join handlers for specific topics (return auto-triggers client's ws.on.join_ack)
    function ws.join.chat_lobby(payload)
        print("Joining chat:lobby")
        -- Authorize based on payload
        if not payload.user then
            return { status = "error", reason = "User required" }
        end
        return { status = "ok", topic = "chat_lobby", user_id = payload.user }
    end

    -- Leave handler (return auto-triggers client's ws.on.leave_ack)
    function ws.leave.chat_lobby(payload)
        print("Leaving chat:lobby")
        return { status = "ok", topic = "chat_lobby" }
    end

    -- Incoming event handlers
    function ws.read.new_msg(payload)
        print("New message: " .. payload.text)
        -- Broadcast to topic
        ws.emit.chat_lobby.new_msg({
            user = payload.user,
            text = payload.text,
            timestamp = os.time()
        })
    end

    function ws.read.user_typing(payload)
        ws.emit.chat_lobby.user_typing({ user = payload.user })
    end

    function ws.read.ping(payload)
        ws.reply.pong({ timestamp = os.time() })
    end

    function ws.read.dm(payload)
        -- Send to specific user
        local target_conn_id = payload.target_connection_id
        ws.send_to(target_conn_id).private_msg({
            from = payload.from,
            text = payload.text
        })
    end

    function ws.read.report_user(payload)
        -- Kick reported user
        local reported_conn_id = payload.reported_connection_id
        ws.send_to(reported_conn_id).kicked({
            topic = "chat_lobby",
            reason = "Reported by moderator"
        })
    end

    -- Outgoing event handlers (for filtering/transforming)
    function ws.map.new_msg(payload)
        payload.server_time = os.time()
        return payload
    end

    -- Terminate handler
    function ws.terminate(reason)
        print("WebSocket terminated: " .. reason)
    end
end

-- Binary WebSocket with functional event handling
function api.binary.ws(ctx, ws)
    function ws.on.connect()
        print("Binary client connected")
    end

    function ws.on.disconnect(code, reason)
        print("Binary client disconnected")
    end

    function ws.join.binary_stream(payload)
        return { status = "ok" }
    end

    function ws.read.binary_data(payload)
        ws.emit.binary_stream.binary_update(payload)
    end
end

return api

