---
sidebar_position: 7
---

# Production Deployment

Practical production setup for current Rover server runtime.

## Recommended Topology

- Run Rover behind reverse proxy (Nginx/Caddy/Traefik)
- Terminate TLS at proxy
- Keep Rover on private network (`127.0.0.1` or internal VPC)
- Use built-in probes: `/healthz` and `/readyz`

## Reverse Proxy Example (Nginx)

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
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Rover Server Config (Production Baseline)

```lua
local api = rover.server {
  host = "127.0.0.1",
  port = 3000,
  log_level = "info",

  strict_mode = true,
  body_size_limit = 1024 * 1024,

  management_prefix = "/_rover",
  docs = true,
  management_token = "replace-me",

  readiness = {
    dependencies = {
      db = true,
      cache = true,
    },
  },
}

return api
```

## Health Probes

Use built-ins directly, no custom health route needed.

- Liveness: `/healthz`
- Readiness: `/readyz`

Kubernetes example:

```yaml
livenessProbe:
  httpGet:
    path: /healthz
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /readyz
    port: 3000
  initialDelaySeconds: 2
  periodSeconds: 5
```

## Scaling

Run multiple instances and load-balance at proxy/LB level.

```bash
rover run app.lua -- --port 3000
rover run app.lua -- --port 3001
rover run app.lua -- --port 3002
```

## Logs

- Use `log_level` for runtime verbosity.
- Ship stdout/stderr logs with platform collector (systemd, Docker, k8s).
- Add request timing with middleware + `ctx:set/get` if needed.

```lua
api.before = function(ctx)
  ctx:set("start", os.clock())
end

api.after = function(ctx)
  local start = ctx:get("start")
  if start then
    print("request_ms", (os.clock() - start) * 1000, ctx.method, ctx.path)
  end
end
```

## Native TLS (When Needed)

If proxy TLS termination is not possible:

```lua
local api = rover.server {
  host = "0.0.0.0",
  port = 443,
  tls = {
    cert_file = "/etc/ssl/certs/server.crt",
    key_file = "/etc/ssl/private/server.key",
    reload_interval_secs = 3600,
  },
}
```

## Deployment Checklist

- `strict_mode` enabled
- `body_size_limit` set
- proxy + TLS configured
- `/healthz` and `/readyz` wired in orchestrator
- management docs protected (`management_token`)
- readiness dependencies configured
