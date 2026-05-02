---
sidebar_position: 7
---

# Production Deployment

Deploy Rover applications in production with load balancing, TLS termination, and observability.

## Reverse Proxy Setup

### Nginx Configuration

Use Nginx as a reverse proxy for Rover applications:

```nginx
upstream rover_backend {
    server 127.0.0.1:3000;
    server 127.0.0.1:3001;
    server 127.0.0.1:3002;
    keepalive 32;
}

server {
    listen 80;
    server_name api.example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name api.example.com;

    ssl_certificate /etc/ssl/certs/api.example.com.crt;
    ssl_certificate_key /etc/ssl/private/api.example.com.key;
    ssl_protocols TLSv1.2 TLSv1.3;

    location / {
        proxy_pass http://rover_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";
    }
}
```

### Caddy Configuration

Caddy provides automatic HTTPS:

```caddy
api.example.com {
    reverse_proxy localhost:3000 localhost:3001 localhost:3002
    header X-Forwarded-Proto https
}
```

### Traefik (Docker Compose)

```yaml
version: "3"
services:
  traefik:
    image: traefik:v3.0
    command:
      - "--api.insecure=true"
      - "--providers.docker=true"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock

  rover-app:
    build: .
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.rover.rule=Host(`api.example.com`)"
      - "traefik.http.routers.rover.entrypoints=websecure"
      - "traefik.http.routers.rover.tls.certresolver=letsencrypt"
    environment:
      - ROVER_PORT=3000
```

## TLS Termination

### External TLS Termination (Recommended)

For most production deployments, terminate TLS at the reverse proxy:

```lua
-- Rover runs on HTTP internally
local api = rover.server {
    host = "127.0.0.1",
    port = 3000,
}
```

Benefits:
- Simpler certificate management
- Centralized TLS configuration
- Better performance (TLS handled by proxy)
- Automatic Let's Encrypt support with Caddy/Traefik

### Native TLS (Edge Cases)

Use Rover's native TLS only when needed:

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

When to use native TLS:
- No reverse proxy in deployment
- End-to-end encryption requirements
- Custom TLS configuration needs

## Load Balancing Patterns

### Round-Robin (Nginx)

```nginx
upstream rover_backend {
    server 127.0.0.1:3000 weight=3;
    server 127.0.0.1:3001 weight=3;
    server 127.0.0.1:3002 backup;
    keepalive 32;
}
```

### Health Checks

Configure health checks in your reverse proxy. Understanding probe behavior is essential for production deployments—see the [Server Lifecycle](/docs/guides/server-lifecycle) guide for details on how the server transitions through phases and how probes interact with each phase.

### Liveness vs Readiness Probes

Kubernetes distinguishes between two probe types that map to Rover's lifecycle phases:

| Probe Type | Purpose | HTTP Endpoint | Lifecycle Phase |
|------------|---------|---------------|-----------------|
| **Liveness** | Is the server running? Should it be restarted? | `/health` | Running |
| **Readiness** | Is the server ready to accept traffic? | `/health` | Running |
| **Startup** | Has the server finished starting? | `/health` | Starting → Running |

