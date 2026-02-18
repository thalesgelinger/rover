---
weight: 4
title: WebSocket Client
---

Rover exposes a WebSocket client at `rover.ws_client(url, opts?)`.

## Quick Start

```lua
local ws = rover.ws_client("ws://localhost:4242/echo")

ws.join = function(ctx)
  ws.send.echo({ text = "hello" })
  return {}
end

ws.listen.echo = function(msg, ctx, state)
  print(msg.text)
end

ws.error = function(err, ctx, state)
  print("ws error", err.message)
end

ws:connect()
while ws:is_connected() do
  ws:pump(16)
end
```

## Lifecycle Methods

- `ws:connect()`
- `ws:pump(timeout_ms?)`
- `ws:run()`
- `ws:close(code?, reason?)`
- `ws:is_connected()`

## DSL Handlers

- `ws.join = function(ctx) ... end`
- `ws.listen.<event> = function(msg, ctx, state) ... end`
- `ws.listen.message = function(msg, ctx, state) ... end` (fallback)
- `ws.leave = function(info, state) ... end`
- `ws.error = function(err, ctx, state) ... end`

If a handler returns non-`nil`, it replaces connection `state`.

## Sending Messages

Typed event send:

```lua
ws.send.chat({ text = "hi" })
-- -> {"type":"chat","text":"hi"}
```

Typed payload must be a table.

Raw send methods:

- `ws:send_text(text)`
- `ws:send_binary(bytes)`
- `ws:ping(payload?)`

## Options

`opts` supports:

- `headers = { ... }`
- `protocols = { "chat.v1" }`
- `handshake_timeout_ms = 10000`
- `max_message_bytes = 4194304`
- `auto_pong = true`
- `reconnect = { enabled=false, min_ms=250, max_ms=10000, factor=2.0, jitter=true, max_attempts=0 }`
- `tls = { roots="bundled", ca_file=nil, insecure=false, pin_sha256={} }`

### Notes

- If `protocols` is provided, handshake enforces server-selected subprotocol.
- Advanced TLS options are parsed but not fully implemented yet.

## Valid Samples

- `examples/ws_client/echo_client.lua`
- `examples/ws_client/chat_client.lua`
- `examples/ws_client/reconnect_client.lua`
