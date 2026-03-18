# PRD: Rover Backend Runtime Production Foundation

## 1. Introduction/Overview

Build a production-complete backend runtime for Rover with a strong Lua-first DSL, performance-first execution model, and reliable operational behavior. Product mindset: runtime capability bar like Bun/Node for backend execution, plus first-party developer ergonomics like Rails/Phoenix where the runtime ships the full toolbox for common backend needs. This phase must consolidate what already exists (notably most P1 foundation tickets already in review) and deliver remaining critical backend capabilities across security, transport, observability, data/runtime controls, and production delivery.

This PRD is based on:
- User direction: foundation-first, performance-first, Lua `api.something()` ergonomics, avoid callback-heavy API surface.
- Rover project tickets in Plane (notably seq 11-44 and related backend specs).
- Existing repo capabilities and examples under `examples/` (middleware, routing, validation, auth, env/config, db/http, websockets).

## 2. Goals

- Deliver all critical backend/runtime capabilities required for production operation, phased from foundation to ops polish.
- Keep Lua API surface simple, explicit, and mostly declarative using named namespace function definitions (for example `function api.auth.middleware(ctx) ... end`, `function api.users.p_id.get(ctx) ... end`) instead of callback registration patterns.
- Hit strong baseline performance SLOs under realistic load while preserving safety and correctness.
- Ensure production reliability through limits, backpressure, graceful shutdown, health/readiness, and hardened admin surfaces.
- Ship a batteries-included runtime: first-party modules for core backend concerns, minimal dependency on third-party infra for baseline production setup.
- Provide full implementation guidance for junior developers and agents, including concrete DSL examples and verifiable acceptance criteria.

## 3. User Stories

### US-001: Foundation baseline hardening and merge gate
**Description:** As a platform engineer, I want all P1 foundation capabilities verified and merged cleanly so that advanced runtime features are built on stable primitives.

**Acceptance Criteria:**
- [ ] Confirm P1 features (middleware, routing semantics, input validation/content negotiation, error handling, body limits, auth, env/config, CORS, security headers) are available in main branch behavior.
- [ ] Create a regression checklist with one executable example per P1 capability from `examples/` or new focused tests.
- [ ] `cargo check --workspace` passes.
- [ ] `cargo test --workspace` passes for touched crates.
- [ ] `cargo clippy --workspace --all-targets --all-features` passes for touched crates.

### US-002: Response compression foundation
**Description:** As an API consumer, I want compressed responses when appropriate so that bandwidth and latency are reduced.

**Acceptance Criteria:**
- [ ] Add negotiated compression (`gzip`, optional `br`, optional `deflate`) with `Accept-Encoding` parsing and deterministic algorithm selection.
- [ ] Skip compression for small bodies (`min_size`), pre-compressed media, and incompatible responses.
- [ ] Ensure correct headers: `Content-Encoding`, `Vary: Accept-Encoding`, and adjusted `Content-Length` behavior.
- [ ] Add streaming compatibility rules (compressible vs non-compressible stream paths explicitly documented and tested).
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-003: Transport protection and proxy correctness
**Description:** As an operator, I want secure transport defaults and trusted proxy semantics so that request metadata is safe and correct in production.

**Acceptance Criteria:**
- [ ] Implement TLS server config with cert/key loading and minimum TLS version policy.
- [ ] Implement optional HTTPS redirect mode with loop-safe behavior.
- [ ] Implement trust-proxy config and safe parsing of `Forwarded` and `X-Forwarded-*` headers.
- [ ] Ensure client IP/proto derivation uses trust boundaries and rejects spoofed chains from untrusted peers.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-004: Runtime limits, timeouts, and overload protection
**Description:** As an operator, I want strict limits and backpressure so that one traffic spike cannot destabilize the runtime.

