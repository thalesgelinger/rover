---
sidebar_position: 6
---

# Server Lifecycle

Current lifecycle behavior and probe contracts.

## Phases

```
Starting -> Running -> (Reloading) -> Running -> Draining -> ShuttingDown -> Shutdown
```

| Phase | Accept New Connections | Process Existing |
|---|---|---|
| `Starting` | No | No |
| `Running` | Yes | Yes |
| `Reloading` | No | No |
| `Draining` | No | Yes |
| `ShuttingDown` | No | No |
| `Shutdown` | No | No |

## Built-in Probe Endpoints

Rover exposes these paths automatically:

- `GET/HEAD /healthz` -> `200 {"status":"ok"}`
- `GET/HEAD /readyz` -> readiness state

Readiness returns:

- `200 {"status":"ready"}` when accepting traffic
- `503 {"status":"not_ready"}` when draining/shutting down
- `503` with dependency reasons when `readiness.dependencies` contains failed deps

Other methods on probe paths return `405` + `Allow: GET, HEAD`.

## TLS Reload Scope

Hot reload support is intentionally narrow:

- Supported: TLS cert/key file reload (`tls.reload_interval_secs`)
- Not supported: route reload, middleware reload, Lua code reload, general config reload

## Graceful Shutdown

On SIGTERM/SIGINT:

1. Stop accepting new connections
2. Drain in-flight requests
3. Enforce optional timeout (`drain_timeout_secs`)
4. Close remaining connections

```lua
local api = rover.server {
  drain_timeout_secs = 30,
}
```

## Important Notes

- There are no public Lua lifecycle hook APIs (`on_start`, `on_ready`, etc.) in current runtime.
- Use process manager orchestration for rollout/restart behavior.

## See Also

- [Configuration](/docs/api-reference/configuration)
- [Production Deployment](/docs/guides/production-deployment)
