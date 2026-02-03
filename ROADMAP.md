# Rover Roadmap

3 states: TODO, DOING, DONE. Items grouped by feature area.

## ✅ Done

### Getting Started
- Intro (`docs/docs/intro.md`)
- Installation (`docs/docs/getting-started/installation.md`)

### Server Core
- HTTP server (`docs/docs/guides/backend-server.md`)
- Configuration (`docs/docs/api-reference/configuration.md`)
- Route patterns (`docs/docs/api-reference/route-patterns.md`)
- Context API + body helpers (`docs/docs/guides/context-api.md`)
- Response builders (`docs/docs/guides/response-builders.md`)
- Server extras (OpenAPI, raw) (`docs/docs/api-reference/server-extras.md`)

### HTML Templates
- HTML templating (`docs/docs/guides/html-templates.md`)

### HTTP Client
- HTTP client (`docs/docs/api-reference/http-client.md`)

### Validation
- Guard validation (`docs/docs/api-reference/guard.md`)

### Database
- Database guide (`docs/docs/guides/database.md`)
- Migrations (`docs/docs/guides/migrations.md`)
- Query DSL (`docs/docs/api-reference/db-query-dsl.md`)

### IO + Debug
- IO module (`docs/docs/api-reference/io.md`)
- Debug utilities (`docs/docs/api-reference/debug.md`)

### Reactive + UI
- Signals guide (`docs/docs/guides/signals.md`)
- Signals API (`docs/docs/api-reference/signals.md`)
- Performance notes (`docs/docs/performance.md`)
- UI runtime (`docs/docs/guides/ui-runtime.md`)

### Tooling
- CLI (`docs/docs/guides/cli.md`)

## 🚧 Doing

### Runtime
- WebSocket DSL docs (PR #29)

### UI Platforms
- TUI renderer crate (PR #28)

## 📋 Todo

### Runtime
- Automated test runner

### UI Core
- Core stabilization + polish (scheduler/events/renderer flow)

### UI Platforms
- Finish TUI renderer + examples
- Web renderer (WASM)
- iOS renderer
- Android renderer
- Desktop renderers (macOS/Windows/Linux)

### Tooling
- LSP refinement (features + stability)
- Package manager
