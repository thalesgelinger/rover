---
weight: 7
title: Server Lifecycle
aliases:
  - /docs/server/server-lifecycle/
  - /docs/operations/server-lifecycle/
---

Understand Rover lifecycle phases, graceful shutdown, and TLS cert hot reload behavior.

## Lifecycle Phases

`Starting -> Running -> (Reloading) -> Running -> Draining -> ShuttingDown -> Shutdown`

| Phase | Accept Connections | Process Requests |
|---|---|---|
| Starting | No | No |
| Running | Yes | Yes |
| Reloading | No | No |
| Draining | No | Yes (existing only) |
| ShuttingDown | No | No |
| Shutdown | No | No |

## Hot Reload Scope

Hot reload is intentionally limited to TLS certificates.

Supported:

- TLS cert/key file reload

Not supported (requires restart):

- Route changes
- Middleware changes
- Server config changes (except cert reload polling)
- Lua app code changes

## TLS Reload Configuration

```lua
local api = rover.server {
  tls = {
    cert_file = "/etc/ssl/certs/server.crt",
    key_file = "/etc/ssl/private/server.key",
    reload_interval_secs = 3600,
  }
}
```

## Lifecycle Hooks

```lua
local api = rover.server {}

api.on_start = function()
  print("starting")
end

api.on_ready = function()
  print("ready")
end

api.on_shutdown = function()
  print("draining")
end

return api
```

## Graceful Shutdown

On `SIGTERM`/`SIGINT`, Rover:

1. Stops accepting new connections.
2. Drains in-flight work.
3. Forces close after timeout.
4. Runs shutdown hooks.

Configure drain timeout:

```lua
local api = rover.server {
  drain_timeout_secs = 30,
}
```

## Production Notes

- Use process manager or orchestrator for code rollouts.
- Keep TLS reload enabled if certs rotate in place.
- Alert on TLS reload errors.
