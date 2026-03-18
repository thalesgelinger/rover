# PRD: Foundation Module Delivery

## 1. Introduction/Overview

Deliver the full `Foundation` module in Plane as a dependency-ordered backend runtime program for Rover. This PRD covers all module items (security, transport, pipeline, resilience, observability, app primitives, realtime, docs/ops guidance) and sequences work to avoid blocker chains.

Problem solved: teams need a secure-by-default backend runtime with predictable behavior, production controls, and complete docs so new services can ship fast with low ops risk.

## 2. Goals

- Complete all Foundation module features with explicit dependency order.
- Keep runtime secure-by-default, with strict mode and fail-fast startup checks.
- Provide robust transport, resilience, and observability for production operation.
- Ship first-party primitives (sessions, cookies, uploads, caching, streaming, docs/versioning).
- Require Rust unit tests for every material behavior and integration tests for cross-feature paths.

## 3. User Stories

### US-001: Finalize Core Pipeline Baseline
**Description:** As a backend developer, I want middleware/routing/validation/error/limits baseline stable so all later features build on deterministic request flow.

**Acceptance Criteria:**
- [x] Middleware pipeline order and short-circuit behavior are deterministic.
- [x] Routing semantics cover 404/405/HEAD/OPTIONS and method fallback rules.
- [x] Validation/content negotiation returns correct 400/406/415 errors.
- [x] Body size limits enforce global and route-level limits.
- [x] Rust unit tests added for middleware ordering, routing edge cases, validation errors, and body limit boundaries.
- [x] Rust integration tests added for end-to-end request lifecycle with middleware + routing + validation.

### US-002: Enforce Strict Security Envelope
**Description:** As a platform owner, I want strict runtime mode and startup fail-fast checks so unsafe production config cannot start.

**Acceptance Criteria:**
- [x] Secure defaults and strict mode implemented with explicit opt-out flags.
- [x] Startup validation blocks invalid/unsafe combinations and prints actionable errors.
- [x] Capability permissions model enforces allow/deny at runtime boundaries.
- [x] HTTP parser hardening blocks smuggling/desync patterns.
- [x] Rust unit tests added for strict mode flags, startup validators, capability policy decisions, and parser hardening cases.
- [x] Rust integration tests added for startup failure matrix and hardened parsing scenarios.

### US-003: Harden HTTP Surface Security
**Description:** As a security engineer, I want headers/auth/secrets/outbound guards so app and platform surfaces are protected by default.

**Acceptance Criteria:**
- [x] Security headers defaults applied with safe override mechanism.
- [x] AuthN/AuthZ middleware helpers support route/group scope and clear deny responses.
- [x] Secrets management supports rotation for signing/encryption keys.
- [x] Outbound HTTP hardening includes SSRF guardrails.
- [x] Admin/management endpoints are isolated and auth-by-default.
- [x] Rust unit tests added for header policies, auth decisions, key rotation behavior, SSRF filtering, and admin guard rules.
- [x] Rust integration tests added for end-to-end protected admin route and outbound request blocking.

### US-004: Deliver Transport Production Readiness
**Description:** As an operator, I want HTTPS, cert ops, HTTP/2, proxy trust, and timeout controls so traffic handling is secure and stable.

**Acceptance Criteria:**
- [x] TLS/HTTPS baseline implemented with sane defaults.
- [x] TLS cert reload supported without unsafe runtime interruption.
- [x] HTTP/2 with ALPN negotiation supported.
- [x] Trusted proxy and forwarded-header parsing honors trust boundaries.
- [x] Timeouts and connection limits defend against slow/stalled clients.
- [x] Rust unit tests added for TLS config parsing, ALPN negotiation logic, proxy chain parsing, timeout policy calculations.
- [x] Rust integration tests added for HTTPS startup, proxy-forwarded request metadata, and timeout enforcement.

### US-005: Add Resilience and Traffic Governance
**Description:** As an SRE, I want load controls and safe retry semantics so spikes and retries do not cause cascade failures.

