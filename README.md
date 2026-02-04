# Rover
<img width="1500" height="500" alt="image" src="https://github.com/user-attachments/assets/5605ca56-530a-4fe5-a719-dd0f862af3ac" />

Opinionated Lua runtime for building REAL full-stack apps. Ship backends, mobile apps, desktop apps, and web frontends with one Lua codebase.

Learn more in the docs: https://rover.uncmplx.com

## Installation

See the install guide for full options:
https://rover.uncmplx.com/docs/getting-started/installation

Build from source:

```bash
cargo build --release
./target/release/rover run path/to/app.lua
```

## Your First Rover App

Create a simple API server:

```lua
local api = rover.server { }

function api.hello.get(ctx)
    return { message = "Hello World" }
end

return api
```

Run it:

```bash
rover run app.lua
```

Visit `http://localhost:4242/hello`.

## What's Included

- ✅ **Backend Server**: HTTP server with built-in routing
- ✅ **Database + Migrations**: schema + migration tooling
- 🚧 **UI Runtime**: native UI components for mobile, desktop, and web
- 🔧 **Zero Config**: opinionated defaults
- 🌍 **Cross-Platform**: one codebase across platforms

## CLI

Rover ships a single `rover` binary with subcommands:

```bash
rover run path/to/app.lua
rover check path/to/app.lua
rover fmt path/to/app.lua
rover build path/to/app.lua --out my-app
rover db migrate
rover lsp
```

Full CLI guide: https://rover.uncmplx.com/docs/guides/cli

## Docs

- Installation: https://rover.uncmplx.com/docs/getting-started/installation
- Backend Server: https://rover.uncmplx.com/docs/guides/backend-server
- Context API: https://rover.uncmplx.com/docs/guides/context-api
- Response Builders: https://rover.uncmplx.com/docs/guides/response-builders
- Database: https://rover.uncmplx.com/docs/guides/database
- Migrations: https://rover.uncmplx.com/docs/guides/migrations
- UI Runtime: https://rover.uncmplx.com/docs/guides/ui-runtime
- Performance: https://rover.uncmplx.com/docs/performance

## Performance

Built for speed with zero-copy response handling:

```
Requests/sec:   182,000
Latency (avg):  0.49ms
Latency (p99):  0.67ms
```

## Roadmap

See `ROADMAP.md`.
