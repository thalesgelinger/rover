-- Test WebSocket Multi-Room Chat
-- This script tests path parameters and topic subscriptions

local api = rover.server {}

function api.chat.p_room_id.ws(ws)
  function ws.join(ctx)
    local room_id = ctx:params().room_id
    local user_id = ctx:query().user_id or "anon"
    local name = ctx:query().name or "Guest"
    
    ws.listen("room:" .. room_id)
    
    ws.send.user_joined():to("room:" .. room_id) {
      user_id = user_id,
      name = name,
      room_id = room_id
    }
    
    return {
      user_id = user_id,
      name = name,
      room_id = room_id
    }
  end

  function ws.listen.chat(msg, ctx, state)
    if msg.text == nil or msg.text == "" then
      ws.send.error { message = "text required" }
      return
    end
    
    ws.send.chat():to("room:" .. state.room_id) {
      user_id = state.user_id,
      name = state.name,
      text = msg.text,
      timestamp = os.time()
    }
  end

  function ws.listen.typing(msg, ctx, state)
    ws.send.typing():except {
      user_id = state.user_id,
      name = state.name,
      room_id = state.room_id
    }
  end

  function ws.leave(state)
    ws.send.user_left():to("room:" .. state.room_id) {
      user_id = state.user_id,
      name = state.name
    }
  end
end

return api
