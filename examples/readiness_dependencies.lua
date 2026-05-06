-- Readiness probe with dependencies example
--
-- Demonstrates:
-- - Configuring readiness dependencies
-- - Built-in /healthz and /readyz probe behavior
-- - Dependency failure responses with structured reasons
-- - Runtime dependency state updates
--
-- Run:
--   cargo run -p rover_cli -- run examples/readiness_dependencies.lua
--
-- Test the probes:
--   curl http://localhost:3000/healthz
--   curl http://localhost:3000/readyz
--
-- Simulate dependency failure:
--   curl -X POST http://localhost:3000/admin/database/fail
--   curl http://localhost:3000/readyz
--
-- Restore dependency:
--   curl -X POST http://localhost:3000/admin/database/restore
--   curl http://localhost:3000/readyz

local api = rover.server {
  host = "0.0.0.0",
  port = 3000,
  -- Configure readiness dependencies
  -- The built-in /readyz endpoint uses these to determine readiness state
  readiness = {
    dependencies = {
      database = true,
      redis = true,
      cache = true,
    },
  },
}

-- Track dependency states (in production, check actual service health)
local deps = {
  database = true,
  redis = true,
  cache = true,
}

-- Update readiness dependencies to match current state
local function sync_readiness()
  for name, healthy in pairs(deps) do
    api.config.readiness.dependencies[name] = healthy
  end
end

-- Health probe endpoint (built-in /healthz is also available)
-- Returns 200 when server is alive
function api.health.custom.get(ctx)
  return api.json {
    status = "ok",
    timestamp = os.time(),
  }
end

-- Get current dependency states
function api.admin.dependencies.get(ctx)
  return api.json {
    dependencies = deps,
    ready = api.config.readiness.dependencies,
  }
end

-- Simulate database failure
function api.admin.database.fail.post(ctx)
  deps.database = false
  sync_readiness()
  return api.json:status(503, {
    message = "Database marked as failed",
    dependency = "database",
  })
end

-- Restore database
function api.admin.database.restore.post(ctx)
  deps.database = true
  sync_readiness()
  return api.json {
    message = "Database restored",
    dependency = "database",
  }
end

-- Simulate redis failure
function api.admin.redis.fail.post(ctx)
  deps.redis = false
  sync_readiness()
  return api.json:status(503, {
    message = "Redis marked as failed",
    dependency = "redis",
  })
end

-- Restore redis
function api.admin.redis.restore.post(ctx)
  deps.redis = true
  sync_readiness()
  return api.json {
    message = "Redis restored",
    dependency = "redis",
  }
end

-- Simulate cache failure
function api.admin.cache.fail.post(ctx)
  deps.cache = false
  sync_readiness()
  return api.json:status(503, {
    message = "Cache marked as failed",
    dependency = "cache",
  })
end

-- Restore cache
function api.admin.cache.restore.post(ctx)
  deps.cache = true
  sync_readiness()
  return api.json {
    message = "Cache restored",
    dependency = "cache",
  }
end

-- Simulate multiple failures
function api.admin.fail_all.post(ctx)
  deps.database = false
  deps.redis = false
  deps.cache = false
  sync_readiness()
  return api.json:status(503, {
    message = "All dependencies marked as failed",
    dependencies = deps,
  })
end

-- Restore all dependencies
function api.admin.restore_all.post(ctx)
  deps.database = true
  deps.redis = true
  deps.cache = true
  sync_readiness()
  return api.json {
    message = "All dependencies restored",
    dependencies = deps,
  }
end

-- Example endpoint that depends on database
function api.data.get(ctx)
  if not deps.database then
    return api.json:status(503, {
      error = "Service temporarily unavailable",
      reason = "database dependency is unhealthy",
    })
  end

  return api.json {
    data = { id = 1, name = "example" },
    source = "database",
  }
end

-- Initialize
api.on_start = function()
  sync_readiness()
  print("Readiness probe example started")
  print("Dependencies: database=" .. tostring(deps.database) .. ", redis=" .. tostring(deps.redis) .. ", cache=" .. tostring(deps.cache))
  print("")
  print("Test commands:")
  print("  curl http://localhost:3000/healthz")
  print("  curl http://localhost:3000/readyz")
  print("  curl http://localhost:3000/admin/dependencies")
  print("")
  print("Simulate failures:")
  print("  curl -X POST http://localhost:3000/admin/database/fail")
  print("  curl -X POST http://localhost:3000/admin/redis/fail")
  print("  curl -X POST http://localhost:3000/admin/cache/fail")
  print("  curl -X POST http://localhost:3000/admin/fail_all")
  print("")
  print("Restore dependencies:")
  print("  curl -X POST http://localhost:3000/admin/database/restore")
  print("  curl -X POST http://localhost:3000/admin/restore_all")
end

return api