**Acceptance Criteria:**
- [x] Rate limiting works for global and scoped policies.
- [x] Load shedding/backpressure handles in-flight and queue pressure with deterministic responses.
- [x] Graceful shutdown drains in-flight work before process exit.
- [x] Rust unit tests added for rate keying, shed thresholds, idempotency storage/expiry, shutdown state transitions.
- [x] Rust integration tests added for overload scenarios and graceful drain under active traffic.

### US-006: Establish Observability Contract
**Description:** As an operator, I want request IDs, logs, traces, metrics, and health/readiness endpoints so runtime health is debuggable.

**Acceptance Criteria:**
- [x] Request ID and correlation propagated across request lifecycle.
- [x] Structured request logging supports redaction and slow-request fields.
- [x] Metrics/tracing exported with stable field names and tags.
- [x] Health/readiness endpoints expose correct liveness/readiness semantics.
- [x] Rust unit tests added for request-id propagation, log field serialization, metric label generation, readiness state logic.
- [x] Rust integration tests added for complete telemetry emission on request flow and dependency outage readiness behavior.

### US-007: Provide Shared Runtime State Layer
**Description:** As a runtime developer, I want distributed store abstraction so session/rate/idempotency can scale beyond single instance.

**Acceptance Criteria:**
- [x] Distributed runtime stores support memory baseline and pluggable external backend.
- [x] TTL, namespacing/prefixing, and error/fallback semantics are explicit.
- [x] Store interface integrates cleanly with session and rate-limiter modules.
- [x] Rust unit tests added for TTL expiry, namespace isolation, and adapter behavior.
- [x] Rust integration tests added for multi-instance consistency paths (where backend supports it).

### US-008: Implement Session and Cookie Primitives
**Description:** As an application developer, I want secure cookie/session APIs so auth flows and user state are easy and safe.

**Acceptance Criteria:**
- [x] Cookie API supports parse/set/delete with secure attributes.
- [x] Session management supports secure cookie sessions with pluggable store backend.
- [x] Session lifecycle supports creation, refresh, invalidation, and expiry.
- [x] Rust unit tests added for cookie parse/serialize edge cases and session lifecycle transitions.
- [x] Rust integration tests added for login-style session issuance, persistence, and invalidation.

### US-009: Support Uploads and Static Asset Serving
**Description:** As an app developer, I want multipart upload and static file serving so common web workloads are first-class.

**Acceptance Criteria:**
- [x] Multipart parser supports file + field extraction with size/count/type controls.
- [x] Static file serving prevents traversal and supports common cache behavior.
- [x] API and static routes can coexist without routing ambiguity.
- [x] Rust unit tests added for multipart boundary parsing, limit violations, and path normalization.
- [x] Rust integration tests added for real multipart upload flow and static route serving behavior.

### US-010: Add Response Optimization Features
**Description:** As an API consumer, I want compression and conditional caching so responses are efficient and bandwidth is reduced.

**Acceptance Criteria:**
- [x] Response compression negotiates by `Accept-Encoding` with safe defaults.
- [x] Caching/conditional request support includes ETag/Last-Modified and 304 behavior.
- [x] `Vary` and related headers remain correct when compression and cache validators combine.
- [x] Rust unit tests added for encoding negotiation, validator precedence, and header composition.
- [x] Rust integration tests added for compressed + conditional response paths.

### US-011: Ship Streaming and Realtime Data Paths
**Description:** As a product engineer, I want streaming responses and SSE so long-running and live update APIs are supported.

**Acceptance Criteria:**
- [x] Streaming responses support chunked/progressive delivery with backpressure-safe writes.
- [x] SSE supports event stream format, reconnect hints, and disconnect cleanup.
- [x] Behavior under cancellation and shutdown is defined and stable.
- [x] Rust unit tests added for frame formatting and stream lifecycle transitions.
- [x] Rust integration tests added for long-running stream cancellation/reconnect paths.

### US-012: Finalize WebSocket Runtime Guidance
**Description:** As a developer, I want clear WebSocket DSL docs and production guidance so implementation is consistent and maintainable.

**Acceptance Criteria:**
- [x] WebSocket docs define lifecycle hooks, event dispatch, and error handling.
- [x] DSL guide includes canonical patterns and anti-patterns.
- [x] Guide maps behavior to tested runtime semantics.
- [x] Rust unit tests added for WebSocket event dispatch contracts that docs rely on.
- [x] Rust integration tests added for connect/message/close flows matching documented behavior.

