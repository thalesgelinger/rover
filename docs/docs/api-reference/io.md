---
sidebar_position: 6
---

# IO Module

Rover overrides Lua `io` with async-friendly file handles.

## Open Files

```lua
local file = io.open("notes.txt", "r")
local line = file:read("*l")
file:close()
```

Supported modes include `r`, `w`, `a`, `r+`, `w+`, `a+` (and binary variants).

## Common Methods

- `file:read(format)` (`*l`, `*L`, `*a`, `*n`, byte counts)
- `file:write(value)`
- `file:flush()`
- `file:close()`
- `file:lines()`
- `file:seek(whence, offset)`

## Global Helpers

- `io.input([file|path])`
- `io.output([file|path])`
- `io.read(format)`
- `io.write(...)`
- `io.flush()`
- `io.type(obj)`
- `io.lines([path])` (iterator)
- `io.popen(command, mode)`
- `io.tmpfile()`

Example iterator:

```lua
for line in io.lines("/tmp/notes.txt") do
  print(line)
end
```

`io.stdin`, `io.stdout`, `io.stderr` are available as handles.
