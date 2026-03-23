---
weight: 1
title: Configuration
aliases:
  - /docs/server/configuration/
  - /docs/server/configuration/
---

Configure Rover server runtime, security, management, and deploy-time behavior.

## Server Options

Pass configuration options to `rover.server`:

```lua
local api = rover.server {
    host = "127.0.0.1",        -- default: "localhost"
    port = 3000,                -- default: 4242
    log_level = "debug",       -- debug|info|warn|error|nope
    docs = false,
    strict_mode = true,
    allow_public_bind = false,
    allow_insecure_http = false,
    allow_wildcard_cors_credentials = false,
    allow_unbounded_body = false,
    security_headers = true,
    allow_insecure_security_header_overrides = false,
    https_redirect = false,
    body_size_limit = 1024 * 1024,
    cors_origin = "*",
    cors_methods = "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD",
    cors_headers = "Content-Type, Authorization",
    cors_credentials = false,
    management_prefix = "/_rover",
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
    allow_unauthenticated_management = false,
    rate_limit = {
        enabled = true,
        global = { requests_per_window = 120, window_secs = 60 },
        scoped = {
            { path_pattern = "/auth", requests_per_window = 12, window_secs = 60 },
        },
    },
    load_shed = {
        max_inflight = 200,
        max_queue = 100,
    },
    drain_timeout_secs = 30,
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
        reload_interval_secs = 3600, -- Optional: auto-reload interval
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

Important: Hot reload only applies to TLS certificates. Route, middleware, config, and Lua code changes still require restart.

## Configuration Reference

### `host`

- **Type**: `string`
- **Default**: `"localhost"`
- **Description**: The host address to bind the server to

### `port`

- **Type**: `number`
- **Default**: `4242`
- **Description**: The port number to listen on

### `log_level`

- **Type**: `string`
- **Default**: `"debug"`
- **Options**: `"debug"`, `"info"`, `"warn"`, `"error"`, `"nope"`
- **Description**: Set the logging verbosity level

### `docs`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Enable OpenAPI docs at `/docs`

### `strict_mode`

- **Type**: `boolean`
- **Default**: `true`
- **Description**: Enforce strict secure startup checks

Strict mode rejects insecure production-ish configs by default. Common failures:

- public bind without explicit opt-out
- wildcard CORS with credentials
- disabled security headers
- unbounded body size without explicit opt-out

### `body_size_limit`

- **Type**: `number`
- **Default**: implementation default limit
- **Description**: Max request body size in bytes; `0` disables limit, but strict mode requires `allow_unbounded_body = true`

### `allow_public_bind`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict host binding checks and allow non-loopback host values

### `allow_insecure_http`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Opt out of strict HTTPS redirect requirement when binding publicly

### `https_redirect`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Redirect HTTP requests toward HTTPS when running with public exposure

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

### `cors_origin`

- **Type**: `string`
- **Default**: `nil` (CORS disabled)
- **Description**: Allowed CORS origin, for example `"*"` or `"https://app.example.com"`

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

### `management_prefix`

- **Type**: `string`
- **Default**: `"/_rover"`
- **Description**: Isolated prefix for management endpoints such as docs

Must start with `/`. `/` is rejected.

### `management_token`

- **Type**: `string`
- **Default**: `nil`
- **Description**: Bearer token accepted by management/admin endpoints

### `allow_unauthenticated_management`

- **Type**: `boolean`
- **Default**: `false`
- **Description**: Disables auth-by-default protection for management endpoints

Use only in local dev.

### `rate_limit`

- **Type**: `boolean | table`
- **Default**: enabled defaults
- **Description**: Configures global and scoped request throttling

Example:

```lua
rover.server {
    rate_limit = {
        enabled = true,
        global = {
            requests_per_window = 100,
            window_secs = 60,
            key_header = "X-API-Key", -- optional; defaults to client identity
        },
        scoped = {
            { path_pattern = "/login", requests_per_window = 10, window_secs = 60 },
            { path_pattern = "/search", requests_per_window = 30, window_secs = 10 },
        },
    }
}
```

### `load_shed`

- **Type**: `boolean | table`
- **Default**: enabled defaults
- **Description**: Bounds inflight work and queue depth to protect latency under overload

Example:

```lua
rover.server {
    load_shed = {
        max_inflight = 500,
        max_queue = 200,
    }
}
```

### `drain_timeout_secs`

- **Type**: `number`
- **Default**: `nil`
- **Description**: Max shutdown drain window before remaining connections are closed

### `compress`

- **Type**: `boolean | table`
- **Default**: enabled with default algorithms
- **Description**: Configures response compression for supported content types

Supported compression algorithms:
- `gzip` (RFC 1952)
- `deflate` (RFC 1951)

Example configuration:

```lua
rover.server {
    compress = {
        enabled = true,
        algorithms = { "gzip", "deflate" },
        min_size = 1024,        -- Only compress responses >= 1024 bytes
        types = {               -- Only compress these content types
            "application/json",
            "text/html",
            "text/plain",
        },
    }
}
```

Configuration options:

- **`enabled`** (`boolean`, default: `true`): Enable or disable compression
- **`algorithms`** (`array[string]`): List of supported algorithms (must be `"gzip"` or `"deflate"`)
- **`min_size`** (`number`, default: `1024`): Minimum response size in bytes to trigger compression
- **`types`** (`array[string]`): Content types to compress (empty means compress all eligible types)

Behavior notes:

- Encoding negotiation respects client `Accept-Encoding` quality values
- Responses include `Content-Encoding` and `Vary: Accept-Encoding` headers when compressed
- Unsupported encodings (like brotli) are ignored
- Small responses below `min_size` are not compressed
- Already compressed content types (images, videos) are automatically skipped

### `trusted_proxies`

- **Type**: `array[string | table]`
- **Default**: `nil` (no trusted proxies)
- **Description**: Define which proxy sources are trusted for forwarded headers

When running behind a reverse proxy or load balancer, configure `trusted_proxies` so Rover can safely derive client IP and protocol from forwarded headers. Requests from untrusted sources have forwarded headers stripped to prevent spoofing.

Supported formats:

**CIDR notation (string):**
```lua
rover.server {
    trusted_proxies = { "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16" }
}
```

**IP range (string with dash):**
```lua
rover.server {
    trusted_proxies = { "10.0.0.1-10.0.0.255" }
}
```

**Table format with explicit fields:**
```lua
rover.server {
    trusted_proxies = {
        { cidr = "10.0.0.0/8" },
        { start = "172.16.0.10", to = "172.16.0.20" },
        { start = "192.168.1.1", end = "192.168.1.100" },
    }
}
```

**Forwarded header handling:**

When the request source matches a trusted proxy:
- `Forwarded` header (RFC 7239) is parsed for `for=` (client IP) and `proto=` (protocol)
- `X-Forwarded-For` and `X-Forwarded-Proto` are used as fallbacks
- The `Forwarded` header takes precedence over `X-Forwarded-*` when both are present
- IP chains are evaluated from right to left, stopping at the first untrusted IP

**Conflict resolution (deterministic):**

When both `Forwarded` and `X-Forwarded-*` headers are present, the server uses this deterministic priority:

1. **Client IP**: First valid IP from `Forwarded:for=` is used; only if `Forwarded` has no valid `for=` parameter, `X-Forwarded-For` is consulted
2. **Protocol**: First valid proto from `Forwarded:proto=` is used; only if `Forwarded` has no valid `proto=` parameter, `X-Forwarded-Proto` is consulted
3. **Malformed values**: Invalid `Forwarded` header syntax falls back to `X-Forwarded-*` headers

Examples:
```
# Forwarded takes precedence
Forwarded: for=203.0.113.20;proto=https
X-Forwarded-For: 198.51.100.9
X-Forwarded-Proto: http
# Result: IP=203.0.113.20, Proto=https

