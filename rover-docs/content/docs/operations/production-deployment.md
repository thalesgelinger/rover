---
weight: 8
title: Production Deployment
aliases:
  - /docs/server/production-deployment/
  - /docs/operations/production-deployment/
---

Deploy Rover in production with secure defaults, proxy trust, observability, and drain-safe rollouts.

## Recommended Topology

- TLS termination at reverse proxy (Nginx/Caddy/Traefik)
- Rover bound to loopback/private network
- Health and readiness endpoints wired into load balancer
- Structured logs and metrics scraped centrally

## Nginx Baseline

```nginx
upstream rover_backend {
  server 127.0.0.1:3000;
  server 127.0.0.1:3001;
  keepalive 32;
}

server {
  listen 443 ssl http2;
  server_name api.example.com;

  ssl_certificate /etc/ssl/certs/api.example.com.crt;
  ssl_certificate_key /etc/ssl/private/api.example.com.key;

  location / {
    proxy_pass http://rover_backend;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Connection "";
  }
}
```

## Rover Production Baseline

```lua
local api = rover.server {
  host = "127.0.0.1",
  port = 3000,
  strict_mode = true,
  security_headers = true,
  docs = false,
  drain_timeout_secs = 30,
  management_prefix = "/_rover",
}

function api.healthz.get(ctx)
  return api.json { status = "ok" }
end

function api.readyz.get(ctx)
  return api.json { ready = true }
end

return api
```

## Scaling Patterns

- Horizontal scale behind LB for API and WebSocket traffic.
- Use sticky sessions for long-lived WS workloads where needed.
- For SSE/stream routes, align proxy timeouts with app route behavior.

## Observability

- Enable request IDs and structured logging.
- Scrape or collect whatever platform metrics you expose around latency, 5xx, and saturation.
- Verify readiness semantics include dependencies (db/cache/downstreams).

## Docs and Management Surface

- Keep docs/admin endpoints under isolated `management_prefix`.
- Require `management_token` outside local dev.
- Do not expose management surface on broad public paths.

## Production Checklist

- Strict mode enabled
- TLS configured (proxy or native)
- Management/admin endpoints isolated and authenticated
- Health/readiness integrated with orchestrator
- Rate limit/load shed configured
- Graceful drain tested during deploy
- Logging + metrics + tracing flowing

## Rollback Checklist

- Keep previous artifact and env config available
- Roll back LB target first, then app replicas
- Confirm readiness/latency/error budget recovered
- Re-run smoke tests on critical endpoints
