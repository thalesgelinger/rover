---
weight: 11
title: HTTP2 and ALPN
aliases:
  - /docs/http-and-realtime/http2-and-alpn/
  - /docs/server/http2-and-alpn/
---

Control HTTP/2 rollout with a minimal transport config surface.

## Configuration

Set `http2` in `rover.server { ... }`:

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 8443,
    strict_mode = true,
    allow_public_bind = true,
    tls = {
        cert_file = "./certs/dev-cert.pem",
        key_file = "./certs/dev-key.pem",
        reload_interval_secs = 300,
    },
    http2 = true,
}

return api
```

## MVP Control Set

- `http2 = true|false` is the only exposed rollout switch.
- HTTP/2 requires TLS.
- In strict mode, startup fails if `http2 = true` and `tls` is missing.

This keeps transport rollout explicit and avoids speculative tuning knobs.

## Current Phase Status

- ALPN intent is computed from config (`h2,http/1.1` when `http2=true`, otherwise `http/1.1`).
- Startup validation enforces TLS requirement for `http2=true` in strict mode.
- Current event-loop transport still serves HTTP/1.1 while TLS/HTTP2 transport migration is in progress.

## Safety Notes

- Keep `http2 = false` as a compatibility fallback during staged rollout.
- Validate cert/key paths in startup checks before production deploy.
- Keep health/readiness probes in rollout checks (`/healthz`, `/readyz`).

## Runnable Example

- `examples/foundation_tls_lifecycle.lua`

Run:

```bash
cargo run -p rover_cli -- run examples/foundation_tls_lifecycle.lua
```

## Related

- [Configuration](/docs/server/configuration/)
- [Production Deployment](/docs/operations/production-deployment/)
- [Server Lifecycle](/docs/operations/server-lifecycle/)
