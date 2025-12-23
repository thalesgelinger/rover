local api = rover.server {}

function api.chat.ws(ctx, ws)
    function ws.on.open()
        print("connected")
    end

    function ws.on.message(msg)
        ws.send("echo: " .. msg)
    end

    function ws.on.close()
        print("bye")
    end
end

return api
