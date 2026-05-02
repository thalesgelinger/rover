---
sidebar_position: 6
---

# Server Lifecycle

Understand the Rover server lifecycle phases and hot reload behavior.

## Lifecycle Phases

The Rover server transitions through well-defined phases during its operation:

```
Starting → Running → (Reloading) → Running → Draining → ShuttingDown → Shutdown
```

### Phase Overview

| Phase | Description | Accept Connections | Process Requests |
|-------|-------------|-------------------|------------------|
| `Starting` | Server initializing | No | No |
| `Running` | Normal operation | Yes | Yes |
| `Reloading` | TLS certificate reload in progress | No | No |
| `Draining` | Graceful shutdown in progress | No | Yes (existing) |
| `ShuttingDown` | Final cleanup | No | No |
| `Shutdown` | Server terminated | No | No |

### Phase Transitions

- **Starting → Running**: After startup hooks complete successfully
- **Running → Reloading**: When TLS certificate reload is triggered
- **Reloading → Running**: After certificates are reloaded
- **Running → Draining**: On shutdown signal (SIGTERM/SIGINT)
- **Draining → ShuttingDown**: When all connections complete or drain timeout
- **ShuttingDown → Shutdown**: Final cleanup complete

## Hot Reload (TLS Certificates Only)

Rover supports **hot reload of TLS certificates** without restarting the server. This allows certificate rotation without downtime.

### Supported Hot Reload Scope

Hot reload is **intentionally limited to TLS certificates only**:

| Component | Hot Reload Support | Notes |
|-----------|-------------------|-------|
| TLS certificates | ✅ Yes | Automatic file change detection |
| TLS configuration | ⚠️ Partial | `reload_interval_secs` only |
| Routes | ❌ No | Requires server restart |
| Middleware | ❌ No | Requires server restart |
| Server config | ❌ No | Requires server restart |
| Lua application code | ❌ No | Requires server restart |

### TLS Certificate Reload

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 443,
    tls = {
        cert_file = "/etc/ssl/certs/server.crt",
        key_file = "/etc/ssl/private/server.key",
        reload_interval_secs = 3600  -- Check for changes every hour
    }
}
```

### How TLS Reload Works

1. **File Monitoring**: The server checks certificate files for changes at `reload_interval_secs` intervals
2. **Change Detection**: Detects changes via file modification time and size
3. **Safe Reload**: New certificates are loaded atomically; old certificates remain in use if reload fails
4. **Zero Downtime**: Existing connections continue using previous certificates; new connections use updated certificates

### Security Constraints

Hot reload enforces these safety constraints:

- **PEM Validation**: Reloaded certificates must be valid PEM format
- **Fail-Safe**: If reload fails, previous certificates remain active
- **Atomic Switch**: All-or-nothing certificate update (cert + key pair)
- **No Downgrade Protection**: Reload accepts certificates with any validity period; monitor externally

## Lifecycle Hooks

Register callbacks for lifecycle events:

```lua
local api = rover.server {}

-- Called when server starts
api.on_start = function()
    print("Server starting...")
end

-- Called when server is ready to accept connections
api.on_ready = function()
    print("Server ready")
end

-- Called when shutdown is requested
api.on_shutdown = function()
    print("Shutdown requested, cleaning up...")
end

return api
```

### Available Hooks

| Hook | Event | Phase |
|------|-------|-------|
| `on_start` | Server initialization | Starting |
| `on_ready` | Server accepting connections | Running |
| `on_shutdown` | Shutdown signal received | Draining |
| `on_reload` | TLS reload requested | Reloading |

## Graceful Shutdown

On shutdown signal (SIGTERM/SIGINT):

1. Stop accepting new connections
2. Wait for existing requests to complete (up to `drain_timeout_secs`)
3. Force close any remaining connections
4. Execute shutdown hooks
5. Exit process

### Configuration

```lua
local api = rover.server {
    drain_timeout_secs = 30,  -- Max seconds to wait for connections
}
```

## Production Recommendations

### Probe Behavior During Lifecycle

Load balancers and orchestrators use HTTP health probes to determine traffic routing. Configure your health endpoints to return appropriate status codes based on the lifecycle phase:

| Phase | Liveness Probe | Readiness Probe | Expected Behavior |
|-------|---------------|-----------------|-------------------|
| `Starting` | 200 (process alive) | 503 (not ready) | Don't route traffic yet |
| `Running` | 200 | 200 | Route traffic normally |
| `Draining` | 200 (still alive) | 503 (stopping) | Stop new connections |
| `ShuttingDown` | 200 (finishing) | 503 | Wait for completion |

For production deployment guidance and probe configuration examples, see [Production Deployment](/docs/guides/production-deployment).

### TLS Certificate Management

1. **Use automation**: Integrate with certbot or similar for automatic renewal
2. **Set appropriate interval**: Balance between freshness and I/O overhead
   - Frequent changes: `reload_interval_secs = 60` (1 minute)
   - Stable certificates: `reload_interval_secs = 3600` (1 hour)
3. **Monitor reload failures**: Log and alert on certificate reload errors
4. **Test rotation**: Validate certificate reload in staging before production

### What Requires Restart

The following changes **require server restart** (hot reload not supported):

- Adding/removing routes
- Changing route handlers
- Modifying middleware configuration
- Updating server configuration (port, host, timeouts)
- Changing Lua application code
- Database configuration changes

Use a process manager (systemd, docker-compose) to handle restarts:

```bash
# Graceful restart with systemd
systemctl restart rover-app

# Or with docker-compose
docker-compose restart app
```

## Limitations and Anti-Patterns

### Do Not

- Attempt to reload application routes or middleware via file watching
- Expect Lua code changes to apply without restart
- Use hot reload for configuration changes beyond TLS certificates
- Rely on hot reload for database connection pool resizing

### Design Rationale

Hot reload is intentionally scoped to TLS certificates because:

1. **Predictability**: Route and middleware changes affect request handling; full restart ensures clean state
2. **Safety**: Lua code changes may affect in-flight requests; restart guarantees consistency
3. **Simplicity**: Narrow scope reduces complexity and testing surface
4. **Stability**: Production services should use deployment automation for code changes

For application code updates, use blue-green deployment or rolling restarts via your orchestration platform.

## Testing Hot Reload

Verify TLS hot reload behavior:

```bash
# 1. Start server with initial certificates
cargo run --bin rover -- run app.lua

# 2. Check current certificate
openssl s_client -connect localhost:443 -servername localhost </dev/null 2>/dev/null | openssl x509 -noout -dates

# 3. Update certificate files (e.g., from certbot renewal)
cp /new/cert.pem /etc/ssl/certs/server.crt
cp /new/key.pem /etc/ssl/private/server.key

# 4. Wait for reload interval or trigger SIGHUP
kill -HUP <pid>

# 5. Verify new certificate is active
openssl s_client -connect localhost:443 -servername localhost </dev/null 2>/dev/null | openssl x509 -noout -dates
```

## See Also

- [Production Deployment](/docs/guides/production-deployment) - Health probes, load balancing, and production operations
- [Configuration](/docs/api-reference/configuration) - Server configuration options
- [Backend Server](/docs/guides/backend-server) - Creating HTTP APIs
