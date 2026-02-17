local ws = rover.ws_client("ws://localhost:4242/echo")

ws.join = function(ctx)
  ws.send.echo({ text = "hello from rover.ws_client" })
  return { sent = 1 }
end

ws.listen.echo = function(msg, ctx, state)
  print("echo:", msg.text)
  return { sent = state.sent }
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
