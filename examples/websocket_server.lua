local api = rover.server {}

-- WebSocket endpoint with lifecycle handlers
function api.chat.ws(ctx, ws)
    function ws.on.open()
        print("WebSocket connection established")
    end

    function ws.on.message(msg)
        -- Echo the received message back
        ws.send("echo: " .. msg)
    end

    function ws.on.close()
        print("WebSocket connection closed")
    end
end

return api
