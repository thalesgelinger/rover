---
weight: 1
title: Configuration
---

Configure your Rover server with custom options.

## Server Options

Pass configuration options to `rover.server`:

```lua
local api = rover.server {
    host = "127.0.0.1",       -- default: "localhost"
    port = 3000,              -- default: 4242
    log_level = "debug",     -- default: "debug" ("debug" | "info" | "warn" | "error" | "nope")
    docs = true               -- default: true (enable OpenAPI docs)
}

function api.hello.get(ctx)
    return { message = "Hello!" }
end

return api
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
- **Default**: `true`
- **Description**: Enable OpenAPI docs at `/docs`

Example:

```lua
rover.server {
    docs = false  -- Disable docs endpoint
}
```

## Complete Example

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 8080,
    log_level = "info",
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
