-- Basic WebSocket Echo Server (updated to new DSL)
-- See examples/websocket/ folder for more WebSocket examples.

local api = rover.server {}

function api.chat.ws(ws)

  function ws.join(ctx)
    ws.send.connected { message = "WebSocket connection established" }
    return {}
  end

  function ws.listen.message(msg, ctx, state)
    -- Echo the received message back
    ws.send.echo(msg)
  end

  function ws.leave(state)
    -- Connection closed
  end

end

return api
