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

## REPL

```bash
rover repl
rover repl path/to/file.lua
rover repl path/to/dir
rover repl -e '1 + 1'
rover repl path/to/dir -e 'rover'
```

`rover repl` starts a Lua REPL with Rover loaded.

Preload rules:

- File paths execute once before the prompt.
- Directory paths are added to `package.path` as `?.lua` and `?/init.lua`.
- Directory paths auto-run `init.lua` or `main.lua` when present.

REPL commands:

- `.help` shows commands.
- `.load <path>` loads a file or directory.
- `.reload` creates a fresh Lua state and reloads loaded paths.
- `.doc <symbol>` shows Rover API docs.
- `.vars` lists user globals.
- `.clear` clears the terminal.
- `.exit` quits.

Input first tries expression mode, then statement mode. For example, `1 + 1` prints `2`. Multiline input continues automatically for incomplete Lua chunks, or explicitly with trailing `\`.

The REPL does not auto-run returned Rover server/UI apps. It evaluates code and keeps the prompt interactive.

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
