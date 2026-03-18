-- Error Handling Middleware Example
-- Demonstrates centralized error handling
--
-- Run with:
--   cargo run -p rover_cli -- run examples/error_handling.lua
--
-- Test commands:
--   curl http://localhost:4242/divide?a=10&b=2    # Success
--   curl http://localhost:4242/divide?a=10&b=0    # Division by zero error
--   curl http://localhost:4242/divide?a=abc&b=2    # Invalid number error

local api = rover.server {}

-- Global error handler
-- Catches all errors and formats them consistently
function api.on_error(err)
  -- Log the error
  print("[ERROR] " .. err.message)
  
  -- Return formatted error response
  return api.json:status(err.status or 500, {
    error = err.message,
    code = err.code or "INTERNAL_ERROR",
    path = err.path,
    timestamp = os.time(),
  })
end

-- Divide endpoint that can throw errors
function api.divide.get(ctx)
  local query = ctx:query()
  local a = tonumber(query.a)
  local b = tonumber(query.b)
  
  -- Validation errors (400)
  if not a then
    error("Validation failed: Parameter 'a' is required and must be a number")
  end
  
  if not b then
    error("Validation failed: Parameter 'b' is required and must be a number")
  end
  
  -- Business logic error (422)
  if b == 0 then
    error("Validation failed: Division by zero is not allowed")
  end
  
  -- Success
  return api.json {
    result = a / b,
    a = a,
    b = b,
  }
end

-- Simulate internal server error
function api.crash.get(ctx)
  -- This will trigger 500 error
  error("Database connection failed")
end

return api