### US-013: Add OpenAPI and API Versioning
**Description:** As an API maintainer, I want generated OpenAPI and explicit versioning so external integrations are stable.

**Acceptance Criteria:**
- [x] OpenAPI generation reflects route schemas and response contracts.
- [x] API versioning/deprecation strategy is explicit and enforceable.
- [x] Generated docs stay deterministic across repeated builds with same inputs.
- [x] Rust unit tests added for schema generation and version route resolution.
- [x] Rust integration tests added for serving OpenAPI docs and versioned endpoints.

### US-014: Provide Runtime Lifecycle and Job Controls
**Description:** As an operator, I want lifecycle controls and bounded background jobs so maintenance and async work are safe.

**Acceptance Criteria:**
- [x] Server lifecycle controls define startup/shutdown/reload behavior.
- [x] Hot reload behavior documented and constrained to safe scope.
- [x] Background jobs support bounded concurrency and cancellation.
- [x] Rust unit tests added for lifecycle state machine and job scheduler limits.
- [x] Rust integration tests added for reload/shutdown interactions with in-flight jobs.

### US-015: Publish Deployment Topology Guidance
**Description:** As an infrastructure engineer, I want deployment guidance so Rover can be operated consistently across environments.

**Acceptance Criteria:**
- [x] Guidance covers LB/proxy/TLS termination, scale patterns, and logging/tracing setup.
- [x] Guidance references health/readiness semantics and strict mode requirements.
- [x] Guidance includes minimum production checklist and rollback checklist.
- [x] Docs checks pass for touched docs (`docs` build/typecheck if modified).

## 4. Functional Requirements

- FR-1: System must provide deterministic middleware pipeline composition and execution.
- FR-2: System must provide deterministic routing/method semantics, including 404/405/HEAD/OPTIONS behavior.
- FR-3: System must validate inputs and enforce content negotiation rules with explicit error contracts.
- FR-4: System must enforce body size limits globally and per route/group.
- FR-5: System must apply CORS policy with explicit allow-list configuration.
- FR-6: System must apply secure HTTP response headers by default.
- FR-7: System must provide centralized error handling middleware.
- FR-8: System must load environment and file config with deterministic precedence.
- FR-9: System must provide authn/authz middleware primitives.
- FR-10: System must enforce runtime capability permissions.
- FR-11: System must implement secure defaults and strict runtime mode.
- FR-12: System must fail fast on unsafe/invalid startup config.
- FR-13: System must harden HTTP parser against smuggling/desync classes.
- FR-14: System must provide secrets management and key rotation primitives.
- FR-15: System must harden outbound HTTP behavior and block SSRF patterns.
- FR-16: System must isolate and harden admin/management endpoints.
- FR-17: System must provide TLS/HTTPS baseline support.
- FR-18: System must support runtime TLS certificate reload.
- FR-19: System must support HTTP/2 and ALPN negotiation.
- FR-20: System must parse trusted proxy forwarded headers with trust-boundary rules.
- FR-21: System must enforce timeout and connection limit controls.
- FR-22: System must provide request ID generation/propagation for correlation.
- FR-23: System must emit structured request logs.
- FR-24: System must emit metrics and tracing data for core request lifecycle.
- FR-25: System must expose health and readiness endpoints with dependency-aware readiness.
- FR-26: System must provide rate limiting (global + scoped).
- FR-27: System must provide load shedding/backpressure controls.
- FR-28: System must support idempotency keys and retry-safe write semantics.
- FR-29: System must support graceful shutdown and request draining.
- FR-30: System must provide distributed runtime stores with pluggable backends.
- FR-31: System must provide cookie handling primitives.
- FR-32: System must provide secure session management primitives.
- FR-33: System must support multipart/file uploads with bounded limits.
- FR-34: System must support static file serving with traversal protection.
- FR-35: System must support response compression negotiation.
- FR-36: System must support caching and conditional request semantics.
- FR-37: System must support streaming responses.
- FR-38: System must support Server-Sent Events.
- FR-39: System must provide documented WebSocket DSL semantics and behavior contracts.
- FR-40: System must generate OpenAPI specifications from route definitions.
- FR-41: System must provide API versioning/deprecation mechanisms.
- FR-42: System must provide server lifecycle controls and safe hot reload behavior.
- FR-43: System must provide bounded background job primitives.
- FR-44: System must provide deployment topology guidance for production use.

