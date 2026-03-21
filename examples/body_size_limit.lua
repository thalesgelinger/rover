-- Body Size Limits Example
-- Demonstrates body size limit configuration
--
-- Run with:
--   cargo run -p rover_cli -- run examples/body_size_limit.lua
--
-- Test commands:
--   # Built-in health probe (no body expected)
--   curl http://localhost:4242/healthz
--
--   curl -X POST -d "small body" http://localhost:4242/echo
--   curl -X POST -d "$(python3 -c 'print(\"x\" * 10000)')" http://localhost:4242/echo

local api = rover.server {
    -- Set body size limit to 1KB (1024 bytes)
    -- Requests with bodies larger than this will get 413 error
    body_size_limit = 1024,
}

-- Echo endpoint that echoes back the request body
function api.echo.post(ctx)
  local body = ctx:body():text() or ""
  return api.json {
    received = body,
    length = #body,
    message = "Body received successfully",
  }
end

return api