**Acceptance Criteria:**
- [ ] Add configurable request/header/body/idle timeouts.
- [ ] Add connection/header-size/in-flight limits.
- [ ] Add overload responses (`503`/`429` policy), retry hints, and overload metrics counters.
- [ ] Ensure limits are configurable globally and optionally overridable per route group where safe.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-005: Health and readiness contract
**Description:** As a deployment platform, I want reliable health/readiness endpoints so that orchestration decisions are accurate.

**Acceptance Criteria:**
- [ ] Implement liveness endpoint with constant-time runtime status check.
- [ ] Implement readiness endpoint with dependency checks (db/store/network dependencies configurable).
- [ ] Return non-ready with structured failure reasons without leaking secrets.
- [ ] Document and test expected status code contracts for healthy, degraded, and unavailable states.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-006: Request identity and structured observability
**Description:** As an SRE, I want request correlation, structured logs, and trace/metric signals so that incidents can be diagnosed fast.

**Acceptance Criteria:**
- [ ] Generate or propagate request ID and expose it via context API and response headers.
- [ ] Provide structured logging mode with redaction controls and slow-request thresholds.
- [ ] Expose metrics endpoint with request count, error count, latency histogram buckets, and shed/limit counters.
- [ ] Add trace context propagation hooks compatible with upstream/downstream services.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-007: Caching and conditional request support
**Description:** As a client and CDN, I want ETag/Last-Modified and correct conditional behavior so that repeated reads are faster and cheaper.

**Acceptance Criteria:**
- [ ] Add API support for `ETag`, `Last-Modified`, and cache-control presets.
- [ ] Implement conditional request handling (`If-None-Match`, `If-Modified-Since`) with correct `304` behavior.
- [ ] Ensure `Vary` behavior is explicit and composable with compression and content negotiation.
- [ ] Add tests for strong/weak validator semantics and precedence rules.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-008: Realtime and large-payload delivery
**Description:** As an application developer, I want SSE, streaming responses, and multipart uploads so that realtime and large data flows are practical.

**Acceptance Criteria:**
- [ ] Implement SSE API with thin route-owned stream writer semantics (`function api.x.sse(ctx, stream) ... end`), built-in keepalive helpers, reconnect-friendly semantics (`retry`, `Last-Event-ID`), and safe client cleanup.
- [ ] Implement WebSocket API with explicit dot-notation event handlers (`function api.x.ws(ws) ... end`), including `ws.open(ctx)`, `ws.close(ctx)`, `ws.message(ctx, msg)` fallback, `ws.message.<event>(ctx, msg)` direct dispatch, explicit topic subscribe/unsubscribe helpers, and broadcast semantics with `exclude_self` support.
- [ ] Implement chunked streaming response API with deterministic headers and backpressure-aware writes.
- [ ] Implement multipart parser with single-file and multi-file helpers plus size/type limit enforcement.
- [ ] Provide static file serving with safe path normalization and route coexistence guarantees.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-009: Stateful runtime primitives (cookies, sessions, rate limiting)
**Description:** As a backend developer, I want secure stateful primitives so that auth/session flows and abuse protection are easy to ship.

**Acceptance Criteria:**
- [ ] Add cookie read/set/delete with full attribute support (`HttpOnly`, `Secure`, `SameSite`, `Max-Age`, `Domain`, `Path`).
- [ ] Add session API with memory/sqlite/custom stores and expiration lifecycle.
- [ ] Add rate limiting globally and per-scope with deterministic keying and standard headers.
- [ ] Ensure session + rate limiting can use distributed store backend when configured.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-010: Rover shared KV store abstraction
**Description:** As an operator running multiple instances, I want Rover-provided shared KV storage so that sessions, limits, and runtime state work consistently across nodes without third-party store dependency.

