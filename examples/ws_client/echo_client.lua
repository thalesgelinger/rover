local ws = rover.ws_client "ws://localhost:4242/echo"

function ws.join(ctx)
	ws.send.echo { text = "hello from rover.ws_client" }
	return { sent = 1 }
end

function ws.listen.echo(msg, ctx, state)
	print("echo:", msg.text)
	return { sent = state.sent }
end

function ws.error(err, ctx, state)
	print("ws error:", err.message)
end

function ws.leave(info, state)
	print("closed:", info.code, info.reason)
end

ws:connect()

while ws:is_connected() do
	ws:pump(16)
end
