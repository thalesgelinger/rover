---
weight: 7
title: Streaming
aliases:
  - /docs/server/streaming/
  - /docs/http-and-realtime/streaming/
---

Use streaming for progressive responses and SSE for browser-consumable live events.

## Chunked Streaming

```lua
function api.stream.chunks.get(ctx)
    local i = 0

    return api.stream(200, "text/plain", function()
        i = i + 1
        if i > 5 then
            return nil
        end
        return "chunk-" .. tostring(i) .. "\n"
    end)
end
```

Producer contract:

- return string for next chunk
- return `nil` to finish
- keep work per chunk small

## Streaming With Headers

```lua
function api.stream.json.get(ctx)
    local i = 0

    return api.stream_with_headers(200, "application/json", {
        ["Cache-Control"] = "no-store",
    }, function()
        i = i + 1
        if i == 1 then return "[" end
        if i <= 3 then return '{"n":' .. tostring(i - 1) .. '}' end
        if i == 4 then return "]" end
        return nil
    end)
end
```

## Server-Sent Events

```lua
function api.events.get(ctx)
    local sent = false

    return api.sse(function()
        if sent then
            return nil
        end
        sent = true
        return {
            event = "ready",
            id = "evt-1",
            data = {
                ok = true,
                ts = os.time(),
            },
        }
    end, 1500)
end
```

SSE producer can return:

- string for plain `data:` event
- table with `event`, `data`, `id`
- `nil` to close

`retry_ms` sets reconnect hint for clients.

## Lifecycle Notes

- long-lived streams participate in shutdown drain window
- configure `drain_timeout_secs` for deploy safety
- proxy buffering/timeouts must align with stream behavior

## Reverse Proxy Notes

- disable proxy buffering for SSE where needed
- keep read timeouts longer than expected stream duration
- test shutdown during active stream connections

## Examples

- `examples/foundation_streaming_sse.lua`
- `examples/foundation_tls_lifecycle.lua`

## Related

- [Response Builders](/docs/server/response-builders/)
- [Server Lifecycle](/docs/operations/server-lifecycle/)
- [Production Deployment](/docs/operations/production-deployment/)
