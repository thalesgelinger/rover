# Changelog

## v0.0.1-alpha.1

First alpha release. Prebuilt `rover` CLI binaries for macOS, Linux, and Windows.

### Install

Unix:

```sh
curl -fsSL https://rover.lu/install | sh
```

Windows:

```powershell
irm https://rover.lu/install.ps1 | iex
```

### Included

- CLI: `run`, `check`, `fmt`, `build`, `db`, `lsp`
- Lua backend server with route functions, request context, response helpers, route patterns, config, lifecycle hooks
- Static assets, HTML templates, HTTP client, WebSocket server/client APIs
- Database module, migrations, DB query DSL
- Cookie session auth, guard validation, permissions, idempotency, compression
- IO/debug helpers, OpenAPI/raw server extras
- Signals/reactive primitives, UI runtime foundation, TUI foundation
- macOS/iOS runtime scaffolding, web runtime/build path
- Early LSP server and `rover.lua` script shortcuts

Docs: https://rover.lu/docs
