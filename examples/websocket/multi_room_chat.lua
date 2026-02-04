-- Multi-Room Chat with Path Parameters and Subscriptions
-- Demonstrates: path params, topic subscriptions, targeted broadcasts.
--
-- Connect to a specific room:
--   wscat -c "ws://localhost:4242/chat/general?user_id=alice&name=Alice"
--   wscat -c "ws://localhost:4242/chat/random?user_id=bob&name=Bob"
--
-- Messages flow only within the same room.

local api = rover.server {}

function api.chat.p_room_id.ws(ws)

  function ws.join(ctx)
    local room_id = ctx:params().room_id
    local user_id = ctx:query().user_id or "anon"
    local name = ctx:query().name or "Guest"

    -- Subscribe to this room's topic
    ws.listen("room:" .. room_id)

    -- Announce to room members
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
    -- Broadcast chat message to room members
    ws.send.chat():to("room:" .. state.room_id) {
      user_id = state.user_id,
      name = state.name,
      text = msg.text,
      timestamp = os.time()
    }
  end

  function ws.listen.typing(msg, ctx, state)
    -- Notify room except sender
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

-- HTTP endpoint to check server health alongside WS
function api.health.get()
  return rover.server.json { status = "ok" }
end

return api
