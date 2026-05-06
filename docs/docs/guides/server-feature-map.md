---
sidebar_position: 9
---

# Server Feature Map

Current server feature surface, mapped to docs.

## Server Construction

- `rover.server { ... }` -> [Configuration](/docs/api-reference/configuration)
- Route declaration via nested tables -> [Backend Server](/docs/guides/backend-server)
- Route pattern DSL (`p_<name>`) -> [Route Patterns](/docs/api-reference/route-patterns)

## Request Context

- `ctx.method`, `ctx.path`, `ctx.client_ip`, `ctx.client_proto`
- `ctx:headers()`, `ctx:query()`, `ctx:params()`, `ctx:body()`
- `ctx:set/get`, `ctx:request_id()`
- Multipart helpers

Docs: [Context API](/docs/guides/context-api)

## Response Builders

- `api.json`, `api.text`, `api.html`
- `api.redirect`, `api:error`, `api.no_content`, `api.raw`
- `api.stream`, `api.stream_with_headers`, `api.sse`

Docs: [Response Builders](/docs/guides/response-builders)

## Server Extras

- Static mounts: `api.<scope>.static { dir, cache? }`
- Middleware: `before` / `after`
- Error hook: `api.on_error`
- Idempotency wrapper: `api.idempotent(...)`
- Management docs endpoint
- Built-in probes: `/healthz`, `/readyz`

Docs: [Server Extras](/docs/api-reference/server-extras)

## Related Runtime Modules

- `rover.guard` -> [Guard](/docs/api-reference/guard)
- `rover.http` -> [HTTP Client](/docs/api-reference/http-client)
- `rover.ws_client` -> [WS Client](/docs/api-reference/ws-client)
- `rover.auth` / `rover.cookie` / `rover.session` -> [Auth, Cookie, Session](/docs/api-reference/auth-cookie-session)
- `rover.db` -> [Database Guide](/docs/guides/database), [DB Query DSL](/docs/api-reference/db-query-dsl)
- `rover.env` / `rover.config` -> [Configuration](/docs/api-reference/configuration)

## Operations

- Lifecycle + probes + drain behavior -> [Server Lifecycle](/docs/guides/server-lifecycle)
- Production setup -> [Production Deployment](/docs/guides/production-deployment)