**Acceptance Criteria:**
- [ ] Implement Rover shared KV store backend as first-party runtime capability, with in-memory mode for local/dev.
- [ ] Support TTL, namespacing/key-prefix, serialization format versioning, and failure fallback policy.
- [ ] Ensure rate limit/session modules can switch stores without application-level code changes.
- [ ] Provide resilience tests for store outages and latency spikes.
- [ ] `cargo check -p rover-server` passes.
- [ ] `cargo test -p rover-server` passes.

### US-011: Safe runtime lifecycle and background work
**Description:** As a platform engineer, I want graceful shutdown and bounded async jobs so that deploys and scheduled work are safe.

**Acceptance Criteria:**
- [ ] Implement signal-aware graceful shutdown with drain timeout and in-flight completion handling.
- [ ] Implement runtime job primitives (`spawn`, `delay`, `interval`) with cancellation and shutdown-aware behavior.
- [ ] Ensure job system has guardrails (max concurrent jobs, panic/error isolation, logging hooks).
- [ ] Document unsupported heavy-job patterns and explicit recommendation to external queue for critical workloads.
- [ ] `cargo check --workspace` passes for touched crates.
- [ ] `cargo test --workspace` passes for touched crates.

### US-012: OpenAPI/versioning and production delivery guidance
**Description:** As a team adopting Rover, I want API contracts and deployment guidance so that production rollout is consistent.

**Acceptance Criteria:**
- [ ] Generate and serve OpenAPI docs from route contracts with stable schema output.
- [ ] Define API versioning strategy (path/header) and deprecation policy.
- [ ] Harden admin/management endpoints with auth-by-default and isolated exposure options.
- [ ] Publish deployment topology guidance (proxy/LB/TLS termination/scaling/log pipeline patterns).
- [ ] `cargo check --workspace` passes for touched crates.
- [ ] `cargo test --workspace` passes for touched crates.

## 4. Functional Requirements

- FR-1: Runtime must support negotiated response compression using `Accept-Encoding` and configurable algorithm priority.
- FR-2: Runtime must skip compression for payloads below configured threshold and non-compressible content types.
- FR-3: Runtime must set `Vary: Accept-Encoding` whenever compression negotiation can affect response bytes.
- FR-4: Runtime must provide TLS configuration with cert/key loading and minimum TLS version constraints.
- FR-5: Runtime must support optional HTTPS redirects with loop-safe behavior behind proxies.
- FR-6: Runtime must implement trusted-proxy config and safe parsing of forwarded headers.
- FR-7: Runtime must expose normalized client IP and protocol through request context.
- FR-8: Runtime must enforce configurable timeouts (header, body, request, idle).
- FR-9: Runtime must enforce configurable connection limits, max header size, and in-flight caps.
- FR-10: Runtime must support load-shedding/backpressure policy with explicit overload responses.
- FR-11: Runtime must expose liveness and readiness endpoints with dependency-aware readiness checks.
- FR-12: Runtime must provide request ID generation/pass-through and context access (`ctx:request_id()` equivalent).
- FR-13: Runtime must support structured logging mode with redaction controls and slow-request threshold warnings.
- FR-14: Runtime must expose operational metrics endpoint and basic trace context propagation.
- FR-15: Runtime must support conditional caching via `ETag` and `Last-Modified` with correct `304` behavior.
- FR-16: Runtime must provide cache-control helpers and ensure `Vary` correctness across compression/content negotiation.
- FR-17: Runtime must provide SSE endpoint support via named route contract `function api.namespace.sse(ctx, stream) ... end`, with `stream:send(data)`, `stream:event(name, data)`, keepalive support, reconnect hints, `Last-Event-ID` compatibility, and disconnect cleanup.
- FR-18: Runtime must provide WebSocket endpoint support via named route contract `function api.namespace.ws(ws) ... end`, with `ws.open(ctx)`, `ws.close(ctx)`, `ws.message(ctx, msg)` fallback, `ws.message.<event>(ctx, msg)` direct event handlers, `ws.send.<event>(ctx, payload)`, `ws.broadcast.<event>(topic, payload, opts?)`, and explicit `ws.subscribe(ctx, topic)` / `ws.unsubscribe(ctx, topic)` helpers.
- FR-19: Runtime must parse multipart/form-data with safe defaults for max parts and max file size.
- FR-20: Runtime must provide helpers for single-file and multi-file upload access.
- FR-21: Runtime must support static file serving with path traversal protections.
- FR-22: Runtime must support cookie read/write/delete helpers with secure attributes.
- FR-23: Runtime must support session lifecycle APIs with pluggable store backends.
- FR-24: Runtime must support global and scoped rate-limiting policies with deterministic key strategies.
- FR-25: Runtime must emit standard rate-limit response headers and retry hints.
- FR-26: Runtime must provide Rover-native distributed shared KV store abstraction usable by session and rate-limit features.
- FR-27: Runtime must support store TTL, namespacing, and configurable failure policy.
- FR-28: Runtime must support graceful shutdown with drain timeout and no new accepted requests during drain.
- FR-29: Runtime must support bounded background job primitives (`spawn`, `delay`, `interval`) with runtime lifecycle hooks.
- FR-30: Runtime must generate and serve OpenAPI spec from route contracts.
- FR-31: Runtime must support explicit API versioning strategy and route deprecation metadata.
- FR-32: Runtime must harden management/admin endpoints with authentication and restricted bind options.
- FR-33: Runtime DSL must use named namespace function definitions as the primary contract (`function api.namespace.action(ctx) ... end`) and avoid callback registration parameters for route and middleware definition; realtime APIs should keep the same style through explicit named handlers (`function api.namespace.sse(ctx, stream) ... end`, `function api.namespace.ws(ws) ... end`) instead of callback registration or metatable-heavy routing magic.
- FR-34: Runtime docs/examples must include production-safe defaults and anti-pattern warnings for each major subsystem.
- FR-35: Runtime must include performance benchmarks and regression thresholds for hot paths.

