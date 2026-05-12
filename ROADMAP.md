# Rover Roadmap

3 states: TODO, DOING, DONE. Items grouped by feature area.

## ✅ Done

### Getting Started
- Intro (`rover-docs/content/docs/intro.md`)
- Installation (`rover-docs/content/docs/getting-started/installation.md`)

### Server Core
- HTTP server (`rover-docs/content/docs/server/backend-server.md`)
- Configuration (`rover-docs/content/docs/server/configuration.md`)
- Route patterns (`rover-docs/content/docs/server/route-patterns.md`)
- Context API + body helpers (`rover-docs/content/docs/server/context-api.md`)
- Response builders (`rover-docs/content/docs/server/response-builders.md`)
- Server extras (OpenAPI, raw) (`rover-docs/content/docs/server/server-extras.md`)

### HTML Templates
- HTML templating (`rover-docs/content/docs/server/html-templates.md`)

### HTTP Client
- HTTP client (`rover-docs/content/docs/http-and-realtime/http-client.md`)

### Validation
- Guard validation (`rover-docs/content/docs/runtime/guard.md`)

### Database
- Database guide (`rover-docs/content/docs/data/database.md`)
- Migrations (`rover-docs/content/docs/data/migrations.md`)
- Query DSL (`rover-docs/content/docs/data/db-query-dsl.md`)

### IO + Debug
- IO module (`rover-docs/content/docs/runtime/io.md`)
- Debug utilities (`rover-docs/content/docs/runtime/debug.md`)

### Reactive + UI
- Signals guide (`rover-docs/content/docs/runtime/signals.md`)
- Signals API (`rover-docs/content/docs/runtime/signals-api.md`)
- Performance notes (`rover-docs/content/docs/runtime/performance.md`)
- UI runtime (`rover-docs/content/docs/runtime/ui-runtime.md`)

### Tooling
- CLI (`rover-docs/content/docs/operations/cli.md`)

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
