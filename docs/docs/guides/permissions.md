---
sidebar_position: 9
---

# Permissions

Rover permissions control runtime capabilities for your Lua app.

Use `permissions` in `rover.server { ... }` to choose mode and explicit allow/deny lists.

## Permission Model

Available permissions:

| Permission | Controls | Risk Level |
|------------|----------|------------|
| `fs` | Filesystem access (read/write files) | High - can access sensitive data |
| `net` | Network access (outbound HTTP) | Medium - can exfiltrate data |
| `env` | Environment variable access | Low - depends on what's in env |
| `process` | Child process execution (`io.popen`) | Very High - arbitrary code execution |
| `ffi` | Foreign function interface | Very High - native code execution |

Mode defaults:

| Mode | Default behavior |
|------|------------------|
| `development` | `fs`, `net`, `env` allowed; `process`, `ffi` denied |
| `production` | deny-by-default (all denied unless explicitly allowed) |

You can use mode aliases:

- `development` or `dev`
- `production` or `prod`

## Configuration Examples

### Development Defaults

```lua
local api = rover.server {
    permissions = {
        mode = "development",
    },
}

return api
```

### Production With Minimal Allow List

Most secure approach: allow only what you need.

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env" },
    },
}

return api
```

### Production With Network Access

For apps that need to make outbound HTTP requests.

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env", "net" },
    },
}

return api
```

### Explicitly Deny a Permission

`deny` always wins if the same permission appears in both lists.

```lua
local api = rover.server {
    permissions = {
        mode = "development",
        allow = { "process" },
        deny = { "process" },
    },
}

return api
```

### Safe Process Execution Enablement

`io.popen` requires `process` permission. Use sparingly.

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env", "process" },
    },
}

function api.run.get(ctx)
    local pipe = io.popen("date", "r")
    local out = pipe:read("*a")
    pipe:close()
    return api.text(out)
end

return api
```

## Permission Details

### `fs` - Filesystem Access

Controls: Reading and writing files via Lua's `io` library.

When denied: File operations return permission errors.

When allowed: Full filesystem access within OS permissions.

```lua
-- Requires fs permission (default in development)
local file = io.open("data.json", "r")
local content = file:read("*a")
file:close()
```

### `net` - Network Access

Controls: Outbound HTTP requests via `rover.http` or `http` module.

When denied: Network requests fail with permission errors.

When allowed: HTTP requests to any allowed destinations.

```lua
-- Requires net permission (default in development)
local response = rover.http.get("https://api.example.com/data")
```

### `env` - Environment Variables

Controls: Access to environment variables via `rover.env`.

When denied: Environment variables return `nil`.

When allowed: Full access to process environment.

```lua
-- Requires env permission (default in development)
local db_host = rover.env.DB_HOST or "localhost"
local api_key = rover.env.API_KEY
```

### `process` - Child Process Execution

Controls: Spawning child processes via `io.popen`.

When denied: Process spawning fails with permission error.

When allowed: Arbitrary command execution. **Use with extreme caution.**

```lua
-- Requires process permission (DENIED by default)
local handle = io.popen("ls -la", "r")
local output = handle:read("*a")
handle:close()
```

### `ffi` - Foreign Function Interface

Controls: Loading native libraries via LuaJIT FFI.

When denied: FFI operations fail.

When allowed: Direct native code execution. **Reserved for future use.**

Currently implemented as a permission key but runtime enforcement is in development.

## Production Guidance

### Principle of Least Privilege

Start with production mode and add only required capabilities:

```lua
-- Good: Minimal surface area
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env" },
    },
}

-- Avoid: Overly permissive for production
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "fs", "net", "env", "process" },
    },
}
```

### Development vs Production Configs

Use environment-aware configuration:

```lua
local mode = rover.env.ROVER_ENV or "development"
local allow_list = {}

if mode == "production" then
    allow_list = { "env" }
end

local api = rover.server {
    permissions = {
        mode = mode,
        allow = allow_list,
    },
}
```

### Process Permission Security

If you must enable `process`:

1. Validate all inputs before passing to shell
2. Prefer command whitelisting over arbitrary execution
3. Audit all process execution in logs
4. Consider if a native API would work instead

```lua
-- Dangerous: User input in command
local cmd = "ls " .. user_input
local handle = io.popen(cmd, "r")

-- Better: Sanitized command
local sanitized = user_input:gsub("[^%w%-_/]", "")
local cmd = "ls " .. sanitized
```

### Audit Logging

Permission violations emit structured audit events. Check logs for:

```
[AUDIT] permission_denied permission=process operation=io.popen
```

These events help identify:

- Attempted capability escapes
- Misconfigured permissions
- Security incidents

## Limitations

Current enforcement boundaries:

| Permission | Enforcement Status |
|------------|-------------------|
| `process` | Fully enforced - `io.popen` blocked |
| `fs` | Parsed, enforcement in development |
| `net` | Parsed, enforcement in development |
| `env` | Parsed, enforcement in development |
| `ffi` | Reserved for future enforcement |

Additional limitations:

- Permissions are configured at startup; no runtime mutation API
- Ambiguous configs (same permission in `allow` and `deny`) fail startup
- Invalid permission names fail startup with validation error
- Mode defaults cannot be modified; use explicit lists

## Validation Errors

Rover rejects invalid permission values with clear startup errors.

Invalid permission name:

```lua
local api = rover.server {
    permissions = {
        allow = { "invalid_perm" },
    },
}
```

Fails with:

```text
permissions.allow contains invalid permission 'invalid_perm'; valid values are: fs, net, env, process, ffi
```

Ambiguous configuration:

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env" },
        deny = { "env" },
    },
}
```

Fails with:

```text
permissions contains ambiguous permissions that appear in both allow and deny: env
```

## Runnable Example

See [`examples/permissions_example.lua`](https://github.com/anomaly/rover/tree/main/examples/permissions_example.lua) for a complete working example demonstrating:

- Development mode defaults
- Production deny-by-default
- Explicit allow lists
- Permission error handling
- Safe process execution pattern

Run with:

```bash
cargo run -p rover_cli -- run examples/permissions_example.lua
```

## See Also

- [Configuration](/docs/api-reference/configuration) - Full server configuration options
- [Production Deployment](/docs/guides/production-deployment) - Production best practices