## 5. Non-Goals (Out of Scope)

- Building a full identity provider or user management product.
- Building a full distributed job queue equivalent to dedicated systems (e.g., Kafka/Celery/BullMQ-class).
- Supporting every external store backend at launch; Rover-native shared KV is the required baseline.
- Refactoring unrelated crates or changing public APIs outside backend runtime scope.
- Building frontend UI features in this phase.

## 6. Design Considerations (Optional)

- DSL direction must keep route-first ergonomics, consistent with current examples (`function api.users.p_id.get(ctx) ... end`).
- Namespace contract must be explicit: handlers and middleware are declared as named functions on the `api` tree, not registered by passing anonymous callbacks into config methods.
- Prefer explicit config blocks under `rover.server { ... }` for runtime controls (limits, compression, TLS, logging, stores).
- Avoid callback-heavy control APIs unless technically mandatory for streaming internals; even then, route and middleware binding remains named function style.
- Realtime DSL should stay explicit and structured: SSE should use thin stream-writer APIs, while WebSockets should use dot-notation event handlers (`ws.open`, `ws.close`, `ws.message`, `ws.message.<event>`) instead of returned state threading, giant `if` chains, or metatable-heavy broadcast chains.
- WebSocket topic membership should be explicit (`subscribe` / `unsubscribe`) and broadcasting should use explicit options (`exclude_self`) rather than chained magic.
- Docs/examples should call out realtime anti-patterns directly: do not model SSE like bidirectional sockets, do not rely on manual subscriber tables for basic SSE, and do not imply replay or guaranteed delivery semantics for WebSockets unless the runtime implements them.
- Keep behavior consistent across HTTP, middleware, and future docs generation to avoid dual mental models.

## 7. Technical Considerations (Optional)

