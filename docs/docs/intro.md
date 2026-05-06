---
sidebar_position: 1
---

# Introduction

Rover is an opinionated Lua runtime for building REAL full-stack applications. Write backends, mobile apps, desktop apps, web frontends - everything - using Lua's speed and simplicity.

Rover is an all-in-one tool that maximizes Lua's power across all platforms - not a framework, but a complete runtime with batteries included for web, mobile, and desktop.

## What's Included

- ✅ **Backend Server**: HTTP server with built-in routing
- 🚧 **UI Framework**: Native UI components for mobile, desktop, and web (coming soon)
- 🔧 **Zero Config**: Opinionated defaults that just work
- 🌍 **Cross-Platform**: One codebase for web, mobile (iOS/Android), and desktop

## Quick Start

Build and run:

```bash
cargo build --release
./target/release/rover run your_app.lua
```

## Hello World

Create a simple API server:

```lua
local api = rover.server { }

function api.hello.get(ctx)
    return { message = "Hello World" }
end

return api
```

Run it with `rover run app.lua` and visit `http://localhost:4242/hello`.

## Next Steps

- [Installation](/docs/getting-started/installation) - Set up Rover
- [Backend Server Guide](/docs/guides/backend-server) - Build your first API
- [Context API](/docs/guides/context-api) - Access request data
- [Response Builders](/docs/guides/response-builders) - Return structured responses