## 5. Non-Goals (Out of Scope)

- Building a full external IAM product (user directory, SSO admin UI, org management UI).
- Building guaranteed durable distributed queue/workflow engine in this phase.
- Shipping every possible external store/provider at first release.
- Re-architecting unrelated crates not required for Foundation features.
- Frontend product UI work beyond docs updates for runtime guidance.

## 6. Design Considerations

- Keep API behavior explicit and deterministic; avoid hidden magic paths.
- Keep security defaults strict; require explicit opt-out for weaker behavior.
- Maintain compatibility with existing Rover crate boundaries and runtime abstractions.
- Prefer composable middleware/policies over feature-specific one-off hooks.

## 7. Technical Considerations

- Primary crates impacted: `rover-server`, `rover-runtime`, `rover-core`, `rover-cli`, `rover-openapi`, plus docs.
- Cross-feature test matrix is mandatory for: compression x caching, proxy x rate-limits, sessions x distributed store, graceful-shutdown x streaming/SSE/jobs.
- Testing baseline per story:
  - Rust unit tests for feature-local logic and edge cases.
  - Rust integration tests for end-to-end behavior across crate boundaries.
- Delivery order (blocker-aware):
  1. Core pipeline baseline (US-001)
  2. Strict security envelope (US-002)
  3. HTTP surface security (US-003)
  4. Transport readiness (US-004)
  5. Resilience controls (US-005)
  6. Observability contract (US-006)
  7. Shared state + session/cookie primitives (US-007, US-008)
  8. Upload/static + compression/caching (US-009, US-010)
  9. Streaming/SSE + websocket docs alignment (US-011, US-012)
  10. OpenAPI/versioning + lifecycle/jobs + deployment guidance (US-013, US-014, US-015)

## 8. Success Metrics

- 100% Foundation module items have implemented, tested, and documented outcomes tied to FR-1..FR-44.
- Critical security controls enabled by default in production presets.
- No P0/P1 regressions in request correctness on integration suite.
- Integration suite covers at least all cross-feature matrices listed in Technical Considerations.
- Runtime passes production readiness checklist (health/readiness, observability, graceful drain, strict startup checks).

## 9. Open Questions

- Should strict mode be default in all environments or only production profile?
- Which distributed store backend is first non-memory target for GA baseline?
- Should HTTP/2 be default-on where TLS/ALPN available, or opt-in first?
- Should WebSocket DSL guide be normative spec with compatibility guarantees across minor versions?

## 10. Plane Traceability Matrix

Use this mapping for execution tracking. Every implementation PR should reference both `US-xxx` and Plane issue IDs.

### US-001: Finalize Core Pipeline Baseline
- `219981c3-8295-451b-b21a-fe298d1b76f8` - Middleware System
- `cdabd8fc-0af9-4b81-b027-18eb44aa8770` - Routing Semantics and Method Handling
- `54adb583-41bf-4c41-86d5-bfb328732a90` - Input Validation and Content Negotiation
- `dababde3-6aac-4834-828e-d882a4cbfe1e` - Error Handling Middleware
- `cac90c89-8db8-4384-9700-4229018feef2` - Body Size Limits
- `949edd49-f010-4459-bbdc-b5be9426bd5e` - CORS
- `496ad637-9a17-4d7e-a8a5-bde2a963e4b2` - Environment and Config

### US-002: Enforce Strict Security Envelope
- `dc23e12b-68eb-40b6-9769-00ff5021b498` - Secure Defaults and Strict Runtime Mode
- `da4ee001-6209-4bff-a507-380caf4108f3` - Startup Validation and Fail-Fast Checks
- `79e1dd5e-01b0-46b4-95ae-cb36e62ea04c` - Capability Permissions Model
- `3cd4db9a-09f4-4d56-b9bb-e04a52631fdd` - HTTP Parser Hardening and Smuggling Defense

