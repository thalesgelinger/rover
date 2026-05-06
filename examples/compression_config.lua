-- Compression configuration example
-- Demonstrates gzip and deflate response compression
--
-- Run:
--   cargo run -p rover_cli -- run examples/compression_config.lua
--
-- Test with curl:
--   curl -H "Accept-Encoding: gzip" http://localhost:4242/api/data --compressed -v
--   curl -H "Accept-Encoding: deflate" http://localhost:4242/api/data --compressed -v
--   curl -H "Accept-Encoding: br" http://localhost:4242/api/data -v  # Should get uncompressed (brotli not supported)

local api = rover.server {
  host = "127.0.0.1",
  port = 4242,
  log_level = "info",
  compress = {
    enabled = true,
    algorithms = { "gzip", "deflate" },  -- Only gzip and deflate are supported
    min_size = 100,                       -- Compress responses >= 100 bytes
    types = {                             -- Only compress these content types
      "application/json",
      "text/plain",
      "text/html",
    },
  },
}

-- Large JSON response - will be compressed
function api.api.data.get(ctx)
  local data = {
    message = "This response will be compressed if client supports gzip or deflate",
    timestamp = os.time(),
    items = {},
  }
  
  -- Generate enough data to trigger compression (> min_size)
  for i = 1, 50 do
    table.insert(data.items, {
      id = i,
      name = "Item " .. i,
      description = "This is a sample item description that adds more bytes to the response",
    })
  end
  
  return api.json(data)
end

-- Small response - won't be compressed due to min_size
function api.api.small.get(ctx)
  return api.json({ message = "Hi" })
end

-- Text response - will be compressed
function api.api.text.get(ctx)
  return api.text("This is a text response that will be compressed. " ..
    "It contains enough content to exceed the minimum size threshold. " ..
    "Compression reduces bandwidth usage and improves response times.")
end

return api
