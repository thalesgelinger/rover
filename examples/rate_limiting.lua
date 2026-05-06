-- Rate Limiting Example
-- Demonstrates global and scoped rate limiting configuration
--
-- Run with:
--   cargo run -p rover_cli -- run examples/rate_limiting.lua
--
-- Test commands:
--   # Built-in health probe
--   curl http://localhost:4242/healthz
--
--   # Global rate limit (100 requests per 60 seconds by default)
--   for i in {1..110}; do curl -s http://localhost:4242/api/status | head -c 100; echo; done
--
--   # Scoped rate limit on /api/limited (10 requests per 60 seconds)
--   for i in {1..15}; do curl -s http://localhost:4242/api/limited | head -c 100; echo; done

local api = rover.server {
    -- Global rate limiting configuration
    rate_limit = {
        enabled = true,
        -- Global policy applies to all routes
        global = {
            requests_per_window = 100,  -- 100 requests
            window_secs = 60,            -- per 60 seconds
            key_header = nil,            -- use client IP (or set to "X-API-Key" for header-based)
        },
        -- Scoped policies for specific path patterns
        -- More specific routes should come first
        scoped = {
            {
                path_pattern = "/api/limited",
                requests_per_window = 10,  -- 10 requests
                window_secs = 60,          -- per 60 seconds
            },
        },
    },
}

-- Regular endpoint (subject to global rate limit)
function api.status.get(ctx)
    return api.json {
        status = "ok",
        message = "Server is running",
    }
end

-- Rate-limited endpoint (subject to both global AND scoped rate limits)
function api.limited.get(ctx)
    return api.json {
        status = "ok",
        message = "This endpoint has stricter rate limits",
    }
end

return api