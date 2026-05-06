-- Permissions Example - Production-safe capability configuration
-- Demonstrates restrictive production permissions and allowed capability paths.
--
-- Run:
--   cargo run -p rover_cli -- run examples/permissions_example.lua
--
-- See: /docs/guides/permissions for full documentation.

-- Example 1: Development mode (permissive defaults)
-- Development mode allows fs, net, env by default; denies process, ffi.
-- Use for local development only.
local dev_api = rover.server {
    permissions = {
        mode = "development",
    },
}

-- Example 2: Production mode (deny-by-default)
-- Production mode denies all capabilities by default.
-- Explicitly allow only what your app needs.
local api = rover.server {
    host = "127.0.0.1",
    port = 4242,
    permissions = {
        mode = "production",
        allow = { "env" },
    },
}

-- Example 3: Production with multiple allowed capabilities
-- Allow specific capabilities for apps that need them.
local api_with_net = rover.server {
    host = "127.0.0.1",
    port = 4243,
    permissions = {
        mode = "production",
        allow = { "env", "net" },
    },
}

-- Example 4: Explicit deny (always wins over allow)
-- Use deny to enforce hard boundaries even in development.
local api_strict = rover.server {
    host = "127.0.0.1",
    port = 4244,
    permissions = {
        mode = "development",
        allow = { "process" },
        deny = { "process", "ffi" },
    },
}

-- Example 5: Minimal production permissions
-- Most restrictive: only env access, no filesystem, no network, no process.
function api.status.get(ctx)
    local env_name = rover.env.ROVER_ENV or "development"

    return api.json {
        status = "healthy",
        environment = env_name,
        permissions = {
            mode = "production",
            allowed = { "env" },
        },
    }
end

-- Example 6: Accessing environment variables (allowed)
-- This works because "env" is in the allow list.
function api.config.get(ctx)
    local db_host = rover.env.DB_HOST or "localhost"
    local db_port = tonumber(rover.env.DB_PORT or "5432")
    local log_level = rover.env.LOG_LEVEL or "info"

    return api.json {
        database = {
            host = db_host,
            port = db_port,
        },
        logging = {
            level = log_level,
        },
    }
end

-- Example 7: Process execution (denied by default in production)
-- This would fail with permission denied error because "process" is not allowed.
-- Uncomment to see the error:
--
-- function api.date.get(ctx)
--     local pipe = io.popen("date", "r")
--     local out = pipe:read("*a")
--     pipe:close()
--     return api.text(out)
-- end

-- Example 8: Demonstrating permission error handling
-- Shows how to handle permission-denied errors gracefully.
function api.safe_exec.get(ctx)
    local success, result = pcall(function()
        local pipe = io.popen("echo 'test'", "r")
        if pipe then
            local out = pipe:read("*a")
            pipe:close()
            return out
        end
        return nil
    end)

    if not success then
        return api.json:status(500, {
            error = "Process execution not permitted",
            hint = "Add 'process' to allow list if needed",
        })
    end

    return api.json {
        result = result,
    }
end

-- Example 9: Production server with process permission
-- For apps that legitimately need process execution.
local api_with_process = rover.server {
    host = "127.0.0.1",
    port = 4245,
    permissions = {
        mode = "production",
        allow = { "env", "process" },
    },
    strict_mode = true,
}

function api_with_process.date.get(ctx)
    local pipe = io.popen("date", "r")
    local out = pipe:read("*a")
    pipe:close()
    return api_with_process.text(out)
end

return api