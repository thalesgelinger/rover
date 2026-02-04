-- WebSocket Chat Server
-- Simple chat room where all connected clients see each other's messages.
--
-- Connect with any WebSocket client:
--   wscat -c ws://localhost:4242/chat
--
-- Messages:
--   Send: {"type":"chat","text":"hello everyone!"}
--   Recv: {"type":"chat","user_id":"anon","text":"hello everyone!","timestamp":1234567890}
--
--   Recv (on connect):  {"type":"user_joined","user_id":"anon","timestamp":1234567890}
--   Recv (on leave):    {"type":"user_left","user_id":"anon"}

local api = rover.server {}

function api.chat.ws(ws)

  function ws.join(ctx)
    local user_id = ctx:query().user_id or "anon"

    -- Announce to everyone
    ws.send.user_joined():all {
      user_id = user_id,
      timestamp = os.time()
    }

    -- Return state (threaded to listen handlers and leave)
    return { user_id = user_id }
  end

  function ws.listen.chat(msg, ctx, state)
    -- Broadcast to all connected clients
    ws.send.chat():all {
      user_id = state.user_id,
      text = msg.text,
      timestamp = os.time()
    }
  end

  function ws.listen.typing(msg, ctx, state)
    -- Notify everyone except the sender
    ws.send.typing():except {
      user_id = state.user_id
    }
  end

  function ws.leave(state)
    ws.send.user_left():all {
      user_id = state.user_id
    }
  end

end

return api
