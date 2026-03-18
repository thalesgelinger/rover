export default `local api = rover.server {}

function api.chat.ws(ws)
    function ws.join(ctx)
        ws.send.connected { message = "connected" }
        return {}
    end

    function ws.listen.echo(msg, ctx, state)
        ws.send.echo(msg)
    end

    function ws.leave(state)
    end
end

return api`;
