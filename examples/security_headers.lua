-- Security Headers Example
-- Demonstrates security headers via response headers field
--
-- Run with:
--   cargo run -p rover_cli -- run examples/security_headers.lua
--
-- Test:
--   curl -I http://localhost:4242/public
--   curl -I http://localhost:4242/admin

local api = rover.server {}

-- Public endpoint with basic security headers
function api.public.get(ctx)
  return api.json {
    message = "Public endpoint",
    headers = {
      ["X-Frame-Options"] = "DENY",
      ["X-Content-Type-Options"] = "nosniff",
      ["Referrer-Policy"] = "strict-origin-when-cross-origin",
    }
  }
end

-- Admin endpoint with strict security headers
function api.admin.get(ctx)
  return api.json:status(200, {
    message = "Admin panel",
    headers = {
      ["Strict-Transport-Security"] = "max-age=31536000; includeSubDomains",
      ["Content-Security-Policy"] = "default-src 'self'; script-src 'self'; style-src 'self'",
      ["X-Frame-Options"] = "DENY",
      ["X-Content-Type-Options"] = "nosniff",
      ["Referrer-Policy"] = "no-referrer",
      ["Permissions-Policy"] = "geolocation=(), microphone=(), camera=()",
    }
  })
end

-- API endpoint with CORS headers
function api.api.get(ctx)
  return api.json {
    data = { users = { "alice", "bob" } },
    headers = {
      ["Access-Control-Allow-Origin"] = "*",
      ["Access-Control-Allow-Methods"] = "GET, POST, PUT, DELETE",
      ["X-API-Version"] = "1.0",
    }
  }
end

return api
