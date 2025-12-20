export default `local api = rover.server {}

function api.chat.ws(ctx, ws)
    function ws.on.connect()
        print("Client connected")
    end

    function ws.join.lobby(payload)
        return { status = "ok", topic = "lobby" }
    end

    function ws.read.message(payload)
        ws.emit.lobby.message({
            user = payload.user,
            text = payload.text
        })
    end

    function ws.on.disconnect()
        print("Client disconnected")
    end
end

return api`;
