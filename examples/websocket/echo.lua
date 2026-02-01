-- WebSocket Echo Server
-- Simplest possible WebSocket endpoint: echoes messages back to the sender.
--
-- Connect with any WebSocket client:
--   wscat -c ws://localhost:4242/echo
--   Send: {"type":"echo","text":"hello"}
--   Recv: {"type":"echo","text":"hello"}

local api = rover.server {}

function api.echo.ws(ws)

  function ws.join(ctx)
    ws.send.welcome { message = "Connected to echo server" }
    return {}
  end

  function ws.listen.echo(msg, ctx, state)
    -- Echo the message back to the sender
    ws.send.echo { text = msg.text }
  end

  -- Catch-all: echo any untyped message
  function ws.listen.message(msg, ctx, state)
    ws.send.echo(msg)
  end

end

return api
