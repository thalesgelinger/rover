# PRD: Foundation Module Completion

## 1. Introduction/Overview

This PRD defines the remaining work needed to finish the Foundation module and move all open Foundation Plane items to `Done` with implementation evidence. The goal is not to expand scope beyond the current Foundation vision, but to close the gap between current code, current documentation, current examples, and the acceptance criteria already implied by the open work items.

Based on current audit, some Foundation capabilities are mostly implemented but still need documentation, runnable examples, and scope alignment before they can be honestly marked complete. Other capabilities are still missing core production-safe MVP behavior and need implementation, tests, docs, and examples.

This PRD covers all seven open Foundation work items:

- `RVR-113` Response Compression
- `RVR-121` Static File Serving
- `RVR-132` Health and Readiness Endpoints
- `RVR-133` Trusted Proxy and Forwarded Headers
- `RVR-145` Capability Permissions Model
- `RVR-149` Idempotency Keys and Safe Retry Semantics
- `RVR-153` HTTP/2 Support and ALPN Negotiation

Delivery target: MVP, production-safe, fully documented in `rover-docs`, with useful runnable examples, and enough evidence to move all covered Plane items to `Done`.

Locked scope decisions for this PRD:

- Response compression MVP remains `gzip` + `deflate` only; Brotli is out of scope for this pass.
- Static file serving will not add directory index support in this pass; scope/docs/Plane wording must align to safe no-directory-listing behavior.
- Permissions MVP must favor honest enforceable boundaries over aspirational fine-grained policy. Coarse but real enforcement is acceptable; fake granular controls are not.
- HTTP/2 MVP must expose minimal rollout controls only: enable/disable switch, ALPN fallback safety, and only small extra safety knobs that are cleanly supported by the implementation.
- Plane completion evidence will live in item comments, with item descriptions kept mostly stable unless scope wording must be corrected.

Final implementation recommendations locked for this PRD:

- Permissions MVP enforcement boundaries for this pass: startup config validation, deny-by-default production mode, runtime gating for currently implemented capability boundaries, structured audit logs on deny, and explicit docs for anything not yet enforced at finer granularity.
- Permissions MVP must not claim per-path, per-host, or per-resource controls unless the runtime can actually enforce them end-to-end in tests.
- HTTP/2 MVP extra knobs default to none beyond explicit enable/disable unless the implementation naturally exposes one small safety control cleanly; avoid speculative operator-facing tuning.
- HTTP/2 delivery priority is correctness of ALPN negotiation, safe HTTP/1.1 fallback, interop coverage, and clear rollout docs over deep configurability.

## 2. Goals

- Complete all seven open Foundation work items with production-safe MVP behavior.
- Ensure every completed Foundation capability has code, tests, `rover-docs` coverage, and runnable examples.
- Align docs and examples with actual runtime behavior so users are not misled by stale APIs or partial features.
- Close mismatches between Plane scope, current implementation, and published documentation.
- Make Plane completion defensible by attaching test/docs/example evidence to each item.

## 3. User Stories

### US-001: Finish response compression behavior
**Description:** As an API consumer, I want standards-compliant response compression so that clients can safely decode responses and operators can reduce bandwidth without breaking cache behavior.

**Acceptance Criteria:**
- [x] Server compression config supports `gzip` + `deflate` for this release, and docs/examples do not claim Brotli support.
- [x] `Accept-Encoding` negotiation is deterministic when multiple supported encodings are configured.
- [x] Responses include correct `Content-Encoding` and `Vary: Accept-Encoding` headers when compression changes response bytes.
- [x] Streaming and SSE responses are not double-compressed.
- [x] Integration tests cover supported encoding success, unsupported encoding fallback, and identity/no-compression cases.
- [x] `rover-docs` includes a dedicated compression page with config examples and behavior notes.
- [x] At least one runnable example demonstrates compression config in a realistic server.
- [x] `cargo test -p rover-server` passes.

### US-002: Finish static file serving delivery
**Description:** As a backend app developer, I want safe static route mounts with documented cache behavior so that I can serve assets or uploads beside API routes without custom handlers.

**Acceptance Criteria:**
- [x] Static mount DSL is documented exactly as implemented.
- [x] Route precedence between API routes and static mounts is documented and covered by tests.
- [x] Path traversal attempts are rejected and covered by tests.
- [x] Cache behavior is documented, including how static mount `cache` maps to `Cache-Control` output.
- [x] Scope mismatch is resolved explicitly by removing directory index support from documented/Plane scope for this release.
- [ ] `rover-docs` includes a runnable static assets example and an uploads/static guide that matches current DSL.
- [ ] At least one runnable example mounts one or more static directories from `examples/`.
- [ ] `cargo test -p rover-core` and relevant `rover-server` tests pass.

### US-003: Align health and readiness docs and examples
**Description:** As a deploy operator, I want health and readiness behavior documented and exemplified accurately so that orchestrators and load balancers use the built-in probes correctly.

