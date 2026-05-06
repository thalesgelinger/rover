-- Test WebSocket Echo Server
-- This script tests the basic echo functionality

local api = rover.server {}

function api.echo.ws(ws)
  function ws.join(ctx)
    ws.send.welcome { message = "Connected to echo server" }
    return {}
  end

  function ws.listen.echo(msg, ctx, state)
    ws.send.echo { text = msg.text }
  end

  function ws.listen.message(msg, ctx, state)
    ws.send.echo(msg)
  end
end

return api
