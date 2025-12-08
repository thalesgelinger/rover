# Rover

A Lua-first cross-platform mobile framework using Skia for rendering and Zig for the native runtime.

## Phase 1: Project Skeleton & CLI

### Building

```bash
zig build
```

### Running

```bash
./zig-out/bin/rover examples/main.lua
```

With platform flag (currently parsed but not used):

```bash
./zig-out/bin/rover examples/main.lua -p ios
./zig-out/bin/rover examples/main.lua --platform android
```

### What works

- ✅ CLI argument parsing
- ✅ Lua file loading and execution
- ✅ `rover.app()` creates app table in Lua
- ✅ Lua standard library available

### Next steps

- Phase 2: Skia integration for rendering
- Phase 3: macOS window backend
- Phase 4: Rover UI components (col, row, text, button)
- Phase 5+: Layout engine, state management, event handling