**Acceptance Criteria:**
- [x] Built-in `/healthz` and `/readyz` behavior remains documented with exact status code and response body contracts.
- [ ] Readiness dependency failure behavior is documented with structured response examples.
- [ ] Existing examples are updated so they do not redefine built-in probes in misleading ways unless the example is explicitly about overriding behavior.
- [ ] At least one runnable example shows readiness dependency config and expected operational usage.
- [ ] `rover-docs` production/operations pages link clearly to probe behavior and deployment guidance.
- [ ] Integration tests cover healthy, draining, and dependency-failure readiness states.
- [ ] `cargo test -p rover-server --test health_probe_states_integration` passes.

### US-004: Finish trusted proxy delivery
**Description:** As an operator behind a reverse proxy or load balancer, I want trusted proxy configuration and forwarded-header handling documented clearly so that client IP and protocol are derived safely.

**Acceptance Criteria:**
- [ ] Trusted proxy configuration supports the documented production-safe MVP forms already implemented by the runtime.
- [ ] Requests from untrusted sources ignore spoofed forwarded headers.
- [ ] Conflict handling between `Forwarded` and `X-Forwarded-*` headers is deterministic and documented.
- [ ] `rover-docs` includes a dedicated section or page for trusted proxy configuration, trust boundaries, and common deployment examples.
- [ ] At least one runnable example shows a server configured for trusted proxies.
- [ ] Integration tests cover trusted and untrusted proxy permutations and malformed forwarded-header cases.
- [ ] `cargo test -p rover-server --test https_and_proxy_tests` passes.

### US-005: Implement capability permissions MVP
**Description:** As a runtime owner, I want a production-safe capability permissions model so that unsafe capabilities are denied by default and violations are auditable.

**Acceptance Criteria:**
- [ ] Server config supports a production-safe MVP permissions schema covering `fs`, `net`, `env`, `process`, and `ffi`.
- [ ] Production mode is deny-by-default.
- [ ] Startup validation rejects invalid or ambiguous permissions configuration.
- [ ] Runtime permission checks are enforced only at capability boundaries that are actually implemented in this release, and docs clearly state any unsupported granularity.
- [ ] Denied operations return typed errors without leaking secrets or sensitive paths beyond what is explicitly allowed.
- [ ] Denied operations emit structured audit log events.
- [ ] Tests cover FS traversal bypass attempts, NET bypass attempts, and process/child-process behavior for the supported MVP boundaries.
- [ ] `rover-docs` includes a dedicated permissions page with config examples, production guidance, and limitations.
- [ ] At least one runnable example demonstrates restrictive production permissions and one allowed capability path.
- [ ] Targeted tests for permissions pass.

### US-006: Implement idempotency keys MVP
**Description:** As an API developer, I want route-level idempotency support so that retrying write requests does not duplicate side effects in production.

**Acceptance Criteria:**
- [ ] Middleware or equivalent route-level API exists to enable idempotency per route.
- [ ] Idempotency key header name and TTL are configurable per route within documented MVP constraints.
- [ ] Request fingerprint includes method, route identity, and body fingerprint.
- [ ] Duplicate request with same key and same fingerprint replays the original stored response.
- [ ] Same key with different fingerprint returns a conflict response.
- [ ] Concurrent duplicate requests are race-safe.
- [ ] Production-safe shared backend support exists for multi-instance use, and local in-memory mode is clearly limited to dev/test.
- [ ] Startup fails clearly when shared backend is selected but misconfigured.
- [ ] `rover-docs` includes a dedicated idempotency page with usage guidance, replay semantics, conflicts, and storage guidance.
- [ ] At least one runnable example demonstrates idempotent write behavior.
- [ ] Targeted integration tests cover replay, conflict, TTL expiry, and multi-instance/shared-storage semantics.

### US-007: Implement HTTP/2 with safe fallback
**Description:** As a platform operator, I want HTTP/2 over TLS with ALPN and safe fallback to HTTP/1.1 so that modern clients get better transport support without breaking compatibility.

**Acceptance Criteria:**
- [ ] TLS ALPN negotiates `h2` and falls back to `http/1.1` when required.
- [ ] Configuration exposes an explicit HTTP/2 enable/disable switch.
- [ ] Configuration exposes a minimal production-safe MVP control set for HTTP/2 rollout: enable/disable switch, ALPN fallback safety, and only implementation-supported extra limits.
- [ ] Representative interop tests cover at least one HTTP/2-capable client path and one fallback path.
- [ ] `rover-docs` includes a dedicated HTTP/2 page or production transport section covering enablement, fallback, compatibility notes, and rollout guidance.
- [ ] At least one runnable example shows TLS config prepared for HTTP/2-capable deployment.
- [ ] Targeted transport tests pass.

### US-008: Move Foundation Plane items to Done with evidence
**Description:** As a project owner, I want each Foundation item closed with evidence so that Plane accurately reflects shipped capability rather than partial progress.