- Existing implementation appears to already cover much of P1 foundation in active review; this PRD prioritizes remaining backend gaps and production hardening.
- Storage direction is first-party: Rover shared KV is runtime-native and should be treated as core infra, not optional third-party integration.
- Performance-sensitive crates: `rover-server`, parser/type flow paths, and runtime/context handling.
- Critical cross-feature interactions to validate:
  - Compression x streaming x cache validators.
  - Trust proxy x rate-limit keying x request ID propagation.
  - Session/rate-limit x distributed store outages.
  - Graceful shutdown x long-lived connections (SSE/stream/ws/jobs).

### DSL Samples (for implementation guidance)

```lua
local api = rover.server {
    compress = {
        enabled = true,
        algorithms = { "br", "gzip" },
        min_size = 1024,
    },
    timeout = {
        header_ms = 3000,
        body_ms = 10000,
        request_ms = 30000,
    },
    limits = {
        max_connections = 10000,
        max_in_flight = 2000,
        max_header_bytes = 16384,
    },
}

function api.healthz.get(ctx)
    return api.json { status = "ok" }
end

function api.readyz.get(ctx)
    local ok = rover.runtime.ready()
    if not ok then
        return api.json:status(503, { status = "not_ready" })
    end
    return api.json { status = "ready" }
end

function api.chat.sse(ctx, stream)
    local body = ctx:body():json()

    stream:keepalive(15000)
    stream:retry(5000)
    stream:event("start", { prompt = body.prompt })

    for chunk in generate_tokens(body.prompt) do
        stream:event("token", { text = chunk })
    end

    stream:event("done", { ok = true })
end

function api.chat.ws(ws)
    function ws.open(ctx)
        ctx:set("user_id", ctx:query().user_id or "anon")
        ws.subscribe(ctx, "room:lobby")

        ws.send.welcome(ctx, {
            user_id = ctx:get("user_id"),
            timestamp = os.time(),
        })
    end

    function ws.message.chat(ctx, msg)
        ws.broadcast.chat("room:lobby", {
            user_id = ctx:get("user_id"),
            text = msg.text,
            timestamp = os.time(),
        })
    end

    function ws.message.typing(ctx, msg)
        ws.broadcast.typing("room:lobby", {
            user_id = ctx:get("user_id"),
        }, {
            exclude_self = true,
        })
    end

    function ws.message(ctx, msg)
        ws.send.error(ctx, {
            error = "unknown_event",
            type = msg.type,
        })
    end

    function ws.close(ctx)
        ws.broadcast.user_left("room:lobby", {
            user_id = ctx:get("user_id"),
            timestamp = os.time(),
        }, {
            exclude_self = true,
        })
    end
end

function api.auth.middleware(ctx)
    local header = ctx:headers().Authorization
    if not header then
        return api.json:status(401, { error = "missing auth" })
    end
end

function api.upload.post(ctx)
    local file = ctx:body():file("asset")
    if not file then
        return api.json:status(400, { error = "asset file required" })
    end
    return api.json:status(201, { name = file.name, size = file.size })
end
```

## 8. Success Metrics

- Foundation completion: all P1-P5 backend runtime tickets are either merged or replaced by equivalent merged work with explicit traceability.
- Performance baseline on reference hardware:
  - p95 request latency does not regress by more than 10% after enabling core production features.
  - Compression reduces response egress bytes by at least 40% on representative JSON payload benchmark.
  - Runtime remains stable under overload test with no crash and predictable shed behavior.
- Reliability baseline:
  - Graceful shutdown completes in configured timeout with zero dropped completed responses in controlled test.
  - Readiness flips correctly during dependency outage simulation.
- DX baseline:
  - Every major backend capability has one runnable Lua DSL example and one test path.

## 9. Open Questions

- Should brotli (`br`) be enabled by default in production presets or only opt-in initially?
- How should Rover shared KV handle cross-instance consistency mode at GA (strong vs eventual) by default?
- Should built-in background jobs remain best-effort only, or include at-least-once durability mode in later phase?
- Which API versioning scheme is default: URI (`/v1`) or header-based negotiation?
