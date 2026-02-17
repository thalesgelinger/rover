local ws = rover.ws_client("ws://localhost:4242/chat?user_id=cli")

ws.join = function(ctx)
  ws.send.chat({ text = "hello room" })
  return { joined_at = os.time() }
end

ws.listen.chat = function(msg, ctx, state)
  print("[chat]", msg.user_id, msg.text)
end

ws.listen.user_joined = function(msg, ctx, state)
  print("[join]", msg.user_id)
end

ws.listen.user_left = function(msg, ctx, state)
  print("[left]", msg.user_id)
end

ws.error = function(err, ctx, state)
  print("ws error:", err.message)
end

ws.leave = function(info, state)
  print("closed:", info.code, info.reason)
end

ws:connect()
while ws:is_connected() do
  ws:pump(16)
end