**When probes respond:**
- During `Starting` phase: Readiness returns 503, liveness returns 200 (process is alive but not ready)
- During `Running` phase: Both return 200 when healthy
- During `Draining` phase: Readiness returns 503 (stop sending new traffic), liveness returns 200 (don't restart yet)

### Nginx Health Checks

```nginx
upstream rover_backend {
    server 127.0.0.1:3000;
    server 127.0.0.1:3001;
    
    check interval=3000 rise=2 fall=3 timeout=1000 type=http;
    check_http_send "GET /health HTTP/1.0\r\n\r\n";
    check_http_expect_alive http_2xx http_3xx;
}
```

Add a health endpoint to your Rover app:

```lua
function api.health.get(ctx)
    -- Check database connectivity if needed
    return { status = "healthy", timestamp = os.time() }
end
```

### Sticky Sessions (WebSocket)

For WebSocket applications, use IP hash:

```nginx
upstream rover_ws {
    ip_hash;
    server 127.0.0.1:3000;
    server 127.0.0.1:3001;
}

server {
    location /ws {
        proxy_pass http://rover_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Scaling Patterns

### Vertical Scaling

Increase resources on a single instance:

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 3000,
    workers = 8,  -- Match CPU cores
    max_connections = 10000,
}
```

### Horizontal Scaling

Run multiple instances behind a load balancer:

```bash
# Start multiple instances on different ports
ROVER_PORT=3000 rover run app.lua &
ROVER_PORT=3001 rover run app.lua &
ROVER_PORT=3002 rover run app.lua &
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rover-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: rover
  template:
    metadata:
      labels:
        app: rover
    spec:
      containers:
      - name: rover
        image: rover-app:latest
        ports:
        - containerPort: 3000
        resources:
          requests:
            memory: "128Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 2
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: rover-service
spec:
  selector:
    app: rover
  ports:
  - port: 80
    targetPort: 3000
  type: ClusterIP
```

### Docker Compose Scaling

```yaml
version: "3"
services:
  rover:
    build: .
    environment:
      - ROVER_PORT=3000
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: '1'
          memory: 512M
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
```

## Logging

### Structured Logging

Configure Rover for structured logging:

```lua
-- Log to stdout (captured by Docker/systemd)
local api = rover.server {
    log_level = "info",  -- debug, info, warn, error
    log_format = "json", -- json or text
}

-- Custom logging in handlers
function api.users.get(ctx)
    rover.log.info("Fetching users", {
        request_id = ctx:headers()["X-Request-ID"],
        user_agent = ctx:headers()["User-Agent"],
    })
    return { users = {} }
end
```

### Log Aggregation

**Filebeat + Elasticsearch:**

```yaml
# filebeat.yml
filebeat.inputs:
- type: log
  enabled: true
  paths:
    - /var/log/rover/*.log
  json.keys_under_root: true

output.elasticsearch:
  hosts: ["localhost:9200"]
```

**Fluent Bit:**

```ini
# fluent-bit.conf
[INPUT]
    Name              tail
    Path              /var/log/rover/app.log
    Parser            json

[OUTPUT]
    Name              loki
    Match             *
    Host              loki
    Port              3100
```

### Request Logging

Add request logging middleware:

```lua
local api = rover.server {}

-- Log all requests
api.before = function(ctx)
    ctx.start_time = os.clock()
    return true
end

api.after = function(ctx, response)
    local duration = (os.clock() - ctx.start_time) * 1000
    rover.log.info("Request completed", {
        method = ctx.method,
        path = ctx.path,
        status = response.status,
        duration_ms = duration,
    })
    return response
end
```

## Tracing

### OpenTelemetry Integration

```lua
local api = rover.server {
    tracing = {
        enabled = true,
        exporter = "otlp",
        endpoint = "http://localhost:4317",
        service_name = "rover-api",
    },
}

-- Traces propagate automatically
function api.users.get(ctx)
    -- This span is automatically traced
    return { users = fetch_users() }
end
```

### Jaeger Setup

```yaml
version: "3"
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP gRPC
      
  rover:
    build: .
    environment:
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
      - OTEL_SERVICE_NAME=rover-api
```

### Custom Spans

```lua
function api.orders.post(ctx)
    local span = rover.trace.start_span("process_order")
    
    span:add_event("validating_order")
    local order = validate_order(ctx:body())
    
    span:add_event("saving_to_db")
    save_order(order)
    
    span:add_event("sending_notification")
    send_notification(order)
    
    span:finish()
    
    return { order_id = order.id }
end
```

## Monitoring

### Prometheus Metrics

```lua
local api = rover.server {
    metrics = {
        enabled = true,
        endpoint = "/metrics",
    },
}

-- Custom metrics
local request_counter = rover.metrics.counter("http_requests_total")
local request_duration = rover.metrics.histogram("http_request_duration_seconds")

function api.users.get(ctx)
    request_counter:inc({ method = "GET", path = "/users" })
    
    local start = os.clock()
    local result = fetch_users()
    request_duration:observe(os.clock() - start)
    
    return result
end
```

### Grafana Dashboard

Key metrics to monitor:

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `http_requests_total` | Total requests | N/A |
| `http_request_duration_seconds` | Request latency | p99 > 500ms |
| `rover_active_connections` | Active connections | > 80% of max |
| `rover_memory_usage_bytes` | Memory usage | > 85% limit |

### Comprehensive Health Endpoint

For detailed health checking with dependency validation. The response status code should reflect the [server lifecycle phase](/docs/guides/server-lifecycle)—return 503 during startup or shutdown to signal load balancers to route traffic elsewhere:

```lua
function api.health.get(ctx)
    local checks = {
        server = "ok",
        database = check_database(),
        cache = check_cache(),
    }
    
    local all_ok = true
    for _, status in pairs(checks) do
        if status ~= "ok" then
            all_ok = false
            break
        end
    end
    
    return rover.json:status(all_ok and 200 or 503, {
        status = all_ok and "healthy" or "unhealthy",
        checks = checks,
        timestamp = os.time(),
    })
end
```

## Security Headers

Configure security headers in your reverse proxy:

```nginx
add_header X-Frame-Options "SAMEORIGIN" always;
add_header X-Content-Type-Options "nosniff" always;
add_header X-XSS-Protection "1; mode=block" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
add_header Content-Security-Policy "default-src 'self';" always;
```

Or in Rover middleware:

```lua
api.after = function(ctx, response)
    response.headers = response.headers or {}
    response.headers["X-Content-Type-Options"] = "nosniff"
    response.headers["X-Frame-Options"] = "SAMEORIGIN"
    return response
end
```

## Best Practices

1. **Use reverse proxy TLS termination** for simpler operations
2. **Implement health checks** for load balancer integration
3. **Log structured data** for better observability
4. **Set resource limits** in container orchestration
5. **Monitor key metrics** and set alerts
6. **Use graceful shutdown** handling for zero-downtime deploys
7. **Run multiple instances** behind a load balancer for availability
8. **Keep Rover behind a firewall** when using native TLS

## Deployment Checklist

- [ ] TLS certificates configured
- [ ] Reverse proxy configured
- [ ] Health endpoint implemented
- [ ] Logging configured (structured)
- [ ] Tracing configured (optional)
- [ ] Metrics endpoint enabled
- [ ] Resource limits set
- [ ] Graceful shutdown configured
- [ ] Security headers configured
- [ ] Monitoring dashboards created
- [ ] Alert rules configured

## See Also

- [Server Lifecycle](/docs/guides/server-lifecycle) - Probe behavior and lifecycle phases (starting, draining, shutdown)
- [Configuration](/docs/api-reference/configuration) - Server configuration options
- [Performance](/docs/performance) - Optimization guidelines
