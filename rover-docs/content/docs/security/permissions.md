---
weight: 8
title: Permissions
aliases:
  - /docs/security/permissions/
  - /docs/server/permissions/
---

Configure capability permissions for Lua runtime behavior.

## Overview

Set `permissions` in `rover.server { ... }` to control runtime capabilities:

- `fs`
- `net`
- `env`
- `process`
- `ffi`

Modes:

- `development` (`dev`): `fs`, `net`, `env` allowed by default
- `production` (`prod`): deny-by-default

`deny` always wins over `allow`.

## Basic Configuration

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env" },
    },
}

return api
```

Enable process execution only when required:

```lua
local api = rover.server {
    permissions = {
        mode = "production",
        allow = { "env", "process" },
    },
}

function api.now.get(ctx)
    local pipe = io.popen("date", "r")
    local out = pipe:read("*a")
    pipe:close()
    return api.text(out)
end

return api
```

## Validation

Startup validation rejects invalid config:

- invalid permission names
- non-array `allow`/`deny`
- same permission in both `allow` and `deny`

Example invalid config:

```lua
local api = rover.server {
    permissions = {
        allow = { "invalid_perm" },
    },
}
```

## Current Enforcement Boundary

Current enforcement is intentionally narrow and explicit:

- `process`: enforced for child process execution (`io.popen`)
- `fs`, `net`, `env`, `ffi`: parsed and validated in config; finer runtime enforcement is not yet complete

Denied operations emit typed errors and audit events.

## Runnable Example

- `examples/permissions_example.lua`

Run:

```bash
cargo run -p rover_cli -- run examples/permissions_example.lua
```

## Related

- [Configuration](/docs/server/configuration/)
- [Auth](/docs/security/auth/)
- [Production Deployment](/docs/operations/production-deployment/)
