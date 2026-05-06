-- Foundation streaming + SSE example
--
-- Run:
--   cargo run -p rover_cli -- run examples/foundation_streaming_sse.lua

local api = rover.server {
  strict_mode = true,
  security_headers = true,
}

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

function api.stream.json.get(ctx)
  local i = 0
  return api.stream_with_headers(200, "application/json", {
    ["Cache-Control"] = "no-store",
  }, function()
    i = i + 1
    if i == 1 then
      return "["
    end
    if i <= 4 then
      local comma = i > 2 and "," or ""
      return comma .. "{\"n\":" .. tostring(i - 1) .. "}"
    end
    if i == 5 then
      return "]"
    end
    return nil
  end)
end

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

return api
