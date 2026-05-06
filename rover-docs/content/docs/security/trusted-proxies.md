---
weight: 7
title: Trusted Proxies
aliases:
  - /docs/server/trusted-proxies/
  - /docs/security/trusted-proxies/
---

Configure Rover to trust forwarded headers from reverse proxies and load balancers while maintaining strict trust boundaries.

## Overview

When running behind a reverse proxy (nginx, Caddy, AWS ALB, etc.), the immediate connection source is the proxy—not the end client. Rover derives the real client IP and protocol from forwarded headers (`Forwarded`, `X-Forwarded-For`, `X-Forwarded-Proto`).

Without `trusted_proxies`, Rover ignores all forwarded headers to prevent IP spoofing attacks.

## Quick Start

```lua
local api = rover.server {
    host = "127.0.0.1",
    port = 3000,
    -- Trust your proxy subnet
    trusted_proxies = { "10.0.0.0/8" },
}
```

## Configuration Formats

### CIDR Notation (Recommended)

```lua
rover.server {
    trusted_proxies = {
        "10.0.0.0/8",      -- AWS VPC
        "172.16.0.0/12",   -- Private range
        "192.168.0.0/16",  -- Private range
    }
}
```

### IP Range (Dash Syntax)

```lua
rover.server {
    trusted_proxies = { "10.0.0.1-10.0.0.255" }
}
```

### Table Format (Explicit Fields)

```lua
rover.server {
    trusted_proxies = {
        { cidr = "10.0.0.0/8" },
        { start = "172.16.0.10", to = "172.16.0.20" },
        { start = "192.168.1.1", end = "192.168.1.100" },
    }
}
```

## Trust Boundaries

Rover implements defense-in-depth for forwarded headers:

### Source Validation

- Headers are only processed when the immediate connection source matches `trusted_proxies`
- Untrusted sources have all forwarded headers stripped
- Malformed header values are ignored

### IP Chain Evaluation

When multiple proxies exist, Rover evaluates the header chain from right to left, stopping at the first untrusted IP:

```
X-Forwarded-For: 203.0.113.1, 198.51.100.2, 10.0.0.5
                                  └─ trusted ─┘
Result: Client IP = 198.51.100.2 (last trusted proxy's client)
```

### Deterministic Conflict Resolution

When both `Forwarded` (RFC 7239) and `X-Forwarded-*` headers exist:

1. **Client IP**: First valid `Forwarded:for=` parameter; falls back to `X-Forwarded-For`
2. **Protocol**: First valid `Forwarded:proto=` parameter; falls back to `X-Forwarded-Proto`
3. **Malformed values**: Invalid `Forwarded` syntax triggers fallback

Example:
```
Forwarded: for=203.0.113.20;proto=https
X-Forwarded-For: 198.51.100.9
X-Forwarded-Proto: http
Result: IP=203.0.113.20, Proto=https
```

## Common Deployment Examples

### AWS ALB/ELB in VPC

```lua
-- ALB typically uses 10.x.x.x in default VPC
local api = rover.server {
    host = "0.0.0.0",
    port = 3000,
    trusted_proxies = { "10.0.0.0/8" },
}
```

### Nginx Reverse Proxy

Nginx configuration:
```nginx
location / {
    proxy_pass http://rover_backend;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Host $host;
}
```

Rover configuration:
```lua
local api = rover.server {
    host = "127.0.0.1",
    port = 3000,
    -- Nginx on same host
    trusted_proxies = { "127.0.0.1/32" },
}
```

### Docker Compose Network

```lua
-- Docker default bridge network
local api = rover.server {
    host = "0.0.0.0",
    port = 3000,
    trusted_proxies = { "172.16.0.0/12" },
}
```

### Kubernetes with Ingress Controller

```lua
-- Internal cluster range where ingress controller runs
local api = rover.server {
    host = "0.0.0.0",
    port = 3000,
    trusted_proxies = {
        "10.0.0.0/8",     -- Pod/service network
        "172.16.0.0/12",  -- Node network
    },
}
```

### Multiple Proxy Tiers

```lua
-- CDN -> Load Balancer -> Application
local api = rover.server {
    host = "0.0.0.0",
    port = 3000,
    trusted_proxies = {
        "10.0.0.0/8",     -- ALB tier
        "172.16.0.0/12",  -- Internal proxy tier
    },
}
```

