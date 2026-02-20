local ws = rover.ws_client("ws://localhost:4242/echo", {
  reconnect = {
    enabled = false,
    min_ms = 250,
    max_ms = 10000,
    factor = 2.0,
    jitter = true,
    max_attempts = 0,
  },
})

ws.join = function(ctx)
  print("connected", ctx.url)
  ws.send.echo({ text = "ping" })
  return { retries = 0 }
end

ws.listen.echo = function(msg, ctx, state)
  print("recv", msg.text)
end

ws.error = function(err, ctx, state)
  print("error", err.message)
end

ws.leave = function(info, state)
  print("leave", info.code, info.reason)
end

ws:connect()
while ws:is_connected() do
  ws:pump(16)
end
