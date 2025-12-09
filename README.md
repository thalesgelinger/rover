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

### Skia (Phase 2) ✅

CPU raster rendering with Skia is working!

**Quick Start:**
See [docs/SKIA_SETUP.md](docs/SKIA_SETUP.md) for:
- Downloading prebuilds (5 min)
- Building from source (60+ min)

**Build with Skia:**
```bash
zig build -Dwith-skia=true test
```

Test output creates `/tmp/rover_skia_test_output.png` - a 64x64 black canvas with red rectangle.

### What works

- ✅ CLI argument parsing
- ✅ Lua file loading and execution
- ✅ `rover.app()` creates app table in Lua
- ✅ Lua standard library available
- ✅ Skia 2D rendering (raster CPU)
- ✅ Canvas drawing (clear, drawRect)
- ✅ PNG export

### Next steps

- Phase 3: macOS window backend
- Phase 4: Rover UI components (col, row, text, button)
- Phase 5+: Layout engine, state management, event handling