# Falls back to X-Forwarded-* when Forwarded lacks parameters
Forwarded: proto=https
X-Forwarded-For: 198.51.100.9
# Result: IP=198.51.100.9 (from X-Forwarded-For), Proto=https (from Forwarded)
```

**Trust boundaries:**

- Untrusted sources cannot spoof client identity via forwarded headers
- Malformed forwarded header values are ignored
- Requests without trusted proxies only see the direct connection IP

**Common deployment examples:**

```lua
-- AWS ALB/ELB in VPC
rover.server {
    trusted_proxies = { "10.0.0.0/8" }
}

-- nginx reverse proxy on private subnet
rover.server {
    trusted_proxies = { "172.16.0.0/12" }
}

-- Multiple proxy tiers
rover.server {
    trusted_proxies = {
        "10.0.0.0/8",     -- Load balancer tier
        "172.16.0.0/12",  -- Internal proxy tier
    }
}
```

Rover provides direct access to environment variables via `rover.env`.

```lua
local port = tonumber(rover.env.PORT or "4242")
local host = rover.env.HOST or "localhost"
```

Rover loads `.env` from the project root at startup.

## Config Files

Use `rover.config.load(path)` for Lua config files:

```lua
local cfg = rover.config.load("config.lua")
```

Use `rover.config.from_env(prefix)` to map prefixed env vars into nested config.

## Complete Example

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 8080,
    log_level = "info",
    docs = true,
    allow_public_bind = true,
    https_redirect = true,
    body_size_limit = 1024 * 1024,
    security_headers = true,
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
}

function api.health.get(ctx)
    return api.text("OK")
end

return api
```

## Related Guides

- [Middleware](/docs/server/middleware/) - request pipeline patterns
- [Auth](/docs/security/auth/) - bearer, JWT, sessions, management auth
- [Sessions and Cookies](/docs/security/sessions-and-cookies/) - cookie/session state patterns
- [CORS and Security](/docs/security/cors-and-security/) - browser + header hardening
- [Streaming](/docs/http-and-realtime/streaming/) - chunked responses and SSE
- [Response Optimization](/docs/http-and-realtime/response-optimization/) - compression and validators
- [OpenAPI and Versioning](/docs/http-and-realtime/openapi-and-versioning/) - docs UI and route versioning
- [Operations](/docs/operations/operations/) - health, readiness, request IDs
- [Server Lifecycle](/docs/operations/server-lifecycle/) - startup hooks, drain, TLS reload
