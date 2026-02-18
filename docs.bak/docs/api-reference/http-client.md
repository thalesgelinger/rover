---
sidebar_position: 3
---

# HTTP Client

Rover exposes a simple HTTP client at `rover.http` for outbound requests.

## Quick Start

```lua
local res = rover.http.get("https://api.example.com/health")
print(res.status, res.ok)
```

## Methods

- `rover.http.get(url, config)`
- `rover.http.post(url, data, config)`
- `rover.http.put(url, data, config)`
- `rover.http.patch(url, data, config)`
- `rover.http.delete(url, config)`
- `rover.http.head(url, config)`
- `rover.http.options(url, config)`
- `rover.http.create(config)`

`data` is JSON-encoded when provided.

## Client Instances

Create a configured client with defaults:

```lua
local client = rover.http.create {
  baseURL = "https://api.example.com",
  timeout = 5000,
  headers = { Authorization = "Bearer token" }
}

local res = client:get("/users")
```

`timeout` is milliseconds.

## Request Config

`config` is an optional table:

- `headers` - table of header key/value pairs
- `params` - table of query parameters

Example:

```lua
rover.http.get("/users", {
  headers = { ["X-Env"] = "dev" },
  params = { page = 2 }
})
```

## Response Shape

All requests return a table:

- `status` - HTTP status code
- `ok` - boolean (true for 2xx)
- `headers` - response headers table
- `data` - parsed JSON table or raw string

## Coroutine Behavior

When called inside a coroutine, requests yield once to allow other tasks to run.
