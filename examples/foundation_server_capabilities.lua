-- Foundation server capabilities example
-- Covers strict defaults, management isolation, rate-limit, load-shed,
-- observability endpoints, sessions, and OpenAPI docs mount.
--
-- Run:
--   cargo run -p rover_cli -- run examples/foundation_server_capabilities.lua

local api = rover.server {
  host = "127.0.0.1",
  port = 4242,
  docs = true,
  strict_mode = true,
  security_headers = true,
  cors_origin = "http://localhost:3000",
  cors_methods = "GET, POST, OPTIONS",
  cors_headers = "Content-Type, Authorization",
  cors_credentials = true,
  body_size_limit = 1024 * 1024,
  management_prefix = "/_rover",
  allow_unauthenticated_management = false,
  rate_limit = {
    enabled = true,
    global = { requests_per_window = 120, window_secs = 60 },
    scoped = {
      { path_pattern = "/auth", requests_per_window = 12, window_secs = 60 },
    },
  },
  load_shed = {
    max_inflight = 200,
    max_queue = 100,
  },
  drain_timeout_secs = 30,
}

local sessions = rover.session.new {
  cookie_name = "foundation_session",
  ttl = 3600,
  secure = false,
  http_only = true,
  same_site = "lax",
}

-- Note: Use built-in /healthz and /readyz probes instead of custom endpoints.
-- These are automatically provided by rover.server() with proper status codes
-- and response contracts. See /docs/operations/ for details.

function api.v1.ping.get(ctx)
  return api.json {
    version = "v1",
    ok = true,
  }
end

function api.v2.ping.get(ctx)
  return api.json {
    version = "v2",
    ok = true,
  }
end

function api.auth.login.post(ctx)
  local body = ctx:body():json() or {}
  local session = sessions:create()
  session:set("user", body.user or "demo")
  session:save()

  return api.json {
    ok = true,
    cookie = session:cookie(),
    session_id = session:id(),
  }
end

function api.auth.me.p_sid.get(ctx)
  local session = sessions:get(ctx:params().sid)
  if session == nil then
    return api.json:status(404, { error = "session not found" })
  end

  local user = session:get("user")
  return api.json {
    user = user and user:as_string() or "unknown",
    state = session:state(),
  }
end

return api
