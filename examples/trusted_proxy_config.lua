-- Trusted proxy configuration example
-- Demonstrates secure handling of forwarded headers behind reverse proxies
--
-- Run:
--   cargo run -p rover_cli -- run examples/trusted_proxy_config.lua
--
-- Test with curl (simulating trusted proxy):
--   curl -H "X-Forwarded-For: 203.0.113.10" \
--        -H "X-Forwarded-Proto: https" \
--        http://127.0.0.1:4242/client-info
--
-- The server will only trust forwarded headers when the immediate
-- connection source matches the configured trusted_proxies.

local api = rover.server {
  host = "127.0.0.1",
  port = 4242,
  log_level = "info",
  docs = true,
  -- Trust proxies in the 10.0.0.0/8 private range
  -- In production, set this to your actual proxy/load balancer subnet
  trusted_proxies = { "10.0.0.0/8" },
  -- Also supports range and table formats:
  -- trusted_proxies = { "10.0.0.1-10.0.0.255" },
  -- trusted_proxies = {
  --   { cidr = "10.0.0.0/8" },
  --   { start = "172.16.0.10", to = "172.16.0.20" },
  -- },
}

-- Return client connection information
-- When trusted, this includes forwarded client IP and protocol
function api.client_info.get(ctx)
  local headers = ctx:headers()
  
  -- rover.context automatically derives client info from
  -- Forwarded / X-Forwarded-* headers when proxy is trusted
  return api.json {
    -- Direct connection info (always available)
    remote_addr = ctx.remote_addr,
    
    -- Protocol (http or https) - derived from X-Forwarded-Proto
    -- when behind trusted proxy, otherwise "http"
    protocol = ctx.protocol or "http",
    
    -- All headers for debugging
    headers = {
      forwarded = headers.forwarded,
      x_forwarded_for = headers["x-forwarded-for"],
      x_forwarded_proto = headers["x-forwarded-proto"],
    },
    
    -- Trust status indicator
    note = "Forwarded headers only processed from trusted proxy sources",
  }
end

-- Health check endpoint
function api.health.get(ctx)
  return api.json { status = "ok" }
end

return api