### US-003: Harden HTTP Surface Security
- `00c02797-0b18-496f-8df0-9d556786a2bf` - Security Headers
- `14acd53d-e860-463f-9dbf-cf9e328f48f3` - Authentication and Authorization
- `d76a3dec-e1cc-42a3-b775-5ddebb1cd784` - Secrets Management and Key Rotation
- `de12d392-7baa-4f18-a3ba-1b6432191a42` - Outbound HTTP Hardening and SSRF Guards
- `2e9da6ac-0edc-48e3-88be-18b939dd9d45` - Admin and Management Endpoint Hardening

### US-004: Deliver Transport Production Readiness
- `b17675c8-d7c4-4454-b533-5609cde50421` - TLS and HTTPS
- `3a434faa-91f6-4225-8e38-3368d0ee3b2c` - TLS Cert Reload and HTTPS Operations
- `b1721047-d064-4616-a45e-9c9a8cde2e4c` - HTTP/2 Support and ALPN Negotiation
- `9a0ccc6f-9094-4846-b43e-2125f982e008` - Trusted Proxy and Forwarded Headers
- `8436e4f6-5bbf-45fe-b753-996f3144d197` - Timeouts and Connection Limits

### US-005: Add Resilience and Traffic Governance
- `b78425cc-d38b-482d-8d2a-8073dfbbeb83` - Rate Limiting
- `ba3778fa-056e-4fbf-b8f4-de106e47935e` - Load Shedding and Backpressure
- `c06bbac0-fabe-4246-a523-8fb53581178c` - Idempotency Keys and Safe Retry Semantics
- `7230e7ef-1a36-4276-b06a-659fe53053b8` - Graceful Shutdown

### US-006: Establish Observability Contract
- `84005047-c7bc-40f7-9d6c-e237db10963c` - Request ID and Correlation
- `e8fb25a0-3c4c-40c4-b1b4-722e884132ea` - Request Logging Structured Format
- `016b6f17-92a3-4c77-8539-0e34a7dbc22e` - Metrics and Tracing
- `565cb947-ea74-42ca-a830-b309644dc021` - Health and Readiness Endpoints

### US-007: Provide Shared Runtime State Layer
- `56e4c79b-4900-4855-bfc9-9e0dfe3730e0` - Distributed Runtime Stores

### US-008: Implement Session and Cookie Primitives
- `c2ff988d-bb0e-4296-a15f-30ea23fb5a9d` - Session Management
- `c3a8f3ed-2c8c-4e7c-a1da-0cb5add7fafa` - Cookie Handling

### US-009: Support Uploads and Static Asset Serving
- `24d0ef52-8a2c-46a2-b9cd-48d7b0edc979` - File Uploads and Multipart
- `f1ceda24-26e0-4880-80db-c6b3f91618d5` - Static File Serving

### US-010: Add Response Optimization Features
- `193c3ca9-826d-4594-a776-0053b05fadeb` - Response Compression
- `0b84211e-c991-4680-8fd2-7eb6ebb3fb5b` - Caching and Conditional Requests

### US-011: Ship Streaming and Realtime Data Paths
- `fa3933af-14e2-4c2d-a65e-05215a650fc2` - Streaming Responses
- `1dd786d0-32d1-470a-8851-3875e590621a` - Server-Sent Events

### US-012: Finalize WebSocket Runtime Guidance
- `39425e0a-3a62-4770-8c14-19b37c0e7dfd` - WebSocket Documentation
- `013c7a46-d188-47a8-9635-9a64d415dc13` - WebSocket DSL Guide

### US-013: Add OpenAPI and API Versioning
- `2ab2953d-c092-4f2e-a963-3c4a237b7072` - OpenAPI and API Versioning

### US-014: Provide Runtime Lifecycle and Job Controls
- `0ceddf0d-75ea-4a03-81f1-4bbce6e98a06` - Server Lifecycle Controls and Hot Reload
- `6db4086b-597e-43e5-ad89-ce228c0ac5ec` - Background Jobs

### US-015: Publish Deployment Topology Guidance
- `cfdaf73f-e581-455e-af1e-d223c3492945` - Deployment Topology Guidance
