---
weight: 1
title: Installation
---

Get Rover up and running on your system.

## Install

macOS and Linux:

```bash
curl -fsSL https://rover.lu/install | sh
```

Windows PowerShell:

```powershell
irm https://rover.lu/install.ps1 | iex
```

The installer downloads the latest prebuilt binary from GitHub Releases, installs it to `~/.rover/bin`, and updates your shell PATH when it can. Restart your terminal if `rover` is not found immediately.

## Options

Install a specific version:

```bash
curl -fsSL https://rover.lu/install | ROVER_VERSION=v0.0.1-alpha.1 sh
```

Use a custom Rover home:

```bash
curl -fsSL https://rover.lu/install | ROVER_HOME="$HOME/tools/rover" sh
```

Skip PATH edits:

```bash
curl -fsSL https://rover.lu/install | ROVER_NO_MODIFY_PATH=1 sh
```

## Build from Source

Prerequisites:

- Rust toolchain (cargo, rustc)
- Git (for cloning the repository)

Clone the repository and build:

```bash
git clone https://github.com/thalesgelinger/rover.git
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
rover run app.lua
```

Your server is now running at `http://localhost:4242`!

## Next Steps

Continue to the [Backend Server Guide](/docs/server/backend-server/) to learn more about building APIs with Rover.
