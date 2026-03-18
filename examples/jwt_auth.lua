-- JWT Authentication Example
-- Demonstrates rover.auth module for JWT tokens
--
-- Run with:
--   cargo run -p rover_cli -- run examples/jwt_auth.lua
--
-- Test commands:
--   # Create a token
--   curl -X POST http://localhost:4242/login -d '{"user_id": "123", "role": "admin"}'
--
--   # Access protected route with token
--   curl http://localhost:4242/protected -H "Authorization: Bearer <token>"

local api = rover.server {}

-- Secret key for JWT signing (use env var in production!)
local JWT_SECRET = rover.env.JWT_SECRET or "my-secret-key-change-in-production"

-- Login endpoint - creates JWT token
function api.login.post(ctx)
  local body = ctx:body():json() or {}
  
  -- Validate credentials (simplified example)
  if not body.user_id then
    error("user_id required")
  end
  
  -- Create JWT claims
  local claims = {
    sub = body.user_id,
    role = body.role or "user",
    iat = os.time(),
    exp = os.time() + 3600,  -- 1 hour expiration
  }
  
  -- Generate token
  local token = rover.auth.create(claims, JWT_SECRET)
  
  return api.json {
    token = token,
    expires_in = 3600,
  }
end

-- Protected route - requires valid JWT
function api.protected.before.auth(ctx)
  local headers = ctx:headers()
  local auth_header = headers["Authorization"]
  
  if not auth_header then
    return api.json:status(401, { error = "Missing Authorization header" })
  end
  
  -- Extract token from "Bearer <token>"
  local token = auth_header:match("Bearer%s+(.+)")
  if not token then
    return api.json:status(401, { error = "Invalid Authorization format" })
  end
  
  -- Verify token
  local result = rover.auth.verify(token, JWT_SECRET)
  
  if not result.valid then
    return api.json:status(401, { error = "Invalid token: " .. (result.error or "") })
  end
  
  -- Store user info in context for the handler
  ctx:set("user", {
    id = result.sub,
    role = result.role,
  })
end

function api.protected.get(ctx)
  local user = ctx:get("user")
  
  return api.json {
    message = "Protected resource",
    user = user,
  }
end

-- Decode endpoint - inspect token without verification
function api.decode.post(ctx)
  local body = ctx:body():json() or {}
  
  if not body.token then
    error("token required")
  end
  
  -- Decode without verification (for inspection only)
  local decoded = rover.auth.decode(body.token)
  
  return api.json(decoded)
end

return api
