---
weight: 8
title: CLI
---

Rover ships a single `rover` binary with subcommands.

## Run

```bash
rover run path/to/app.lua
```

Optional:

- `--platform stub` (UI stub renderer)
- `--yolo` (skip migration prompts)

## Check

```bash
rover check path/to/app.lua
```

## Format

```bash
rover fmt path/to/app.lua
rover fmt --check
```

## Build

```bash
rover build path/to/app.lua --out my-app
```

## DB

```bash
rover db migrate
rover db rollback --steps 1
rover db status
rover db reset --force
```

## LSP

```bash
rover lsp
```

LSP exists but needs more features and polish.
