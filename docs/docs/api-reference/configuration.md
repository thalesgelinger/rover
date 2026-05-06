---
sidebar_position: 1
---

# Configuration

Complete `rover.server { ... }` config reference for current runtime.

## Minimal

```lua
local api = rover.server {
  host = "localhost",
  port = 4242,
}
```

## Core

| Key | Type | Default | Notes |
|---|---|---:|---|
| `host` | `string` | `"localhost"` | Bind host |
| `port` | `number` | `4242` | Bind port |
| `log_level` | `"debug" \| "info" \| "warn" \| "error" \| "nope"` | `"debug"` | `"nope"` disables logs |
| `docs` | `boolean` | `false` | Enables OpenAPI UI |
| `body_size_limit` | `number` | `1048576` | Bytes. `0` disables limit |

## CORS / Headers

| Key | Type | Default | Notes |
|---|---|---:|---|
| `cors_origin` | `string?` | `nil` | Example: `"*"` or exact origin |
| `cors_methods` | `string` | `"GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD"` | Preflight allow methods |
| `cors_headers` | `string` | `"Content-Type, Authorization"` | Preflight allow headers |
| `cors_credentials` | `boolean` | `false` | Adds `Access-Control-Allow-Credentials: true` |
| `security_headers` | `boolean` | `true` | Adds safe defaults |
| `allow_insecure_security_header_overrides` | `boolean` | `false` | Strict-mode escape hatch |

## Strict Mode / Network Safety

| Key | Type | Default | Notes |
|---|---|---:|---|
| `strict_mode` | `boolean` | `true` | Enforces startup safety rules |
| `allow_public_bind` | `boolean` | `false` | Allows non-loopback bind with strict mode |
| `https_redirect` | `boolean` | `false` | Required by strict mode for public bind |
| `allow_insecure_http` | `boolean` | `false` | Escape hatch for strict-mode HTTPS redirect rule |
| `allow_wildcard_cors_credentials` | `boolean` | `false` | Escape hatch for `*` + credentials |
| `allow_unbounded_body` | `boolean` | `false` | Escape hatch for `body_size_limit = 0` |

## Management Endpoints

| Key | Type | Default | Notes |
|---|---|---:|---|
| `management_prefix` | `string` | `"/_rover"` | Namespace for management paths |
| `management_token` | `string?` | `nil` | Token for management auth |
| `allow_unauthenticated_management` | `boolean` | `false` | If `true`, no auth needed |

When `docs = true`, docs UI is exposed at:

- ``${management_prefix}/docs`` (default: `/_rover/docs`)

Auth accepted for management docs:

- `Authorization: Bearer <token>`
- `X-Rover-Management-Token: <token>`

## Proxies / Client IP

| Key | Type | Default | Notes |
|---|---|---:|---|
| `trusted_proxies` | `array` | `[]` | CIDR or IP range entries |

Supported entries:

```lua
trusted_proxies = {
  "10.0.0.0/8",
  "192.168.1.10-192.168.1.50",
  { cidr = "172.16.0.0/12" },
  { start = "203.0.113.10", to = "203.0.113.20" },
}
```

## TLS / HTTP2

| Key | Type | Default | Notes |
|---|---|---:|---|
| `tls` | `table?` | `nil` | Native TLS config |
| `http2` | `boolean` | `true` | Config-level toggle; requires TLS |

TLS table:

```lua
tls = {
  cert_file = "/path/server.crt",   -- required
  key_file = "/path/server.key",    -- required
  reload_interval_secs = 3600,        -- default 1
}
```

## Compression

| Key | Type | Default | Notes |
|---|---|---:|---|
| `compress.enabled` | `boolean` | `true` | Enable compression |
| `compress.algorithms` | `string[]` | `{ "gzip", "deflate" }` | Negotiated from `Accept-Encoding` |
| `compress.min_size` | `number` | `1024` | Minimum body size in bytes |
| `compress.types` | `string[]` | `{}` | Optional content-type allowlist |

## Rate Limit

| Key | Type | Default | Notes |
|---|---|---:|---|
| `rate_limit.enabled` | `boolean` | `false` | Master switch |
| `rate_limit.global` | `table?` | `nil` | Global token bucket |
| `rate_limit.scoped` | `table[]` | `[]` | Path-pattern policies |

Policy fields:

- `requests_per_window` (default `1000`)
- `window_secs` (default `60`)
- `key_header` (optional header identity key)

## Load Shed / Backpressure

| Key | Type | Default | Notes |
|---|---|---:|---|
| `load_shed.max_inflight` | `number?` | `10000` | `nil` disables inflight cap |
| `load_shed.max_queue` | `number?` | `1000` | `nil` disables queue cap |

## Readiness / Shutdown

| Key | Type | Default | Notes |
|---|---|---:|---|
| `readiness.dependencies` | `table<string, boolean>` | `{}` | `false` marks dependency unavailable |
| `drain_timeout_secs` | `number?` | `nil` | Graceful drain timeout |

Built-in probes (always available):

- `/healthz` (liveness)
- `/readyz` (readiness)

## Permissions / Idempotency

| Key | Type | Default | Notes |
|---|---|---:|---|
| `permissions.mode` | `"development" \| "production"` | `"development"` | Runtime permission mode |
| `permissions.allow` | `string[]` | `[]` | `fs`, `net`, `env`, `process`, `ffi` |
| `permissions.deny` | `string[]` | `[]` | Same set as allow |
| `idempotency.backend` | `"memory" \| "sqlite"` | `"memory"` | Store backend |
| `idempotency.sqlite_path` | `string?` | `nil` | Required when backend is `sqlite` |

## Full Example

```lua
local api = rover.server {
  host = "0.0.0.0",
  port = 8080,
  docs = true,
  management_prefix = "/_rover",
  management_token = "replace-me",

  cors_origin = "https://app.example.com",
  cors_credentials = true,

  compress = {
    enabled = true,
    algorithms = { "gzip", "deflate" },
    min_size = 1024,
  },

  rate_limit = {
    enabled = true,
    global = {
      requests_per_window = 120,
      window_secs = 60,
    },
  },

  readiness = {
    dependencies = {
      db = true,
      redis = true,
    },
  },
}

return api
```
