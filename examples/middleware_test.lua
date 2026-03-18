-- Middleware Test - Using function-based DSL
-- This matches the working rest_api_basic.lua style

local api = rover.server {}

-- Global before middleware
function api.before.log(ctx)
  print("[BEFORE LOG] Request started")
  ctx:set("start_time", os.time())
end

-- Global after middleware
function api.after.log(ctx)
  local start_time = ctx:get("start_time")
  if start_time then
    print("[AFTER LOG] Request completed in " .. (os.time() - start_time) .. "s")
  end
end

-- Public route (no middleware protection)
function api.public.get(ctx)
  return api.json {
    message = "This is public",
  }
end

-- Protected route - with auth middleware
function api.protected.before.auth(ctx)
  local token = ctx:headers()["Authorization"]
  if not token then
    return api.json:status(401, { error = "Unauthorized" })
  end
  ctx:set("user", { id = 1, name = "admin" })
end

function api.protected.get(ctx)
  return api.json {
    message = "Protected resource",
    user = ctx:get("user"),
  }
end

-- Using ctx:set and ctx:get
function api.counter.before.auth(ctx)
  local token = ctx:headers()["Authorization"]
  if not token then
    return api.json:status(401, { error = "Unauthorized" })
  end
end

function api.counter.get(ctx)
  local count = ctx:get("count") or 0
  count = count + 1
  ctx:set("count", count)
  
  return api.json {
    count = count,
  }
end

return api
