-- Environment and Config Example - Simple API
-- Access env vars directly: rover.env.MY_VAR
--
-- Run with:
--   API_KEY=secret123 DB_HOST=localhost cargo run -p rover_cli -- run examples/env_config.lua

local api = rover.server {}

-- Example 1: Direct env var access
function api.env.get(ctx)
  -- Access env vars directly as properties
  local api_key = rover.env.API_KEY
  local db_host = rover.env.DB_HOST or "localhost"
  local db_port = rover.env.DB_PORT or "5432"

  return api.json {
    api_key = api_key,
    database = {
      host = db_host,
      port = db_port,
    },
  }
end

-- Example 2: Check if env var is set
function api.check.get(ctx)
  if rover.env.DEBUG then
    return api.json {
      mode = "debug",
      debug_enabled = true,
    }
  else
    return api.json {
      mode = "production",
      debug_enabled = false,
    }
  end
end

-- Example 3: Load config from file
function api.config.get(ctx)
  local success, config = pcall(function()
    return rover.config.load("examples/test_config.lua")
  end)

  if not success then
    return api.json:status(404, {
      error = "Config file not found",
    })
  end

  return api.json {
    config = config,
  }
end

-- Example 4: Load config from env vars with prefix
-- Note: Set these env vars before running, e.g.:
--   export APP_DEBUG=true APP_API_KEY=secret123 APP_DB_HOST=db.example.com
function api.config_env.get(ctx)
  -- Load config from env vars with APP_ prefix
  local config = rover.config.from_env("APP")

  return api.json {
    config = config,
    note = "Set APP_DEBUG, APP_API_KEY, APP_DB_HOST env vars before running",
  }
end

-- Example 5: Production config pattern
function api.production.get(ctx)
  -- Direct access with defaults using Lua's or operator
  local config = {
    port = tonumber(rover.env.PORT or "3000"),
    host = rover.env.HOST or "0.0.0.0",
    log_level = rover.env.LOG_LEVEL or "info",
  }

  return api.json {
    config = config,
    note = "Set PORT, HOST, LOG_LEVEL env vars or create a .env file",
  }
end

return api