## Security Best Practices

### 1. Minimize Trust Scope

Use the most specific CIDR that covers your actual proxy infrastructure:

```lua
-- Better: specific subnet
trusted_proxies = { "10.0.1.0/24" }

-- Avoid: overly broad ranges
-- trusted_proxies = { "0.0.0.0/0" }  -- NEVER DO THIS
```

### 2. Never Trust Public IPs

```lua
-- WRONG: allows any client to spoof headers
trusted_proxies = { "0.0.0.0/0" }

-- WRONG: allows spoofing from anywhere
trusted_proxies = { "::/0" }
```

### 3. Bind to Loopback When Possible

```lua
-- Safer: only accept connections from localhost
host = "127.0.0.1",
trusted_proxies = { "127.0.0.1/32" },

-- Less safe: public bind requires careful proxy config
host = "0.0.0.0",
trusted_proxies = { "10.0.0.0/8" },
```

### 4. Validate Protocol Headers

Always derive the protocol from headers, not the connection:

```lua
function api.webhook.post(ctx)
    -- ctx.protocol reflects X-Forwarded-Proto when trusted
    if ctx.protocol ~= "https" then
        return api:error(400, "HTTPS required")
    end
    -- ...
end
```

### 5. Use Strict Mode in Production

```lua
local api = rover.server {
    strict_mode = true,
    host = "127.0.0.1",
    port = 3000,
    trusted_proxies = { "127.0.0.1/32" },
}
```

## Accessing Client Information

### In Route Handlers

```lua
function api.client_info.get(ctx)
    return api.json {
        -- Real client IP (derived from forwarded headers when trusted)
        remote_addr = ctx.remote_addr,
        
        -- Protocol (http or https)
        protocol = ctx.protocol or "http",
        
        -- Direct connection info
        direct_remote = ctx.direct_remote_addr,
    }
end
```

### In Middleware

```lua
function api.secure.before.check_https(ctx)
    -- Trust-derived protocol
    if ctx.protocol ~= "https" then
        return api:redirect("https://" .. ctx:headers()["Host"] .. ctx:path())
    end
end
```

## Testing and Debugging

### Local Testing with Curl

```bash
# Simulate request from trusted proxy
curl -H "X-Forwarded-For: 203.0.113.10" \
     -H "X-Forwarded-Proto: https" \
     http://127.0.0.1:4242/client-info

# Request from untrusted source (headers ignored)
curl http://127.0.0.1:4242/client-info
```

### Test Proxy Chain Evaluation

```bash
# Multiple proxies in chain (rightmost trusted IP wins)
curl -H "X-Forwarded-For: 198.51.100.1, 203.0.113.5, 10.0.0.5" \
     http://127.0.0.1:4242/client-info
```

### RFC 7239 Forwarded Header

```bash
curl -H 'Forwarded: for=198.51.100.17;host=example.com;proto=https' \
     http://127.0.0.1:4242/client-info
```

## Complete Production Example

```lua
local api = rover.server {
    -- Bind to loopback, proxy handles TLS
    host = "127.0.0.1",
    port = 3000,
    
    -- Strict security
    strict_mode = true,
    security_headers = true,
    body_size_limit = 10 * 1024 * 1024,
    
    -- Trust only nginx on same host
    trusted_proxies = { "127.0.0.1/32" },
    
    -- Management endpoints
    management_prefix = "/_rover",
    management_token = rover.env.ROVER_MANAGEMENT_TOKEN,
    
    -- Rate limiting
    rate_limit = {
        enabled = true,
        global = { requests_per_window = 1000, window_secs = 60 },
    },
}

-- Protected route using trusted client info
function api.webhook.stripe.post(ctx)
    -- Ensure HTTPS (from X-Forwarded-Proto)
    if ctx.protocol ~= "https" then
        return api:error(400, "HTTPS required")
    end
    
    -- Access real client IP for logging/rate limiting
    local client_ip = ctx.remote_addr
    
    -- Process webhook...
    return api.json { received = true }
end

return api
```

## Examples

- `examples/trusted_proxy_config.lua` - Basic trusted proxy setup

## Related

- [Configuration](/docs/server/configuration/) - Complete server options reference
- [CORS and Security](/docs/security/cors-and-security/) - Cross-origin and header security
- [Production Deployment](/docs/operations/production-deployment/) - Deployment patterns and best practices
