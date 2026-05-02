---
sidebar_position: 1
---

# Configuration

Configure your Rover server with custom options, environment variables, and external config files.

## Server Options

Pass configuration options to `rover.server`:

```lua
local api = rover.server {
    host = "127.0.0.1",       -- default: "localhost"
    port = 3000,              -- default: 4242
    log_level = "debug",     -- default: "debug" ("debug" | "info" | "warn" | "error" | "nope")
    docs = false,             -- default: false (enable OpenAPI docs)
    strict_mode = true,       -- default: true
    allow_public_bind = false,
    allow_wildcard_cors_credentials = false,
    allow_unbounded_body = false,
    security_headers = true,
    allow_insecure_security_header_overrides = false,
    cors_origin = "*",       -- optional
    cors_methods = "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD",
    cors_headers = "Content-Type, Authorization",
    cors_credentials = false
}

function api.hello.get(ctx)
    return { message = "Hello!" }
end

return api
```

## TLS Configuration

Enable HTTPS with TLS certificate configuration:

```lua
local api = rover.server {
    tls = {
        cert_file = "/path/to/cert.pem",
        key_file = "/path/to/key.pem",
        reload_interval_secs = 3600  -- Optional: auto-reload interval
    }
}
```

### `tls.cert_file`

- **Type**: `string`
- **Required**: Yes (when TLS is enabled)
- **Description**: Path to TLS certificate file (PEM format)

### `tls.key_file`

- **Type**: `string`
- **Required**: Yes (when TLS is enabled)
- **Description**: Path to TLS private key file (PEM format)

### `tls.reload_interval_secs`

- **Type**: `number`
- **Default**: `1` (1 second)
- **Description**: Interval in seconds to check for certificate file changes. Set to enable hot reload of TLS certificates without server restart.

**Important**: Hot reload is **only supported for TLS certificates**. Changes to routes, middleware, server configuration, or Lua code require a server restart.

Example with 1-hour reload interval:

```lua
local api = rover.server {
    tls = {
        cert_file = "/etc/ssl/certs/server.crt",
        key_file = "/etc/ssl/private/server.key",
        reload_interval_secs = 3600  -- Check every hour
    }
}
```

## Configuration Reference

### `host`

- **Type**: `string`
- **Default**: `"localhost"`
- **Description**: The host address to bind the server to

Example:

```lua
rover.server {
    host = "0.0.0.0"  -- Listen on all interfaces
}
```

### `port`

- **Type**: `number`
- **Default**: `4242`
- **Description**: The port number to listen on

Example:

```lua
rover.server {
    port = 8080
}
```

### `log_level`

- **Type**: `string`
- **Default**: "debug"
- **Options**: "debug", "info", "warn", "error", "nope"
- **Description**: Set the logging verbosity level


Example:

```lua
rover.server {
    log_level = "debug"  -- Show all logs including debug messages
}
```

### `docs`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Enable OpenAPI docs at `/docs`

### `strict_mode`

- **Type**: `boolean`
- **Default**: `true`
- **Description**: Enforce strict secure startup checks

### `allow_public_bind`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict host binding checks and allow non-loopback host values

### `allow_wildcard_cors_credentials`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict CORS checks and allow `cors_origin = "*"` with `cors_credentials = true`

### `allow_unbounded_body`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict body-size checks and allow `body_size_limit = 0`

### `security_headers`

- **Type**: `boolean`
- **Default**: `true`
- **Description**: Apply secure default response headers (`X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`)

### `allow_insecure_security_header_overrides`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict header override checks and allow unsafe values for protected security headers

Example:

```lua
rover.server {
    docs = false  -- Disable docs endpoint
}
```

### `cors_origin`

- **Type**: `string`
- **Default**: `nil` (CORS disabled)
- **Description**: Allowed CORS origin, e.g. `"*"` or `"https://app.example.com"`

### `cors_methods`

- **Type**: `string`
- **Default**: `"GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD"`
- **Description**: Value for `Access-Control-Allow-Methods`

### `cors_headers`

- **Type**: `string`
- **Default**: `"Content-Type, Authorization"`
- **Description**: Value for `Access-Control-Allow-Headers`

### `cors_credentials`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Sets `Access-Control-Allow-Credentials: true` when enabled

## Permissions Configuration

Configure runtime permissions with `permissions.mode`, `permissions.allow`, and `permissions.deny`:

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env" },
        deny = { "process" },
    },
}
```

For complete examples, production guidance, and current enforcement limitations, see [Permissions](/docs/guides/permissions).

## Complete Example

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 8080,
    log_level = "info",
    allow_public_bind = true,
    docs = true
}

function api.health.get(ctx)
    return api.text("OK")
end

return api
```

This configuration will:
- Listen on all network interfaces (`0.0.0.0`)
- Use port 8080
- Show info-level logs and above
- Expose OpenAPI docs at `/docs`

## Environment Variables

Rover provides direct access to environment variables via `rover.env`.

### Loading .env Files

Rover automatically loads `.env` files from your project root on startup. Create a `.env` file:

```bash
# .env
API_KEY=your-secret-key
DB_HOST=localhost
DB_PORT=5432
DEBUG=true
```

### Direct Access

Access environment variables directly as properties:

```lua
-- Get env var (returns nil if not set)
local api_key = rover.env.API_KEY
local db_host = rover.env.DB_HOST

-- With default using Lua's or operator
local port = rover.env.PORT or "3000"
local host = rover.env.HOST or "localhost"

-- Check if set
if rover.env.DEBUG then
    -- Enable debug mode
end
```

### Production Best Practices

```lua
local api = rover.server {}

function api.config.get(ctx)
    -- Direct access with defaults using Lua's or operator
    local config = {
        port = tonumber(rover.env.PORT or "3000"),
        host = rover.env.HOST or "0.0.0.0",
        log_level = rover.env.LOG_LEVEL or "info",
    }
    
    -- Check if required var is set
    if not rover.env.API_KEY then
        return api.error(500, "API_KEY not configured")
    end
    
    return api.json {
        config = config,
        has_api_key = true,
    }
end

return api
```

## Config Files

### `rover.config.load(path)`

Load configuration from a Lua file:

```lua
-- config.lua
return {
    database = {
        host = "localhost",
        port = 5432,
        name = "myapp"
    },
    features = {
        "auth",
        "websocket"
    }
}
```

```lua
-- app.lua
local api = rover.server {}

local config = rover.config.load("config.lua")

function api.db.host.get(ctx)
    return api.json {
        host = config.database.host
    }
end

return api
```

### `rover.config.from_env(prefix)`

Load nested configuration from environment variables with a prefix:

```bash
# .env
MYAPP_DEBUG=true
MYAPP_API_KEY=secret123
MYAPP_DATABASE_HOST=db.example.com
MYAPP_DATABASE_PORT=3306
```

```lua
local config = rover.config.from_env("MYAPP")
-- Results in:
-- config.debug = "true"
-- config.api_key = "secret123"
-- config.database.host = "db.example.com"
-- config.database.port = "3306"
```

## Complete Environment Example

```lua
local api = rover.server {
    port = tonumber(rover.env.PORT or "4242"),
    host = rover.env.HOST or "localhost",
    log_level = rover.env.LOG_LEVEL or "debug",
}

-- Load external config
local db_config = rover.config.load("database.lua")

function api.health.get(ctx)
    return api.json {
        status = "healthy",
        db_host = db_config.host,
        environment = rover.env.ROVER_ENV or "development",
    }
end

return api
```

This example demonstrates:
- Server configuration from environment variables
- Loading external config files
- Safe defaults with Lua's `or` operator
- Runtime environment detection
