-- Test WebSocket Chat Server
-- This script tests state management and broadcasting

local api = rover.server {}

function api.chat.ws(ws)
  function ws.join(ctx)
    ws.send.welcome {
      message = "connected",
      timestamp = os.time()
    }
    return { user_id = "anon" }
  end

  function ws.listen.identify(msg, ctx, state)
    local user_id = msg.user_id or "anon"
    
    ws.send.user_joined():all {
      user_id = user_id,
      timestamp = os.time()
    }
    
    return { user_id = user_id }
  end

  function ws.listen.chat(msg, ctx, state)
    if msg.text == nil or msg.text == "" then
      ws.send.error { message = "text required" }
      return
    end
    
    ws.send.chat():all {
      user_id = state.user_id or "anon",
      text = msg.text,
      timestamp = os.time()
    }
  end

  function ws.listen.message(msg, ctx, state)
    local kind = msg.type
    if kind == "identify" then
      return ws.listen.identify(msg, ctx, state)
    end
    if kind == "chat" then
      return ws.listen.chat(msg, ctx, state)
    end
  end

  function ws.leave(state)
    ws.send.user_left():all {
      user_id = state.user_id or "anon",
      timestamp = os.time()
    }
  end
end

return api
