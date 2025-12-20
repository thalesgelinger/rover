---
sidebar_position: 1
---

# Introduction

Rover is an opinionated Lua runtime for building full-stack applications. Write backends, frontends (coming soon), and everything in between using Lua's speed and simplicity.

Rover is an all-in-one tool that maximizes Lua's power - not a framework, but a complete runtime with batteries included.

## What's Included

- ✅ **Backend Server**: HTTP server with built-in routing
- 🚧 **UI Framework**: Native UI components (coming soon)
- 🔧 **Zero Config**: Opinionated defaults that just work

## Quick Start

Build and run:

```bash
cargo build --release
./target/release/rover your_app.lua
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

Run it and visit `http://localhost:4242/hello` to see your API in action!

## Next Steps

- [Installation](/getting-started/installation) - Set up Rover
- [Backend Server Guide](/guides/backend-server) - Build your first API
- [Context API](/guides/context-api) - Access request data
- [Response Builders](/guides/response-builders) - Return structured responses
