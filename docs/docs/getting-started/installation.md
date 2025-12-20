---
sidebar_position: 1
---

# Installation

Get Rover up and running on your system.

## Prerequisites

- Rust toolchain (cargo, rustc)
- Git (for cloning the repository)

## Build from Source

Clone the repository and build:

```bash
git clone https://github.com/your-org/rover.git
cd rover
cargo build --release
```

The compiled binary will be available at `./target/release/rover`.

## Running Your First App

Create a Lua file (e.g., `app.lua`):

```lua
local api = rover.server { }

function api.hello.get(ctx)
    return { message = "Hello from Rover!" }
end

return api
```

Run it:

```bash
./target/release/rover app.lua
```

Your server is now running at `http://localhost:4242`!

## Next Steps

Continue to the [Backend Server Guide](/guides/backend-server) to learn more about building APIs with Rover.