**Acceptance Criteria:**
- [ ] Each of the seven open Foundation Plane items has linked evidence in comments or description updates.
- [ ] Evidence includes code references, test coverage, `rover-docs` path, and example path.
- [ ] Items are moved to `Done` only after acceptance criteria for the corresponding capability are satisfied.
- [ ] Any scope reduction from original Plane wording is documented explicitly before marking the item complete.

## 4. Functional Requirements

- FR-1: The system must complete all seven currently open Foundation items in a way that is production-safe for MVP release.
- FR-2: The system must provide documentation in `rover-docs` for every completed Foundation capability.
- FR-3: The system must provide at least one runnable example for every completed Foundation capability.
- FR-4: The system must ensure documentation matches actual implemented DSL, config shape, response contracts, and runtime behavior.
- FR-5: Response compression must negotiate `gzip` and `deflate` from `Accept-Encoding` deterministically for this MVP release.
- FR-6: Response compression must set `Content-Encoding` and `Vary: Accept-Encoding` correctly whenever response bytes differ by encoding.
- FR-7: Response compression must not double-compress streaming or SSE responses.
- FR-8: Static file serving must prevent path traversal and unsafe absolute path access.
- FR-9: Static file serving must document API-route precedence, cache behavior, the exact static mount DSL, and the lack of directory index support.
- FR-10: Health and readiness must expose built-in `/healthz` and `/readyz` semantics with documented status/body contracts.
- FR-11: Readiness must support dependency-aware failure responses with structured reasons.
- FR-12: Trusted proxy handling must derive client IP and protocol only within configured trust boundaries.
- FR-13: Trusted proxy handling must ignore or safely resolve malformed or spoofed forwarded headers from untrusted sources.
- FR-14: Permissions config must support `fs`, `net`, `env`, `process`, and `ffi` within the defined MVP schema.
- FR-15: Permissions MVP must only promise enforcement at boundaries that are actually implemented and tested in this release.
- FR-16: Permissions production mode must be deny-by-default.
- FR-17: Permission violations must emit structured audit logs.
- FR-18: Idempotency must support per-route configuration of key header and TTL.
- FR-19: Idempotency must fingerprint requests using method, route identity, and body fingerprint.
- FR-20: Idempotency must replay stored responses for safe duplicates and return conflict for key reuse with payload mismatch.
- FR-21: Idempotency must support a production-safe shared backend suitable for multi-instance deployment.
- FR-22: HTTP/2 must support ALPN negotiation with safe HTTP/1.1 fallback.
- FR-23: HTTP/2 rollout must include a compatibility disable switch.
- FR-24: HTTP/2 MVP must avoid a large tuning matrix and expose only minimal implementation-supported rollout controls.
- FR-25: Each completed capability must have targeted tests, and all targeted checks must pass before related Plane items move to `Done`.
- FR-26: Plane updates must include explicit completion evidence for each item, stored in Plane comments.

## 5. Non-Goals (Out of Scope)

- No redesign of Foundation beyond current seven open items.
- No large refactor of unrelated server/runtime internals solely for elegance.
- No expansion into advanced non-MVP permission models or fake fine-grained controls that cannot be enforced safely.
- No advanced idempotency features beyond safe retry semantics, shared backend support, and required production-safe config.
- No HTTP/3 or QUIC work.
- No broad docs-site redesign unrelated to Foundation coverage.
- No moving Plane items to `Done` without evidence.

## 6. Design Considerations

- Docs should favor operational clarity over marketing language.
- Each Foundation doc page should include: what it does, why to use it, config shape, edge cases, and a runnable example path.
- Examples should be small, runnable, and focused on one capability or one coherent deployment pattern.
- Stale examples that override built-in behavior should either be corrected or explicitly labeled.

## 7. Technical Considerations

- Keep edits scoped; avoid unrelated crate boundary changes.
- Prefer targeted crate tests while iterating, then run broader validation before closure.
- If Plane wording does not match implemented MVP behavior, explicitly narrow and document the accepted scope before completion rather than adding risky feature creep.
- Shared idempotency storage must be appropriate for multi-instance production use.
- Permission enforcement must happen at actual runtime boundaries, not just config parse time.
- Permissions docs must clearly separate enforced behavior from future possible granularity.
- HTTP/2 work must preserve current HTTP/1.1 behavior when disabled or when negotiation falls back.
- HTTP/2 should ship with no extra tuning knobs unless one is clearly required and cleanly supported by implementation and tests.
- Docs must live under `rover-docs/content/docs/` and examples under `examples/`.

## 8. Success Metrics

- All seven open Foundation Plane items are moved to `Done` with linked evidence.
- Every Foundation capability covered by this PRD has at least one dedicated `rover-docs` entry and at least one runnable example.
- Targeted tests for each capability pass without regressions in existing HTTP/1.1 behavior.
- No remaining mismatch between documented Foundation behavior and actual shipped runtime behavior for covered features.
- A new contributor can identify how to configure and verify each Foundation feature using only `rover-docs` and `examples/`.

## 9. Open Questions

- None. Scope is locked for implementation.
