---
sidebar_position: 8
---

# CLI

Rover ships a single `rover` binary with subcommands.

## Global Flags

- `-v, --verbose` enables verbose logs for all subcommands.

## Help

```bash
rover --help
rover <subcommand> --help
```

## Run

```bash
rover run path/to/app.lua
```

Optional:

- `-p, --platform <platform>` selects runtime backend.
- `-y, --yolo` skips prompts.
- `-- <args...>` forwards extra args to your app.

Supported `run` platforms:

- `stub`
- `tui`
- `web`
- `ios`
- `android`
- `macos`
- `windows`
- `linux`

## Check

```bash
rover check path/to/app.lua
```

Optional:

- `-v, --verbose` for extra diagnostic output.
- `-f, --format <pretty|json>` output format (`pretty` default).

## Format

```bash
rover fmt path/to/app.lua
rover fmt --check
```

## Build

```bash
rover build path/to/app.lua --out my-app
```

Optional:

- `-t, --target <target>` build target.

## DB

```bash
rover db migrate
rover db rollback --steps 1
rover db status
rover db reset --force
```

Each DB action supports optional `--database <path>` and `--migrations <path>`.

`rollback` also supports `--steps <n>`.

## LSP

```bash
rover lsp
```

LSP exists but needs more features and polish.

## Script Shortcuts (`rover.lua`)

If `rover.lua` defines a `scripts` table, you can run scripts directly:

```lua
return {
  scripts = {
    dev = "rover run app.lua -p web",
    smoke = function()
      print("custom Lua script")
    end,
  },
}
```

Then run:

```bash
rover dev
rover smoke
rover dev -- --port 8080
```

Script values can be shell command strings or Lua functions.
